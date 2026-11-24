//! Rastérisation — port de `t3_create`/`t3_resize`/`t3_begin`/`raster` (C-REF).
//!
//! Le `T3Frame` du C est scindé en deux objets (§G.1) :
//! - [`RasterTarget`] : échantillons (`2 x 4` par cellule) + profondeurs `1/z`, privé ;
//! - [`CellBuffer`] : la sortie, exposée.
//!
//! [`Renderer`] les agrège, comme le `Renderer` de `astre-app` (§C.2) mais **sans**
//! simulation ni HUD. Ce qui est contractuel et conservé à l'identique :
//!
//! 1. ordre de `begin` : clamp de `near_z`, base caméra avec les **deux** replis,
//!    `fx`/`fy`, remise à zéro des profondeurs, puis ciel **par échantillon** ;
//! 2. `near_z >= 0.001` et pas de NaN en sortie (§G.3.8) ;
//! 3. `iz = 1/z`, interpolation barycentrique, test strict `iz > depth[i]` (indépendant
//!    de l'ordre de soumission) ;
//! 4. triangles double-face : aire négative -> inversion de deux sommets, rejet si
//!    `|area| < 0.00001` ;
//! 5. bbox en `f32` puis cast `i32` **comme le C** (`max(0, ceil(min - 0.5))`,
//!    `min(limite, floor(max - 0.5))`).
//!
//! Écart assumé avec le C : la grille écrite peut être restreinte à un `viewport`
//! (`CellRect` en cellules), ce que le C ne savait pas faire (il écrivait toujours
//! plein écran). Le viewport complet redonne exactement le comportement d'origine ;
//! la projection, elle, reste calculée sur la grille **entière** (comme le C).

use crate::cell::{BufferError, CellBuffer, CellRect, Mode, ResizePolicy};
use crate::clip;
use crate::math::{camera_basis, clamp_sat, to_view, Camera, CameraBasis, Vec3};
use crate::resolve::resolve_cell;

/// Nombre d'échantillons par cellule en largeur (C-REF : `w = cols * 2`).
pub const SUBSAMPLE_X: i32 = 2;
/// Nombre d'échantillons par cellule en hauteur (C-REF : `h = rows * 4`).
pub const SUBSAMPLE_Y: i32 = 4;
/// Seuil de rejet d'un triangle dégénéré (`0.00001f` du C).
pub const MIN_AREA: f32 = 0.00001;

/// Source de couleur de fond, évaluée **par échantillon** (§A.1.2).
pub trait Sky {
    /// Couleur `0x00RRGGBB` pour un rayon monde **normalisé**.
    fn sample(&mut self, ray: Vec3) -> u32;
}

/// Ciel noir : équivalent du `sky = NULL` du C (échantillons à zéro).
#[derive(Clone, Copy, Debug, Default)]
pub struct BlackSky;

impl Sky for BlackSky {
    #[inline]
    fn sample(&mut self, _ray: Vec3) -> u32 {
        0
    }
}

/// Point projeté en espace écran (échantillons) : `Projected` du C.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projected {
    /// Abscisse en échantillons.
    pub x: f32,
    /// Ordonnée en échantillons.
    pub y: f32,
    /// `1/z` en espace vue (la profondeur comparée est croissante vers la caméra).
    pub iz: f32,
}

/// Couleurs et profondeurs d'une grille d'échantillons `(cols*2) x (rows*4)`.
///
/// Toujours initialisé (§G.5 : le C `malloc`-ait sans initialiser) : `samples` à noir,
/// `depth` à zéro, ce qui signifie « ciel » pour `resolve`.
#[derive(Clone, Debug)]
pub struct RasterTarget {
    w: i32,
    h: i32,
    samples: Vec<u32>,
    depth: Vec<f32>,
}

