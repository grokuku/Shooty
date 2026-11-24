//! `Cell`, `CellRect`, `DirtyTiles`, `CellBuffer` — contrat §C.1 et §C.5.
//!
//! Le C mélangeait tout dans `T3Frame` (cellules + échantillons + profondeurs +
//! caméra). Ici la sortie est un [`CellBuffer`] autonome ; les échantillons et les
//! profondeurs vivent dans [`crate::raster`] (§G.1, ligne `t3_create`/`t3_resize`).
//!
//! Invariants (§C.5.4) :
//! - I1 : `1 <= cols <= 4096`, `1 <= rows <= 4096`, `cols * rows <= 1_000_000` ;
//! - I2 : `stride >= cols` **et** `cells.len() == stride * rows` ;
//! - I3 : les colonnes `cols..stride` sont du padding, jamais lues par un présentateur ;
//! - I4 : `glyph` est un scalaire Unicode valide et jamais `0` ;
//! - I5 : `fg`/`bg` ont leurs 8 bits hauts à zéro ;
//! - I6 : `epoch` est monotone et n'est jamais réutilisé ;
//! - I7 : `resolve` ne touche ni `cols`/`rows`/`stride` ni `epoch` ;
//! - I8 : aucune opération ne lit au-delà de `cols`.

use core::fmt;

/// Dimension maximale par axe — parité `t3_resize` (`term3d.c`).
pub const MAX_DIM: i32 = 4096;
/// Budget de cellules (ACTÉ, §A.4) : garde-fou anti-`malloc`-fou, pas une cible.
pub const MAX_CELLS: u32 = 1_000_000;
/// Largeur d'une tuile sale, en cellules (§A.4 : dirty tiles 8 x 4).
pub const TILE_W: i32 = 8;
/// Hauteur d'une tuile sale, en cellules.
pub const TILE_H: i32 = 4;

/// Bit 0 : souligné (le présentateur terminal peut le rendre, le web l'ignore).
pub const FLAG_UNDERLINE: u16 = 1 << 0;
/// Bit 1 : gras.
pub const FLAG_BOLD: u16 = 1 << 1;
/// Bit 2 : vidéo inverse.
pub const FLAG_REVERSE: u16 = 1 << 2;
/// Bit 3 : glyphe double-largeur (réservé CJK).
pub const FLAG_WIDE: u16 = 1 << 3;

/// Une cellule de la grille : **16 octets**, layout explicite (§A.4, §C.1).
///
/// L'ordre des champs est celui du contrat, pas celui que rustc choisirait : les
/// offsets sont vérifiés par le test `cell_layout_is_16_bytes`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    /// Codepoint Unicode ; `0` n'est jamais produit (I4).
    pub glyph: u32,
    /// `0x00RRGGBB`.
    pub fg: u32,
    /// `0x00RRGGBB`.
    pub bg: u32,
    /// `FLAG_*`.
    pub flags: u16,
    /// Padding explicite : `size_of::<Cell>() == 16`.
    pub _pad: u16,
}

impl Cell {
    /// Glyphe espace, couleurs nulles, aucun flag (`Clear` du C).
    pub const BLANK: Cell = Cell {
        glyph: 0x20,
        fg: 0,
        bg: 0,
        flags: 0,
        _pad: 0,
    };

    #[inline]
    #[must_use]
    /// Construit une cellule ; `_pad` vaut toujours `0` (layout 16 o).
    pub const fn new(glyph: u32, fg: u32, bg: u32, flags: u16) -> Self {
        Self {
            glyph,
            fg,
            bg,
            flags,
            _pad: 0,
        }
    }

    /// Cellule opaque d'un glyphe : pas de flag (cas de `resolve` et du HUD).
    #[inline]
    #[must_use]
    pub const fn solid(glyph: u32, fg: u32, bg: u32) -> Self {
        Self::new(glyph, fg, bg, 0)
    }

    #[inline]
    #[must_use]
    /// Cellule vide (`' '`, couleurs nulles).
    pub const fn blank() -> Self {
        Self::BLANK
    }

    #[inline]
    #[must_use]
    /// `true` si la cellule est exactement [`Cell::BLANK`].
    pub const fn is_blank(&self) -> bool {
        self.glyph == 0x20 && self.fg == 0 && self.bg == 0 && self.flags == 0
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::BLANK
    }
}

/// Mode de résolution des échantillons en glyphes (`T3Mode` du C).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// Glyphes ASCII imprimables (32..127) seulement.
    #[default]
    Ascii,
    /// Braille (U+2800..) et blocs alignés (U+2580..).
    Braille,
}

/// Palette de sortie terminal (`T3Palette` de `terminal.h`).
///
/// Déclarée ici parce que §C.1 la range dans les contrats du cœur ; l'encodeur ANSI
/// qui la consomme appartient à `astre-term` (Phase 2).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Palette {
    /// Vrai couleur (séquences 24 bits).
    #[default]
    Rgb,
    /// Palette 256 couleurs (cube + gris).
    Ansi256,
    /// Glyphes seuls, sans couleur.
    Plain,
}

