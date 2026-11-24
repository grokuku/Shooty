//! Géométrie en **pixels device** — la partie du contrat qui sert au futur
//! présentateur (§C.1, §C.3, §E.2).
//!
//! `astre-core` ne fait aucun rendu : il fournit seulement les types que l'application
//! (`astre-app`) calcule et que les présentateurs consomment — terminal (colonnes) ou
//! web (`presenter.ts`, pixels device du canvas, letterbox).
//!
//! Aucun de ces types n'est utilisé par le chemin de rendu de la Phase 1 ; ils sont
//! fournis maintenant parce que le contrat `Presenter`/`SurfaceView` de §C.3 en dépend
//! et que les tests de `fit_letterbox` / `fit_auto_level` (§H.3) s'écriront dessus.

use crate::cell::CellBuffer;

/// Taille en pixels **device** (jamais en pixels CSS : le DPR est déjà appliqué).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelSize {
    /// Largeur en pixels device.
    pub w: u32,
    /// Hauteur en pixels device.
    pub h: u32,
}

impl PixelSize {
    /// Construit une taille en pixels device.
    #[inline]
    #[must_use]
    pub const fn new(w: u32, h: u32) -> Self {
        Self { w, h }
    }

    /// `true` si l'une des dimensions est nulle.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// Nombre de pixels device.
    #[inline]
    #[must_use]
    pub const fn area(&self) -> u64 {
        (self.w as u64) * (self.h as u64)
    }
}

/// Rectangle en pixels device : la zone réellement couverte par la grille dans le
/// canvas (après letterbox), ou une sous-surface future (split-screen).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewport {
    /// Coin haut-gauche de la zone, en pixels device.
    pub origin_px: (i32, i32),
    /// Taille de la zone, en pixels device.
    pub size_px: PixelSize,
}

impl Viewport {
    /// Construit un rectangle en pixels device.
    #[inline]
    #[must_use]
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self {
            origin_px: (x, y),
            size_px: PixelSize::new(w, h),
        }
    }

    /// Zone occupée par `cols x rows` cellules de `cell_px` pixels, à `origin`.
    ///
    /// C'est le calcul du letterbox de §E.2, sans le centrage (qui dépend de la
    /// surface disponible et reste dans `astre-app`).
    #[must_use]
    pub const fn from_cells(origin: (i32, i32), cell_px: PixelSize, cols: i32, rows: i32) -> Self {
        // `max` n'est pas encore `const` : bornage explicite.
        let cols = if cols < 0 { 0 } else { cols } as u32;
        let rows = if rows < 0 { 0 } else { rows } as u32;
        let w = cell_px.w.wrapping_mul(cols);
        let h = cell_px.h.wrapping_mul(rows);
        Self {
            origin_px: origin,
            size_px: PixelSize::new(w, h),
        }
    }

    /// Abscisse du bord droit (exclu).
    #[inline]
    #[must_use]
    pub const fn right(&self) -> i32 {
        self.origin_px.0 + self.size_px.w as i32
    }

    /// Ordonnée du bord bas (exclu).
    #[inline]
    #[must_use]
    pub const fn bottom(&self) -> i32 {
        self.origin_px.1 + self.size_px.h as i32
    }

    /// `true` si la zone est dégénérée.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.size_px.is_empty()
    }

    /// `true` si le pixel device `(x, y)` est dans la zone (bords exclus).
    #[inline]
    #[must_use]
    pub const fn contains_px(&self, x: i32, y: i32) -> bool {
        x >= self.origin_px.0 && y >= self.origin_px.1 && x < self.right() && y < self.bottom()
    }
}

/// Sous-surface présentable : `SurfaceView` de §C.1.
///
/// La v1 n'en produit qu'une, égale à la grille entière ; le présentateur reçoit un
/// `&mut [SurfaceView]` pour que le passage au split-screen ne change pas son API.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceView {
    /// Coin haut-gauche de la zone dans le canvas, en pixels device.
    pub origin_px: (i32, i32),
    /// Taille d'une cellule en pixels device.
    pub cell_px: PixelSize,
}

impl SurfaceView {
    /// Surface unique couvrant la grille d'un `CellBuffer` (cas v1).
    #[must_use]
    pub fn full_grid(cells: &CellBuffer, origin_px: (i32, i32), cell_px: PixelSize) -> Self {
        debug_assert!(cells.cols() >= 1 && cells.rows() >= 1);
        Self { origin_px, cell_px }
    }

    /// Rectangle en pixels device occupé par cette surface pour un buffer donné.
    #[must_use]
    pub fn viewport(&self, cells: &CellBuffer) -> Viewport {
        Viewport::from_cells(self.origin_px, self.cell_px, cells.cols(), cells.rows())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_size_basics() {
        let s = PixelSize::new(0, 10);
        assert!(s.is_empty());
        assert_eq!(PixelSize::new(4, 5).area(), 20);
    }

    #[test]
    fn viewport_from_cells_is_exact() {
        let v = Viewport::from_cells((12, 8), PixelSize::new(6, 12), 40, 20);
        assert_eq!(v.origin_px, (12, 8));
        assert_eq!(v.size_px, PixelSize::new(240, 240));
        assert_eq!(v.right(), 252);
        assert_eq!(v.bottom(), 248);
        assert!(v.contains_px(12, 8));
        assert!(v.contains_px(251, 247));
        assert!(!v.contains_px(252, 247));
        assert!(!v.contains_px(11, 8));
    }

    #[test]
    fn surface_view_matches_the_grid() {
        let cells = match CellBuffer::new(80, 24) {
            Ok(c) => c,
            Err(e) => panic!("buffer : {e}"),
        };
        let s = SurfaceView::full_grid(&cells, (0, 0), PixelSize::new(8, 16));
        let v = s.viewport(&cells);
        assert_eq!(v.size_px, PixelSize::new(80 * 8, 24 * 16));
    }

    #[test]
    fn empty_viewport_never_matches() {
        let v = Viewport::from_cells((0, 0), PixelSize::new(8, 16), 0, 5);
        assert!(v.is_empty());
        assert!(!v.contains_px(0, 0));
    }
}