impl RasterTarget {
    /// Cible pour `cols x rows` cellules, entièrement noire.
    #[must_use]
    pub fn new(cols: i32, rows: i32) -> Self {
        let w = cols * SUBSAMPLE_X;
        let h = rows * SUBSAMPLE_Y;
        let n = (w as usize) * (h as usize);
        Self {
            w,
            h,
            samples: vec![0; n],
            depth: vec![0.0; n],
        }
    }

    /// Largeur en échantillons.
    #[inline]
    #[must_use]
    pub const fn width(&self) -> i32 {
        self.w
    }

    /// Hauteur en échantillons.
    #[inline]
    #[must_use]
    pub const fn height(&self) -> i32 {
        self.h
    }

    /// Couleurs brutes (origne des futurs imports de framebuffer, §G.1
    /// `t3_import_rgb565` : non porté en Phase 1).
    #[inline]
    #[must_use]
    pub fn samples(&self) -> &[u32] {
        &self.samples
    }

    /// Profondeurs brutes (`1/z`, `0` = ciel).
    #[inline]
    #[must_use]
    pub fn depths(&self) -> &[f32] {
        &self.depth
    }

    /// Couleur d'un échantillon (borné).
    #[inline]
    #[must_use]
    pub fn sample_at(&self, x: i32, y: i32) -> Option<u32> {
        self.index(x, y).and_then(|i| self.samples.get(i).copied())
    }

    /// Profondeur d'un échantillon (borné).
    #[inline]
    #[must_use]
    pub fn depth_at(&self, x: i32, y: i32) -> Option<f32> {
        self.index(x, y).and_then(|i| self.depth.get(i).copied())
    }

    #[inline]
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        Some(y as usize * self.w as usize + x as usize)
    }

    fn resize(&mut self, cols: i32, rows: i32) {
        let w = cols * SUBSAMPLE_X;
        let h = rows * SUBSAMPLE_Y;
        let n = (w as usize) * (h as usize);
        if self.w == w && self.h == h {
            return;
        }
        self.w = w;
        self.h = h;
        self.samples.clear();
        self.samples.resize(n, 0);
        self.depth.clear();
        self.depth.resize(n, 0.0);
    }

    /// Remet à zéro profondeurs (et couleurs si `clear_samples`) sur un rectangle
    /// d'échantillons. Le rectangle est supposé déjà borné.
    fn reset_rect(&mut self, r: CellRect, clear_samples: bool) {
        if r.is_empty() {
            return;
        }
        for y in r.y..r.bottom() {
            let start = y as usize * self.w as usize + r.x as usize;
            let end = start + r.w as usize;
            if let Some(slice) = self.depth.get_mut(start..end) {
                for d in slice {
                    *d = 0.0;
                }
            }
            if clear_samples {
                if let Some(slice) = self.samples.get_mut(start..end) {
                    for s in slice {
                        *s = 0;
                    }
                }
            }
        }
    }
}

/// Moteur de rastérisation : cible d'échantillons + [`CellBuffer`] de sortie.
///
/// `Renderer::resize` est un **no-op** sur une taille identique (§G.8.1) : le contenu
/// des deux buffers survit, comme `t3_resize` dans le C.
#[derive(Clone, Debug)]
pub struct Renderer {
    target: RasterTarget,
    cells: CellBuffer,
    camera: Camera,
    basis: CameraBasis,
    fx: f32,
    fy: f32,
    viewport: CellRect,
}