/// Rectangle en **coordonnées cellules** (`x`, `y` = coin haut-gauche).
///
/// Les champs sont signés : le HUD du C reçoit des offsets négatifs (§G.3.5) et le
/// bornage doit rester local, jamais repoussé chez l'appelant.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CellRect {
    /// Abscisse du coin haut-gauche, en cellules.
    pub x: i32,
    /// Ordonnée du coin haut-gauche, en cellules.
    pub y: i32,
    /// Largeur, en cellules (négative ou nulle = rectangle vide).
    pub w: i32,
    /// Hauteur, en cellules (négative ou nulle = rectangle vide).
    pub h: i32,
}

impl CellRect {
    #[inline]
    #[must_use]
    /// Construit un rectangle de cellules.
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// Grille entière de `cols` x `rows` cellules.
    #[inline]
    #[must_use]
    pub const fn full(cols: i32, rows: i32) -> Self {
        Self::new(0, 0, cols, rows)
    }

    #[inline]
    #[must_use]
    /// `true` si la largeur ou la hauteur est nulle/négative.
    pub const fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// `x + w`, saturé (le C débordait silencieusement).
    #[inline]
    #[must_use]
    pub const fn right(&self) -> i32 {
        self.x.saturating_add(self.w)
    }

    /// `y + h`, saturé.
    #[inline]
    #[must_use]
    pub const fn bottom(&self) -> i32 {
        self.y.saturating_add(self.h)
    }

    /// Surface en cellules (0 si vide).
    #[inline]
    #[must_use]
    pub fn area(&self) -> u64 {
        if self.is_empty() {
            0
        } else {
            (self.w as u64) * (self.h as u64)
        }
    }

    #[inline]
    #[must_use]
    /// `true` si `(x, y)` est à l'intérieur (bord droit/bas exclus).
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }

    /// Intersection ; rectangle vide si disjoints.
    #[inline]
    #[must_use]
    pub fn intersect(&self, o: Self) -> Self {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        Self::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }

    /// Bornage à la grille `cols` x `rows`.
    #[inline]
    #[must_use]
    pub fn clip(&self, cols: i32, rows: i32) -> Self {
        self.intersect(Self::full(cols, rows))
    }

    /// Passage cellules -> échantillons (facteur `sx` en largeur, `sy` en hauteur).
    #[inline]
    #[must_use]
    pub fn to_samples(&self, sx: i32, sy: i32) -> Self {
        Self::new(self.x * sx, self.y * sy, self.w * sx, self.h * sy)
    }

    /// Rectangle union (ignore les vides). Sert au `dirty_bbox`.
    #[inline]
    #[must_use]
    pub fn union(&self, o: Self) -> Self {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return *self;
        }
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        let x1 = self.right().max(o.right());
        let y1 = self.bottom().max(o.bottom());
        Self::new(x0, y0, x1 - x0, y1 - y0)
    }
}

/// Erreurs de dimensionnement — jamais de `panic` sur une entrée hors bornes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BufferError {
    /// `cols` ou `rows` hors de `1..=MAX_DIM`.
    Dimension {
        /// Largeur demandée (refusée).
        cols: i32,
        /// Hauteur demandée (refusée).
        rows: i32,
    },
    /// `cols * rows` (ou `stride * rows`) dépasse [`MAX_CELLS`].
    Budget {
        /// Nombre de cellules demandé.
        cells: u32,
        /// Plafond autorisé ([`MAX_CELLS`]).
        max: u32,
    },
    /// `stride < cols`.
    Stride {
        /// Stride demandé.
        stride: i32,
        /// Largeur logique qu'il devait couvrir.
        cols: i32,
    },
    /// La surface ne tient pas dans un `usize` (cible 32 bits).
    Overflow,
}

impl fmt::Display for BufferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dimension { cols, rows } => write!(
                f,
                "dimensions invalides {cols}x{rows} (attendu 1..={MAX_DIM} par axe)"
            ),
            Self::Budget { cells, max } => {
                write!(f, "budget de cellules dépassé : {cells} > {max}")
            }
            Self::Stride { stride, cols } => {
                write!(f, "stride {stride} < cols {cols}")
            }
            Self::Overflow => write!(f, "surface non représentable dans un usize"),
        }
    }
}

/// Politique de redimensionnement — explicite, jamais déduite (§C.5.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ResizePolicy {
    /// Recopie la zone commune `min(cols) x min(rows)` ; le reste devient vide.
    Preserve,
    /// Tout le buffer devient [`Cell::BLANK`].
    #[default]
    Clear,
}