impl Renderer {
    /// Crée un moteur `cols x rows`, grille vide, toutes tuiles sales.
    pub fn new(cols: i32, rows: i32) -> Result<Self, BufferError> {
        let cells = CellBuffer::new(cols, rows)?;
        let target = RasterTarget::new(cols, rows);
        Ok(Self {
            target,
            cells,
            camera: Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), 0.79, 0.15, 0.5),
            basis: camera_basis(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0)),
            fx: 1.0,
            fy: 1.0,
            viewport: CellRect::full(cols, rows),
        })
    }

    /// (Re)dimensionne la cible et la grille de sortie (§C.2, §C.5.3).
    ///
    /// Taille identique = no-op intégral (aucun `epoch++`, contenu conservé).
    pub fn resize(
        &mut self,
        cols: i32,
        rows: i32,
        policy: ResizePolicy,
    ) -> Result<u32, BufferError> {
        let epoch = self.cells.resize(cols, rows, policy)?;
        self.target.resize(self.cells.cols(), self.cells.rows());
        debug_assert_eq!(self.target.width(), self.cells.cols() * SUBSAMPLE_X);
        debug_assert_eq!(self.target.height(), self.cells.rows() * SUBSAMPLE_Y);
        self.viewport = CellRect::full(self.cells.cols(), self.cells.rows());
        Ok(epoch)
    }

    #[inline]
    #[must_use]
    /// Grille de cellules de sortie (lecture).
    pub fn cells(&self) -> &CellBuffer {
        &self.cells
    }

    #[inline]
    /// Grille de cellules de sortie (écriture : HUD, overlays).
    pub fn cells_mut(&mut self) -> &mut CellBuffer {
        &mut self.cells
    }

    #[inline]
    #[must_use]
    /// Cible d'échantillons et de profondeurs (lecture, tests, futurs imports).
    pub fn target(&self) -> &RasterTarget {
        &self.target
    }

    /// Rectangle de cellules effectivement écrit par `begin`/`triangle`.
    #[inline]
    #[must_use]
    pub const fn viewport(&self) -> CellRect {
        self.viewport
    }

    /// `fx`, `fy` courants (précalculés par `begin`) — utiles aux tests et au HUD.
    #[inline]
    #[must_use]
    pub const fn focal(&self) -> (f32, f32) {
        (self.fx, self.fy)
    }

    /// Démarre une image : équivalent exact de `t3_begin` (C-REF).
    ///
    /// `sky = None` reproduit `sky == NULL` (échantillons du viewport à noir).
    pub fn begin<S: Sky + ?Sized>(
        &mut self,
        camera: Camera,
        viewport: CellRect,
        mut sky: Option<&mut S>,
    ) {
        debug_assert!(
            camera.eye.is_finite() && camera.target.is_finite(),
            "caméra non finie en entrée (§G.3.8)"
        );
        let normalized = Camera {
            eye: camera.eye,
            target: camera.target,
            fov_y: camera.fov_y,
            near_z: camera.near_z.max(Camera::NEAR_MIN),
            cell_aspect: camera.cell_aspect,
        };
        self.camera = normalized;
        self.basis = camera_basis(normalized.eye, normalized.target);

        let w = self.target.w;
        let h = self.target.h;
        let fov = clamp_sat(normalized.fov_y, Camera::FOV_MIN, Camera::FOV_MAX);
        self.fy = 0.5 * h as f32 / (fov * 0.5).tan();
        self.fx = self.fy
            / (2.0
                * clamp_sat(
                    normalized.cell_aspect,
                    Camera::ASPECT_MIN,
                    Camera::ASPECT_MAX,
                ));

        self.viewport = viewport.clip(self.cells.cols(), self.cells.rows());
        let srect = self
            .viewport
            .to_samples(SUBSAMPLE_X, SUBSAMPLE_Y)
            .clip(w, h);

        // 1. Profondeurs à zéro sur le viewport (le C le faisait plein écran, §G.5) et
        //    couleurs à noir quand il n'y a pas de ciel : c'est le `memset(samples)`
        //    du C, restreint au viewport (viewpoint complet = comportement d'origine).
        self.target.reset_rect(srect, sky.is_none());
        if srect.is_empty() {
            return;
        }

        let Some(sky) = sky.as_mut() else {
            return;
        };

        // 2. Ciel par échantillon : à 240 x 72 c'est 138 240 évaluations, le premier
        //    poste de coût du moteur (§A.1.2) — ordre des boucles identique au C.
        let basis = self.basis;
        let (fx, fy) = (self.fx, self.fy);
        for sy in srect.y..srect.bottom() {
            let v = (0.5 * h as f32 - sy as f32 - 0.5) / fy;
            let row = sy as usize * w as usize;
            for sx in srect.x..srect.right() {
                let u = (sx as f32 + 0.5 - 0.5 * w as f32) / fx;
                let ray = Vec3::new(
                    basis.forward.x + u * basis.right.x + v * basis.up.x,
                    basis.forward.y + u * basis.right.y + v * basis.up.y,
                    basis.forward.z + u * basis.right.z + v * basis.up.z,
                );
                let color = sky.sample(ray.unit());
                if let Some(slot) = self.samples_mut_at(row + sx as usize) {
                    *slot = color;
                }
            }
        }
    }

    #[inline]
    fn samples_mut_at(&mut self, idx: usize) -> Option<&mut u32> {
        self.target.samples.get_mut(idx)
    }

    /// Soumet un triangle **monde**, double-face, avec la couleur finale de surface.
    ///
    /// Reproduction fidèle : passage en espace vue, clipping plan proche, éventail.
    pub fn triangle(&mut self, a: Vec3, b: Vec3, c: Vec3, color: u32) {
        let eye = self.camera.eye;
        let basis = self.basis;
        let v = [
            to_view(eye, &basis, a),
            to_view(eye, &basis, b),
            to_view(eye, &basis, c),
        ];
        let mut clipped = [Vec3::ZERO; clip::MAX_CLIPPED];
        let n = clip::near_plane(&v, self.camera.near_z, &mut clipped);
        if n < 3 {
            return;
        }
        // Projection recalculée par sommet comme le C (l'éventail rappelle `project`).
        for i in 1..n - 1 {
            let p0 = self.project(clipped[0]);
            let p1 = self.project(clipped[i]);
            let p2 = self.project(clipped[i + 1]);
            match (p0, p1, p2) {
                (Some(p0), Some(p1), Some(p2)) => self.raster(p0, p1, p2, color),
                _ => return,
            }
        }
    }

    /// `project` du C : `x = w/2 + fx·x·iz`, `y = h/2 - fy·y·iz`, `iz = 1/z`.
    #[inline]
    fn project(&self, p: Vec3) -> Option<Projected> {
        if p.z <= 0.0 {
            // Le clipping garantit `z >= near_z >= 0.001` ; garde-fou explicite plutôt
            // qu'une division par zéro silencieuse.
            return None;
        }
        let iz = 1.0 / p.z;
        Some(Projected {
            x: 0.5 * self.target.w as f32 + self.fx * p.x * iz,
            y: 0.5 * self.target.h as f32 - self.fy * p.y * iz,
            iz,
        })
    }

    /// Rastérise un triangle projeté dans le viewport (§C.2, point 4).
    fn raster(&mut self, a: Projected, b: Projected, c: Projected, color: u32) {
        let x_lo = self.viewport.x * SUBSAMPLE_X;
        let x_hi = (self.viewport.right() - 1) * SUBSAMPLE_X + (SUBSAMPLE_X - 1);
        let y_lo = self.viewport.y * SUBSAMPLE_Y;
        let y_hi = (self.viewport.bottom() - 1) * SUBSAMPLE_Y + (SUBSAMPLE_Y - 1);
        if x_lo > x_hi || y_lo > y_hi {
            return;
        }

        let mut area = edge(a, b, c.x, c.y);
        if area.abs() < MIN_AREA {
            return;
        }
        let (mut b, mut c) = (b, c);
        if area < 0.0 {
            core::mem::swap(&mut b, &mut c);
            area = -area;
        }

        let min_x = a.x.min(b.x).min(c.x);
        let max_x = a.x.max(b.x).max(c.x);
        let min_y = a.y.min(b.y).min(c.y);
        let max_y = a.y.max(b.y).max(c.y);

        let x0f = (min_x - 0.5).ceil().max(x_lo as f32);
        let x1f = (max_x - 0.5).floor().min(x_hi as f32);
        let y0f = (min_y - 0.5).ceil().max(y_lo as f32);
        let y1f = (max_y - 0.5).floor().min(y_hi as f32);
        if x0f > x1f || y0f > y1f {
            return;
        }
        // Cast `i32` **comme le C** : les bornes sont des entiers non négatifs exacts.
        let (x0, x1) = (x0f as i32, x1f as i32);
        let (y0, y1) = (y0f as i32, y1f as i32);

        let w = self.target.w as usize;
        for y in y0..=y1 {
            let yc = y as f32 + 0.5;
            let xc0 = x0 as f32 + 0.5;
            let mut w0 = edge(b, c, xc0, yc);
            let mut w1 = edge(c, a, xc0, yc);
            let mut w2 = edge(a, b, xc0, yc);
            let row_start = y as usize * w + x0 as usize;
            let row_len = (x1 - x0 + 1) as usize;
            for i in row_start..row_start + row_len {
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    let iz = (w0 * a.iz + w1 * b.iz + w2 * c.iz) / area;
                    match self.target.depth.get(i) {
                        Some(&d) if iz > d => {
                            if let Some(slot) = self.target.depth.get_mut(i) {
                                *slot = iz;
                            }
                            if let Some(slot) = self.target.samples.get_mut(i) {
                                *slot = color;
                            }
                        }
                        _ => {}
                    }
                }
                w0 += b.y - c.y;
                w1 += c.y - a.y;
                w2 += a.y - b.y;
            }
        }
    }

    /// Convertit les échantillons en cellules ; ne consomme rien (§G.8.3).
    ///
    /// Les tuiles des cellules **effectivement modifiées** sont sales (I7) ; deux
    /// `resolve` identiques laissent donc `dirty_count()` inchangé au second appel.
    pub fn resolve(&mut self, mode: Mode) {
        let cols = self.cells.cols();
        let rows = self.cells.rows();
        let sw = self.target.w as usize;
        for y in 0..rows {
            for x in 0..cols {
                let mut p = [0u32; 8];
                let mut surface = false;
                for j in 0..4usize {
                    let i = (4 * y as usize + j) * sw + 2 * x as usize;
                    if i + 1 >= self.target.samples.len() {
                        continue;
                    }
                    p[2 * j] = self.target.samples[i];
                    p[2 * j + 1] = self.target.samples[i + 1];
                    surface |= self.target.depth[i] > 0.0 || self.target.depth[i + 1] > 0.0;
                }
                let cell = resolve_cell(&p, surface, mode);
                self.cells.set_if_changed(x, y, cell);
            }
        }
    }

    /// `resolve` + `CellBuffer::mark_all_dirty` : équivalent littéral de `t3_resolve`,
    /// qui réécrivait toutes les cellules sans notion de « inchangé ».
    ///
    /// Utilisé par les goldens et par le présentateur ANSI (qui veut toutes les
    /// cellules à jour, §G.8.3).
    pub fn resolve_full(&mut self, mode: Mode) {
        self.resolve(mode);
        self.cells.mark_all_dirty();
    }
}