/// Bitfield de tuiles sales 8 x 4 cellules (§C.5.5).
///
/// Un seul mot de 64 bits couvre 2048 cellules : à 240 x 72 (540 tuiles) il y a
/// 9 mots, donc un `mark_tile` reste une opération mémoire unique.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirtyTiles {
    words: Vec<u64>,
    tiles_x: u16,
    tiles_y: u16,
    dirty_count: u32,
}

impl DirtyTiles {
    /// Crée un bitfield couvrant `tiles_x * tiles_y` tuiles, **toutes propres**.
    #[must_use]
    pub fn new(tiles_x: u16, tiles_y: u16) -> Self {
        let total = u32::from(tiles_x) * u32::from(tiles_y);
        let words = (total as usize).div_ceil(64);
        Self {
            words: vec![0; words],
            tiles_x,
            tiles_y,
            dirty_count: 0,
        }
    }

    #[inline]
    #[must_use]
    /// Nombre de tuiles en largeur (`ceil(cols / 8)`).
    pub const fn tiles_x(&self) -> u16 {
        self.tiles_x
    }

    #[inline]
    #[must_use]
    /// Nombre de tuiles en hauteur (`ceil(rows / 4)`).
    pub const fn tiles_y(&self) -> u16 {
        self.tiles_y
    }

    /// Nombre de tuiles actuellement sales.
    #[inline]
    #[must_use]
    pub const fn count(&self) -> u32 {
        self.dirty_count
    }

    /// Nombre total de tuiles de la grille.
    #[inline]
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.tiles_x as u32 * self.tiles_y as u32
    }

    /// Remet toutes les tuiles à propre.
    pub fn clear(&mut self) {
        self.words.fill(0);
        self.dirty_count = 0;
    }

    /// Marque **toutes** les tuiles sales (après un resize, §C.5.3).
    pub fn mark_all(&mut self) {
        let total = self.total();
        let mut base = 0u32;
        for w in self.words.iter_mut() {
            // Ne jamais allumer de bits hors grille : `is_tile_dirty` reste juste et
            // `iter` n'a pas à re-filtrer.
            let valid = (total - base).min(64);
            *w = if valid == 64 {
                u64::MAX
            } else {
                (1u64 << valid) - 1
            };
            base += 64;
        }
        self.dirty_count = total;
    }

    /// Marque une tuile ; rend `true` si elle est passée de propre à sale.
    pub fn mark_tile(&mut self, tx: i32, ty: i32) -> bool {
        if tx < 0 || ty < 0 || tx >= i32::from(self.tiles_x) || ty >= i32::from(self.tiles_y) {
            return false;
        }
        let idx = (ty as u32) * u32::from(self.tiles_x) + tx as u32;
        let word = (idx / 64) as usize;
        let bit = 1u64 << (idx % 64);
        let Some(w) = self.words.get_mut(word) else {
            return false;
        };
        if *w & bit != 0 {
            return false;
        }
        *w |= bit;
        self.dirty_count = self.dirty_count.saturating_add(1);
        true
    }

    /// Marque toutes les tuiles touchées par un rectangle **déjà borné**.
    pub fn mark_rect(&mut self, rect: CellRect) {
        if rect.is_empty() {
            return;
        }
        let tx0 = rect.x / TILE_W;
        let tx1 = (rect.right() - 1) / TILE_W;
        let ty0 = rect.y / TILE_H;
        let ty1 = (rect.bottom() - 1) / TILE_H;
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                self.mark_tile(tx, ty);
            }
        }
    }

    #[inline]
    #[must_use]
    /// `true` si la tuile `(tx, ty)` est sale ; `false` hors grille.
    pub fn is_tile_dirty(&self, tx: i32, ty: i32) -> bool {
        if tx < 0 || ty < 0 || tx >= i32::from(self.tiles_x) || ty >= i32::from(self.tiles_y) {
            return false;
        }
        let idx = (ty as u32) * u32::from(self.tiles_x) + tx as u32;
        match self.words.get((idx / 64) as usize) {
            Some(w) => w & (1u64 << (idx % 64)) != 0,
            None => false,
        }
    }

    /// Itère les tuiles sales dans l'ordre ligne par ligne (gauche -> droite).
    pub fn iter(&self) -> impl Iterator<Item = (u16, u16)> + '_ {
        let tiles_x = self.tiles_x;
        (0..self.total()).filter_map(move |idx| {
            let word = self.words.get((idx / 64) as usize)?;
            if word & (1u64 << (idx % 64)) == 0 {
                return None;
            }
            Some((
                (idx % u32::from(tiles_x)) as u16,
                (idx / u32::from(tiles_x)) as u16,
            ))
        })
    }
}

/// Grille de cellules : le `T3Frame` « sortie » du C, en propre (§C.5).
#[derive(Clone, Debug)]
pub struct CellBuffer {
    cells: Vec<Cell>,
    cols: i32,
    rows: i32,
    stride: i32,
    epoch: u32,
    dirty: DirtyTiles,
}

fn check_dims(cols: i32, rows: i32) -> Result<(), BufferError> {
    if !(1..=MAX_DIM).contains(&cols) || !(1..=MAX_DIM).contains(&rows) {
        return Err(BufferError::Dimension { cols, rows });
    }
    let cells = i64::from(cols) * i64::from(rows);
    if cells > i64::from(MAX_CELLS) {
        return Err(BufferError::Budget {
            cells: cells as u32,
            max: MAX_CELLS,
        });
    }
    Ok(())
}

fn buffer_len(stride: i32, rows: i32) -> Result<usize, BufferError> {
    let total = i64::from(stride) * i64::from(rows);
    if total < 0 {
        return Err(BufferError::Overflow);
    }
    if total > i64::from(MAX_CELLS) {
        return Err(BufferError::Budget {
            cells: total.min(i64::from(u32::MAX)) as u32,
            max: MAX_CELLS,
        });
    }
    usize::try_from(total).map_err(|_| BufferError::Overflow)
}

impl CellBuffer {
    /// Buffer `cols` x `rows`, `stride == cols`, entièrement vide, tuiles toutes sales.
    pub fn new(cols: i32, rows: i32) -> Result<Self, BufferError> {
        Self::with_stride(cols, rows, cols)
    }

    /// Buffer à `stride` explicite (`stride >= cols`, padding non lu par I3).
    ///
    /// Le padding est alloué et **initialisé** (jamais de « non initialisé » en Rust,
    /// §G.5) mais n'est jamais exposé par [`Self::row`] ni [`Self::span`].
    pub fn with_stride(cols: i32, rows: i32, stride: i32) -> Result<Self, BufferError> {
        check_dims(cols, rows)?;
        if stride < cols {
            return Err(BufferError::Stride { stride, cols });
        }
        let len = buffer_len(stride, rows)?;
        let dirty = {
            let mut d = DirtyTiles::new(tiles_x(cols), tiles_y(rows));
            d.mark_all();
            d
        };
        Ok(Self {
            cells: vec![Cell::BLANK; len],
            cols,
            rows,
            stride,
            epoch: 0,
            dirty,
        })
    }

    #[inline]
    #[must_use]
    /// Largeur logique, en cellules.
    pub const fn cols(&self) -> i32 {
        self.cols
    }

    #[inline]
    #[must_use]
    /// Hauteur logique, en cellules.
    pub const fn rows(&self) -> i32 {
        self.rows
    }

    /// Nombre de cellules par rangée allouée (>= `cols`).
    #[inline]
    #[must_use]
    pub const fn stride(&self) -> i32 {
        self.stride
    }

    /// Taille d'une rangée en octets (`stride * size_of::<Cell>()`).
    #[inline]
    #[must_use]
    pub const fn stride_bytes(&self) -> usize {
        self.stride as usize * core::mem::size_of::<Cell>()
    }