/// `edge` du C (produit vectoriel 2D orienté).
#[inline]
#[must_use]
pub fn edge(a: Projected, b: Projected, x: f32, y: f32) -> f32 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;
    use crate::math::rgb;

    const RED: u32 = 0x00FA_2214;
    const BLUE: u32 = 0x000A_28DC;

    /// Caméra de `tests/render.c` : oeil à l'origine, visée -Z, aspect 0.5.
    fn test_camera() -> Camera {
        Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), 1.0, 0.5, 0.5)
    }

    fn renderer(cols: i32, rows: i32) -> Renderer {
        match Renderer::new(cols, rows) {
            Ok(r) => r,
            Err(e) => panic!("renderer {cols}x{rows} : {e}"),
        }
    }

    fn face(r: &mut Renderer, z: f32, color: u32) {
        r.triangle(
            Vec3::new(-2.0, -2.0, z),
            Vec3::new(2.0, -2.0, z),
            Vec3::new(0.0, 2.0, z),
            color,
        );
    }

    fn begin_no_sky(r: &mut Renderer, cols: i32, rows: i32) {
        r.begin(
            test_camera(),
            CellRect::full(cols, rows),
            None::<&mut BlackSky>,
        );
    }

    #[test]
    fn target_dimensions_are_two_by_four_per_cell() {
        let r = renderer(40, 20);
        assert_eq!(r.target().width(), 80);
        assert_eq!(r.target().height(), 80);
        assert_eq!(r.target().samples().len(), 80 * 80);
        assert_eq!(r.target().depths().len(), 80 * 80);
        // Tout est initialisé (jamais de « non initialisé », §G.5).
        assert!(r.target().samples().iter().all(|&s| s == 0));
        assert!(r.target().depths().iter().all(|&d| d == 0.0));
    }

    #[test]
    fn depth_order_is_independent_of_submission_order() {
        // Parité `tests/render.c:visibility`.
        for reverse in [false, true] {
            let mut r = renderer(40, 20);
            begin_no_sky(&mut r, 40, 20);
            if reverse {
                face(&mut r, -3.0, BLUE);
                face(&mut r, -2.0, RED);
            } else {
                face(&mut r, -2.0, RED);
                face(&mut r, -3.0, BLUE);
            }
            r.resolve(Mode::Braille);
            let cell = r.cells().get(20, 10).copied().unwrap_or(Cell::BLANK);
            assert_eq!(cell.bg, RED, "reverse={reverse}");
        }
    }

    #[test]
    fn triangle_writes_cells_and_increasing_depth() {
        let mut r = renderer(40, 20);
        begin_no_sky(&mut r, 40, 20);
        r.cells_mut().clear_dirty();
        face(&mut r, -2.0, RED);
        r.resolve(Mode::Braille);
        // Le triangle couvre la cellule centrale et salit des tuiles.
        assert!(r.cells().dirty_count() > 0);
        let centre = r.cells().get(20, 10).copied().unwrap_or(Cell::BLANK);
        assert_ne!(centre.bg, 0, "la surface doit être peinte");

        // Profondeurs strictement croissantes vers la caméra : le plan z = -3 est
        // derrière le plan z = -2, donc `iz(-3) < iz(-2)`.
        let mut r = renderer(40, 20);
        begin_no_sky(&mut r, 40, 20);
        face(&mut r, -3.0, BLUE);
        let far = r.target().depth_at(40, 40);
        face(&mut r, -2.0, RED);
        let near = r.target().depth_at(40, 40);
        match (far, near) {
            (Some(f), Some(n)) => assert!(n > f, "iz proche ({n}) doit dépasser iz loin ({f})"),
            _ => panic!("échantillon central hors grille"),
        }
    }

    #[test]
    fn raster_triangle_fills_only_the_cells_it_covers() {
        // Un triangle minuscule autour du centre : peu de cellules touchées.
        let mut r = renderer(40, 20);
        begin_no_sky(&mut r, 40, 20);
        r.triangle(
            Vec3::new(-0.05, -0.05, -2.0),
            Vec3::new(0.05, -0.05, -2.0),
            Vec3::new(0.0, 0.05, -2.0),
            RED,
        );
        let mut painted = 0;
        for y in 0..r.target().height() {
            for x in 0..r.target().width() {
                if r.target().depth_at(x, y).unwrap_or(0.0) > 0.0 {
                    painted += 1;
                }
            }
        }
        assert!(
            painted > 0,
            "le triangle doit couvrir au moins un échantillon"
        );
        assert!(painted < 200, "le triangle ne doit pas couvrir la grille");
    }

    #[test]
    fn fully_clipped_triangle_leaves_the_image_empty() {
        // Parité `tests/render.c:visibility` : tout derrière near_z -> rien.
        let mut r = renderer(40, 20);
        begin_no_sky(&mut r, 40, 20);
        face(&mut r, -0.2, RED);
        r.resolve(Mode::Braille);
        let cell = r.cells().get(20, 10).copied().unwrap_or(Cell::BLANK);
        assert_eq!(cell.bg, 0, "image attendue noire");
        assert!(r.target().depths().iter().all(|&d| d == 0.0));
    }

    #[test]
    fn partially_clipped_triangle_still_paints_the_centre() {
        // Parité `tests/render.c:visibility` : deux sommets derrière near_z.
        let mut r = renderer(40, 20);
        begin_no_sky(&mut r, 40, 20);
        r.triangle(
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, -1.5),
            RED,
        );
        r.resolve(Mode::Braille);
        assert_eq!(
            r.cells().get(20, 10).copied().unwrap_or(Cell::BLANK).bg,
            RED
        );
    }

    #[test]
    fn farther_surface_does_not_overwrite_a_nearer_one() {
        // Le triangle lointain soumis après ne doit rien changer (test C).
        let mut r = renderer(40, 20);
        begin_no_sky(&mut r, 40, 20);
        r.triangle(
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, -1.5),
            RED,
        );
        r.resolve(Mode::Braille);
        face(&mut r, 1.0, BLUE);
        r.resolve(Mode::Braille);
        assert_eq!(
            r.cells().get(20, 10).copied().unwrap_or(Cell::BLANK).bg,
            RED
        );
    }

    #[test]
    fn degenerate_triangle_is_ignored() {
        let mut r = renderer(20, 10);
        begin_no_sky(&mut r, 20, 10);
        // Aire nulle : trois points alignés.
        r.triangle(
            Vec3::new(-1.0, 0.0, -2.0),
            Vec3::new(0.0, 0.0, -2.0),
            Vec3::new(1.0, 0.0, -2.0),
            RED,
        );
        assert!(r.target().depths().iter().all(|&d| d == 0.0));
        // Aire sous le seuil 1e-5.
        r.triangle(
            Vec3::new(0.0, 0.0, -2.0),
            Vec3::new(1e-4, 0.0, -2.0),
            Vec3::new(0.0, 1e-4, -2.0),
            RED,
        );
        assert!(r.target().depths().iter().all(|&d| d == 0.0));
    }

    #[test]
    fn viewport_restricts_writes() {
        let mut r = renderer(40, 20);
        r.begin(
            test_camera(),
            CellRect::new(18, 8, 4, 4),
            None::<&mut BlackSky>,
        );
        assert_eq!(r.viewport(), CellRect::new(18, 8, 4, 4));
        face(&mut r, -2.0, RED);
        // Aucun échantillon hors du viewport n'est touché.
        let vp = CellRect::new(18, 8, 4, 4).to_samples(SUBSAMPLE_X, SUBSAMPLE_Y);
        let mut inside = 0;
        let mut outside = 0;
        for y in 0..r.target().height() {
            for x in 0..r.target().width() {
                if r.target().depth_at(x, y).unwrap_or(0.0) > 0.0 {
                    if vp.contains(x, y) {
                        inside += 1;
                    } else {
                        outside += 1;
                    }
                }
            }
        }
        assert!(inside > 0, "le viewport doit contenir la surface");
        assert_eq!(outside, 0, "aucune écriture hors viewport");
    }

    #[test]
    fn begin_with_black_sky_and_no_sky_agree() {
        let sky_free = {
            let mut r = renderer(12, 6);
            begin_no_sky(&mut r, 12, 6);
            r.resolve(Mode::Ascii);
            r.cells().cells().to_vec()
        };
        let with_black = {
            let mut r = renderer(12, 6);
            let mut sky = BlackSky;
            r.begin(
                test_camera(),
                CellRect::full(12, 6),
                Some(&mut sky as &mut dyn Sky),
            );
            r.resolve(Mode::Ascii);
            r.cells().cells().to_vec()
        };
        assert_eq!(sky_free, with_black);
    }

    #[test]
    fn resolve_is_idempotent_and_does_not_touch_geometry() {
        let mut r = renderer(24, 12);
        begin_no_sky(&mut r, 24, 12);
        face(&mut r, -2.0, RED);
        let epoch = r.cells().epoch();
        let dims = (r.cells().cols(), r.cells().rows(), r.cells().stride());
        r.resolve(Mode::Ascii);
        let first = r.cells().cells().to_vec();
        r.resolve(Mode::Ascii);
        assert_eq!(r.cells().cells(), first.as_slice());
        assert_eq!(r.cells().epoch(), epoch);
        assert_eq!(
            (r.cells().cols(), r.cells().rows(), r.cells().stride()),
            dims
        );
        // Deux modes différents sur la même image (test `formats` du C).
        r.resolve(Mode::Braille);
        let has_unicode = r.cells().cells().iter().any(|c| c.glyph > 127);
        assert!(has_unicode);
        r.resolve(Mode::Ascii);
        assert!(r
            .cells()
            .cells()
            .iter()
            .all(|c| (32..127).contains(&c.glyph)));
    }

    #[test]
    fn near_z_is_clamped_and_no_nan_escapes() {
        // near_z négatif -> 0.001 comme le C ; pas de NaN dans la sortie.
        let mut r = renderer(16, 8);
        let cam = Camera::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0), 1.0, -5.0, 0.5);
        r.begin(cam, CellRect::full(16, 8), None::<&mut BlackSky>);
        face(&mut r, -2.0, RED);
        r.resolve(Mode::Braille);
        // Aucun glyphe nul, aucun canal hors 24 bits.
        assert!(r
            .cells()
            .cells()
            .iter()
            .all(|c| c.glyph != 0 && c.bg & 0xFF00_0000 == 0));
        // Focal positif et fini.
        let (fx, fy) = r.focal();
        assert!(fx.is_finite() && fy.is_finite() && fx > 0.0 && fy > 0.0);
    }

    #[test]
    fn resize_noop_keeps_content_and_epoch() {
        let mut r = renderer(20, 10);
        r.cells_mut().set(0, 0, Cell::solid(0x41, RED, BLUE));
        let epoch = r.cells().epoch();
        assert_eq!(r.resize(20, 10, ResizePolicy::Clear), Ok(epoch));
        assert_eq!(
            r.cells().get(0, 0).copied().unwrap_or(Cell::BLANK).glyph,
            0x41
        );
        let next = r.resize(22, 11, ResizePolicy::Preserve);
        assert_eq!(next, Ok(epoch + 1));
        assert_eq!(r.target().width(), 44);
        assert_eq!(r.target().height(), 44);
    }

    #[test]
    fn rgb_helper_matches_the_c_rounding() {
        assert_eq!(rgb(300, -3, 42), 0x00FF_002A);
        assert_eq!(rgb(0, 0, 0), 0);
    }

    #[test]
    fn edge_sign_depends_on_winding() {
        let a = Projected {
            x: 0.0,
            y: 0.0,
            iz: 1.0,
        };
        let b = Projected {
            x: 1.0,
            y: 0.0,
            iz: 1.0,
        };
        let c = Projected {
            x: 0.0,
            y: 1.0,
            iz: 1.0,
        };
        assert!(edge(a, b, c.x, c.y) > 0.0);
        assert!(edge(a, c, b.x, b.y) < 0.0);
    }
}