    /// Nombre de cellules allouées (`stride * rows`).
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.stride as usize * self.rows as usize
    }

    /// Jamais vide : un buffer valide a `cols >= 1` et `rows >= 1`.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Numéro de génération ; change à **chaque** resize effectif (§C.5.1).
    #[inline]
    #[must_use]
    pub const fn epoch(&self) -> u32 {
        self.epoch
    }

    /// Toutes les cellules, padding compris. Réservé aux présentateurs et aux goldens.
    #[inline]
    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Accès brut réservé aux tests d'import : marque **tout** le buffer sale (§C.5.5).
    pub fn cells_mut_raw(&mut self) -> &mut [Cell] {
        self.dirty.mark_all();
        &mut self.cells
    }

    #[inline]
    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.cols && y < self.rows
    }

    #[inline]
    fn index(&self, x: i32, y: i32) -> usize {
        debug_assert!(self.in_bounds(x, y), "index hors bornes ({x}, {y})");
        y as usize * self.stride as usize + x as usize
    }

    /// Lecture bornée.
    #[inline]
    #[must_use]
    pub fn get(&self, x: i32, y: i32) -> Option<&Cell> {
        if !self.in_bounds(x, y) {
            return None;
        }
        self.cells.get(self.index(x, y))
    }

    /// Écriture bornée ; **marque la tuile sale** (§C.5.5).
    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        if !self.in_bounds(x, y) {
            return None;
        }
        self.dirty.mark_tile(x / TILE_W, y / TILE_H);
        let idx = self.index(x, y);
        self.cells.get_mut(idx)
    }

    /// Écrit une cellule ; marque la tuile sale. `false` si hors bornes.
    pub fn set(&mut self, x: i32, y: i32, cell: Cell) -> bool {
        if !self.in_bounds(x, y) {
            return false;
        }
        let idx = self.index(x, y);
        if let Some(slot) = self.cells.get_mut(idx) {
            *slot = cell;
        }
        self.dirty.mark_tile(x / TILE_W, y / TILE_H);
        true
    }

    /// Écrit **seulement si la valeur change** ; ne salit la tuile qu'en cas de
    /// changement. C'est le chemin de `resolve` (invariant I7).
    pub fn set_if_changed(&mut self, x: i32, y: i32, cell: Cell) -> bool {
        if !self.in_bounds(x, y) {
            return false;
        }
        let idx = self.index(x, y);
        match self.cells.get(idx) {
            Some(old) if *old == cell => true,
            Some(_) => {
                if let Some(slot) = self.cells.get_mut(idx) {
                    *slot = cell;
                }
                self.dirty.mark_tile(x / TILE_W, y / TILE_H);
                true
            }
            None => false,
        }
    }

    /// Rangée `y`, tronquée à `cols` (padding exclu, I3).
    #[inline]
    #[must_use]
    pub fn row(&self, y: i32) -> Option<&[Cell]> {
        if y < 0 || y >= self.rows {
            return None;
        }
        let start = y as usize * self.stride as usize;
        self.cells.get(start..start + self.cols as usize)
    }

    /// `span(x, y, len)` : `len` cellules contiguës, bornées à `cols`.
    /// Rend `None` si `(x, y)` ou `len` sort de la grille logique.
    #[inline]
    #[must_use]
    pub fn span(&self, x: i32, y: i32, len: i32) -> Option<&[Cell]> {
        if len < 0 || !self.in_bounds(x, y) || x + len > self.cols {
            return None;
        }
        let start = self.index(x, y);
        self.cells.get(start..start + len as usize)
    }

    /// Remplit un rectangle avec `Cell { glyph: ' ', fg: bg, bg }` (parité `t3_fill`).
    pub fn fill_bg(&mut self, rect: CellRect, bg: u32) {
        let r = rect.clip(self.cols, self.rows);
        if r.is_empty() {
            return;
        }
        let cell = Cell::solid(0x20, bg, bg);
        for y in r.y..r.bottom() {
            let start = self.index(r.x, y);
            let end = start + r.w as usize;
            if let Some(slice) = self.cells.get_mut(start..end) {
                for slot in slice {
                    *slot = cell;
                }
            }
        }
        self.dirty.mark_rect(r);
    }

    /// Tout le buffer redevient vide (pas de changement d'`epoch`).
    pub fn clear(&mut self) {
        self.cells.fill(Cell::BLANK);
        self.dirty.mark_all();
    }

    /// Marque un rectangle sale (borné à la grille).
    pub fn mark_dirty(&mut self, rect: CellRect) {
        let r = rect.clip(self.cols, self.rows);
        self.dirty.mark_rect(r);
    }

    /// Marque tout sale.
    pub fn mark_all_dirty(&mut self) {
        self.dirty.mark_all();
    }

    #[inline]
    #[must_use]
    /// Bitfield des tuiles sales.
    pub const fn dirty(&self) -> &DirtyTiles {
        &self.dirty
    }

    #[inline]
    #[must_use]
    /// Nombre de tuiles sales.
    pub const fn dirty_count(&self) -> u32 {
        self.dirty.count()
    }

    /// Remet le bitfield à zéro (le présentateur le fait après une présentation
    /// réussie, §G.8.6).
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Boîte englobante des tuiles sales, en cellules (padding de tuile inclus,
    /// puis bornée à la grille). `None` si rien n'est sale.
    #[must_use]
    pub fn dirty_bbox(&self) -> Option<CellRect> {
        if self.dirty.count() == 0 {
            return None;
        }
        let mut min_tx = i32::MAX;
        let mut min_ty = i32::MAX;
        let mut max_tx = i32::MIN;
        let mut max_ty = i32::MIN;
        for (tx, ty) in self.dirty.iter() {
            let (tx, ty) = (i32::from(tx), i32::from(ty));
            min_tx = min_tx.min(tx);
            min_ty = min_ty.min(ty);
            max_tx = max_tx.max(tx);
            max_ty = max_ty.max(ty);
        }
        let r = CellRect::new(
            min_tx * TILE_W,
            min_ty * TILE_H,
            (max_tx - min_tx + 1) * TILE_W,
            (max_ty - min_ty + 1) * TILE_H,
        );
        Some(r.clip(self.cols, self.rows))
    }

    /// Resize (seul chemin qui change `cols`/`rows`/`stride`, §C.5.7.3).
    ///
    /// - taille identique : `Ok(epoch)` **sans** réallocation ni `epoch++` (parité C) ;
    /// - sinon : `epoch += 1`, toutes les tuiles sales, capacité réutilisée ;
    /// - hors bornes / budget : `Err`, buffer **inchangé** (jamais de `panic`).
    pub fn resize(
        &mut self,
        cols: i32,
        rows: i32,
        policy: ResizePolicy,
    ) -> Result<u32, BufferError> {
        check_dims(cols, rows)?;
        if cols == self.cols && rows == self.rows {
            return Ok(self.epoch);
        }
        let stride = cols;
        let new_len = buffer_len(stride, rows)?;
        let old_cols = self.cols;
        let old_rows = self.rows;
        let old_stride = self.stride;
        let old_len = self.cells.len();

        // 1. Allonger d'abord : sinon une réduction (stride ou rows) tronquerait des
        //    cellules qu'on doit encore recopier.
        if self.cells.len() < new_len {
            self.cells.resize(new_len, Cell::BLANK);
        }

        if policy == ResizePolicy::Clear {
            self.cells.fill(Cell::BLANK);
        } else {
            let common_cols = old_cols.min(cols);
            let common_rows = old_rows.min(rows);
            debug_assert!(old_len <= self.cells.len());
            // Sens de recopie : vers l'avant si le stride rétrécit, vers l'arrière s'il
            // grandit (une rangée destination ne doit jamais écraser une source future).
            if stride > old_stride {
                for y in (0..common_rows).rev() {
                    let src = y as usize * old_stride as usize;
                    let dst = y as usize * stride as usize;
                    self.cells.copy_within(src..src + common_cols as usize, dst);
                }
            } else {
                for y in 0..common_rows {
                    let src = y as usize * old_stride as usize;
                    let dst = y as usize * stride as usize;
                    self.cells.copy_within(src..src + common_cols as usize, dst);
                }
            }
            // 2. Vider la zone non recopiée.
            for y in 0..rows {
                let row_blank_from = if y < common_rows { common_cols } else { 0 };
                if row_blank_from >= cols {
                    continue;
                }
                let start = y as usize * stride as usize + row_blank_from as usize;
                let end = y as usize * stride as usize + cols as usize;
                if let Some(slice) = self.cells.get_mut(start..end) {
                    for slot in slice {
                        *slot = Cell::BLANK;
                    }
                }
            }
        }

        // 3. Ajuster la longueur logique (l'excédent est conservé en capacité).
        if self.cells.len() > new_len {
            self.cells.truncate(new_len);
        }

        self.cols = cols;
        self.rows = rows;
        self.stride = stride;
        self.epoch = self.epoch.saturating_add(1);
        self.dirty = DirtyTiles::new(tiles_x(cols), tiles_y(rows));
        self.dirty.mark_all();
        Ok(self.epoch)
    }
}

#[inline]
fn tiles_x(cols: i32) -> u16 {
    ((cols + TILE_W - 1) / TILE_W) as u16
}

#[inline]
fn tiles_y(rows: i32) -> u16 {
    ((rows + TILE_H - 1) / TILE_H) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(cols: i32, rows: i32) -> CellBuffer {
        match CellBuffer::new(cols, rows) {
            Ok(b) => b,
            Err(e) => panic!("buffer {cols}x{rows} inattendu : {e}"),
        }
    }

    #[test]
    fn cell_layout_is_16_bytes() {
        assert_eq!(core::mem::size_of::<Cell>(), 16);
        assert_eq!(core::mem::align_of::<Cell>(), 4);
        let c = Cell::new(0x41, 0x0011_2233, 0x0044_5566, FLAG_BOLD);
        assert_eq!(c.glyph, 0x41);
        assert_eq!(c.fg, 0x0011_2233);
        assert_eq!(c.bg, 0x0044_5566);
        assert_eq!(c.flags, FLAG_BOLD);
        // Offsets explicites du contrat §C.1.
        assert_eq!(core::mem::offset_of!(Cell, glyph), 0);
        assert_eq!(core::mem::offset_of!(Cell, fg), 4);
        assert_eq!(core::mem::offset_of!(Cell, bg), 8);
        assert_eq!(core::mem::offset_of!(Cell, flags), 12);
        assert_eq!(core::mem::offset_of!(Cell, _pad), 14);
    }

    #[test]
    fn buffer_bounds() {
        assert_eq!(
            CellBuffer::new(0, 9).err(),
            Some(BufferError::Dimension { cols: 0, rows: 9 })
        );
        assert_eq!(
            CellBuffer::new(-1, 9).err(),
            Some(BufferError::Dimension { cols: -1, rows: 9 })
        );
        assert_eq!(
            CellBuffer::new(4097, 1).err(),
            Some(BufferError::Dimension {
                cols: 4097,
                rows: 1
            })
        );
        assert_eq!(
            CellBuffer::new(1000, 1001).err(),
            Some(BufferError::Budget {
                cells: 1_001_000,
                max: MAX_CELLS
            })
        );
        // Plafond exact accepté : 1000 x 1000 = 1 000 000.
        assert!(CellBuffer::new(1000, 1000).is_ok());

        let mut b = buffer(40, 20);
        assert_eq!(b.stride(), 40);
        assert_eq!(b.stride_bytes(), 40 * 16);
        assert_eq!(b.len(), 800);
        let e0 = b.epoch();
        // Taille identique : ni allocation, ni epoch++.
        assert_eq!(b.resize(40, 20, ResizePolicy::Clear), Ok(e0));
        assert_eq!(b.epoch(), e0);
        // Resize effectif : epoch strictement croissant.
        assert_eq!(b.resize(41, 20, ResizePolicy::Clear), Ok(e0 + 1));
        // Hors bornes : le buffer reste intact.
        assert!(b.resize(0, 20, ResizePolicy::Clear).is_err());
        assert_eq!((b.cols(), b.rows(), b.epoch()), (41, 20, e0 + 1));
    }

    #[test]
    fn buffer_resize_preserve_keeps_common_area() {
        let mut b = buffer(6, 5);
        for y in 0..5 {
            for x in 0..6 {
                let glyph = u32::from(b'a') + (y * 6 + x) as u32;
                assert!(b.set(x, y, Cell::solid(glyph, 1, 2)));
            }
        }
        let e0 = b.epoch();
        assert_eq!(b.resize(4, 3, ResizePolicy::Preserve), Ok(e0 + 1));
        assert_eq!(b.cols(), 4);
        assert_eq!(b.rows(), 3);
        assert_eq!(b.stride(), 4);
        assert_eq!(b.len(), 12);
        for y in 0..3 {
            for x in 0..4 {
                let c = b.get(x, y).copied().unwrap_or(Cell::BLANK);
                let expect = u32::from(b'a') + (y * 6 + x) as u32;
                assert_eq!(c.glyph, expect, "cellule ({x},{y}) mal recopiée");
            }
        }
        // Toutes les tuiles sont sales après resize (§C.5.3).
        assert_eq!(b.dirty_count(), b.dirty().total());

        // Agrandissement : la zone commune survit, le reste est vide.
        assert_eq!(b.resize(6, 5, ResizePolicy::Preserve), Ok(e0 + 2));
        assert_eq!(
            b.get(3, 2).copied().unwrap_or(Cell::BLANK).glyph,
            u32::from(b'p')
        );
        assert!(b.get(5, 4).copied().unwrap_or(Cell::BLANK).is_blank());
    }

    #[test]
    fn buffer_resize_clear_wipes_content() {
        let mut b = buffer(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                assert!(b.set(x, y, Cell::solid(0x2588, 0x0011_2233, 0x0044_5566)));
            }
        }
        // Même taille : Clear n'a aucun effet (parité C : no-op).
        assert_eq!(b.resize(8, 8, ResizePolicy::Clear), Ok(0));
        assert_eq!(b.get(0, 0).copied().unwrap_or(Cell::BLANK).glyph, 0x2588);
        assert_eq!(b.resize(9, 9, ResizePolicy::Clear), Ok(1));
        for y in 0..9 {
            for x in 0..9 {
                assert!(b.get(x, y).copied().unwrap_or(Cell::BLANK).is_blank());
            }
        }
    }

    #[test]
    fn buffer_resize_shrink_and_grow_preserve_are_lossless() {
        // Rétrécissement puis agrandissement : la zone commune survit aux deux sens
        // de recopie (le stride change dans les deux directions).
        let mut b = buffer(10, 4);
        for y in 0..4 {
            for x in 0..10 {
                assert!(b.set(x, y, Cell::solid((x + y * 10) as u32 + 1, 0, 0)));
            }
        }
        assert!(b.resize(3, 2, ResizePolicy::Preserve).is_ok());
        for y in 0..2 {
            for x in 0..3 {
                assert_eq!(
                    b.get(x, y).copied().unwrap_or(Cell::BLANK).glyph,
                    (x + y * 10) as u32 + 1
                );
            }
        }
        assert!(b.resize(12, 6, ResizePolicy::Preserve).is_ok());
        for y in 0..2 {
            for x in 0..3 {
                assert_eq!(
                    b.get(x, y).copied().unwrap_or(Cell::BLANK).glyph,
                    (x + y * 10) as u32 + 1
                );
            }
        }
        assert!(b.get(11, 5).copied().unwrap_or(Cell::BLANK).is_blank());
    }

    #[test]
    fn buffer_dirty_tiles() {
        let mut b = buffer(40, 20);
        b.clear_dirty();
        assert_eq!(b.dirty_count(), 0);
        assert!(b.dirty_bbox().is_none());

        // (7,3) -> tuile (0,0) ; (8,0) -> tuile (1,0).
        assert!(b.get_mut(7, 3).is_some());
        assert!(b.dirty().is_tile_dirty(0, 0));
        assert!(!b.dirty().is_tile_dirty(1, 0));
        assert_eq!(b.dirty_count(), 1);

        assert!(b.get_mut(8, 0).is_some());
        assert!(b.dirty().is_tile_dirty(1, 0));
        assert_eq!(b.dirty_count(), 2);

        // Marquage idempotent : le compteur ne double pas.
        assert!(b.get_mut(9, 1).is_some());
        assert_eq!(b.dirty_count(), 2);

        let bbox = b.dirty_bbox().unwrap_or(CellRect::new(0, 0, 0, 0));
        assert_eq!(bbox, CellRect::new(0, 0, 16, 4));

        // Hors bornes : rien ne change, aucune panique.
        assert!(b.get_mut(40, 0).is_none());
        assert!(b.get_mut(-1, 0).is_none());
        assert_eq!(b.dirty_count(), 2);

        let tiles: Vec<(u16, u16)> = b.dirty().iter().collect();
        assert_eq!(tiles, vec![(0, 0), (1, 0)]);

        b.clear_dirty();
        assert_eq!(b.dirty_count(), 0);
        assert!(b.dirty_bbox().is_none());
    }

    #[test]
    fn buffer_dirty_rect_and_fill() {
        let mut b = buffer(40, 20);
        b.clear_dirty();
        b.fill_bg(CellRect::new(-4, 18, 9, 7), 0x0000_00FF);
        assert_eq!(b.get(0, 19).copied().unwrap_or(Cell::BLANK).bg, 0x0000_00FF);
        assert_eq!(b.get(4, 18).copied().unwrap_or(Cell::BLANK).bg, 0x0000_00FF);
        assert_ne!(b.get(5, 18).copied().unwrap_or(Cell::BLANK).bg, 0x0000_00FF);
        // fg == bg pour un remplissage (parité `t3_fill`).
        assert_eq!(b.get(0, 18).copied().unwrap_or(Cell::BLANK).fg, 0x0000_00FF);
        assert!(b.dirty_count() > 0);
        assert!(b.dirty_bbox().is_some());
        // Rectangle entièrement hors grille : aucun effet.
        b.clear_dirty();
        b.fill_bg(CellRect::new(100, 100, 4, 4), 0x00FF_0000);
        assert_eq!(b.dirty_count(), 0);
    }

    #[test]
    fn two_buffers_are_independent() {
        let mut a = buffer(8, 8);
        let mut b = buffer(8, 8);
        assert!(a.set(0, 0, Cell::solid(0x41, 1, 2)));
        assert_ne!(a.get(0, 0), b.get(0, 0));
        assert_eq!(a.epoch(), 0);
        assert_eq!(b.epoch(), 0);
        assert!(b.resize(9, 9, ResizePolicy::Clear).is_ok());
        assert_eq!(a.epoch(), 0, "l'epoch ne doit pas être global");
        assert_eq!(a.cols(), 8);
    }

    #[test]
    fn span_and_row_are_bounded() {
        let mut b = buffer(10, 3);
        assert!(b.set(9, 2, Cell::solid(0x7A, 0, 0)));
        assert_eq!(b.row(2).map(|r| r.len()), Some(10));
        assert_eq!(b.row(3).map(|r| r.len()), None);
        assert_eq!(b.span(8, 2, 2).map(|s| s.len()), Some(2));
        assert_eq!(b.span(9, 2, 2), None);
        assert_eq!(b.span(0, 0, -1), None);
        assert_eq!(b.span(0, 0, 10).map(|s| s.len()), Some(10));
    }

    #[test]
    fn with_stride_keeps_padding_out_of_the_logical_grid() {
        let mut b = match CellBuffer::with_stride(2, 1, 4) {
            Ok(b) => b,
            Err(e) => panic!("with_stride(2, 1, 4) : {e}"),
        };
        assert_eq!(b.stride(), 4);
        assert_eq!(b.len(), 4);
        assert_eq!(b.stride_bytes(), 64);
        assert!(!b.set(2, 0, Cell::solid(0x41, 0, 0)));
        assert_eq!(b.row(0).map(|r| r.len()), Some(2));
        assert_eq!(
            CellBuffer::with_stride(4, 1, 3).err(),
            Some(BufferError::Stride { stride: 3, cols: 4 })
        );
    }

    #[test]
    fn dirty_tiles_mark_all_masks_only_valid_tiles() {
        // 40 cols -> 5 tuiles ; 20 rows -> 5 tuiles ; 25 tuiles au total.
        let b = buffer(40, 20);
        assert_eq!(b.dirty().tiles_x(), 5);
        assert_eq!(b.dirty().tiles_y(), 5);
        assert_eq!(b.dirty().total(), 25);
        assert_eq!(b.dirty_count(), 25);
        assert!(b.dirty().is_tile_dirty(4, 4));
        assert!(!b.dirty().is_tile_dirty(5, 0));
        assert!(!b.dirty().is_tile_dirty(0, 5));
    }
}
