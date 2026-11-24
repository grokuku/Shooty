//! Zoom d'affichage : paliers discrets en **pixels device** et calcul de grille (§E).
//!
//! Le zoom web n'agit pas sur la caméra : il choisit la taille d'une cellule en pixels
//! device, donc le nombre de colonnes et de lignes, donc la quantité de travail
//! (§E.1). Il ne change **jamais** `cell_aspect` : l'aspect reste strict `0,5`
//! (`hauteur = 2 × largeur`), ce qui rend les points Braille carrés et l'atlas exactement
//! aligné sur la grille d'échantillons 2 × 4 (§E.3).
//!
//! ## Paliers retenus
//!
//! [`ZOOM_LEVELS`] vaut `{8, 10, 12, 14, 16, 20, 24, 32, 40, 48, 64}` (11 paliers).
//!
//! **Écart assumé avec §E.2**, qui listait 9 paliers `{4, 6, 8, 10, 12, 16, 20, 24, 32}` :
//! la Phase 3 fixe la liste `{8 … 64}`. Le plancher à `8 px` évite une grille de plus
//! de 30 000 cellules à `1920 × 1080` (coût du ciel par échantillon, §I.1 P2) et le
//! plafond à `64 px` reste lisible sur un écran HiDPI. Seule conséquence : le nombre de
//! configurations testables change, la propriété « palier entier en pixels device »
//! (justification du discret, §E.2.1–4) est conservée.
//!
//! ## Clamp de budget : jamais une erreur
//!
//! `cols × rows` est borné à [`MAX_CELLS`] (1 M). Si le palier demandé dépasse le
//! budget, on **recule d'un palier** ; si même le palier le plus grossier dépasse
//! (canvas théorique > 20 000 px de large), on tronque `rows` puis `cols` en gardant le
//! centrage. Aucun `Err` : une violation de `MAX_DIM` serait un bug d'appelant
//! (§C.5.2, §E.5).

use crate::cell::MAX_CELLS;
use crate::viewport::PixelSize;

/// Aspect strict d'une cellule en v1 : `hauteur / largeur`.
pub const CELL_ASPECT_INV: u32 = 2;

/// Palier de zoom automatique visé : hauteur de cellule confortable, en pixels device.
pub const AUTO_TARGET_CELL_PX_H: u32 = 20;

/// Paliers de zoom, en **pixels device** de largeur de cellule (hauteur = 2 ×).
///
/// Voir l'écart documenté en tête de module : la liste de la Phase 3 est `{8 … 64}`.
pub const ZOOM_LEVELS: [u32; 11] = [8, 10, 12, 14, 16, 20, 24, 32, 40, 48, 64];

/// Indice du palier le plus proche d'une largeur de cellule demandée.
///
/// Sert à valider une préférence persistée (`localStorage`) ou une valeur de config :
/// aucune valeur n'est refusée, elle est ramenée au palier le plus proche.
#[must_use]
pub fn nearest_level(cell_px_w: u32) -> usize {
    let mut best = 0usize;
    let mut best_delta = u32::MAX;
    let mut i = 0usize;
    while i < ZOOM_LEVELS.len() {
        let delta = ZOOM_LEVELS[i].abs_diff(cell_px_w);
        if delta < best_delta {
            best_delta = delta;
            best = i;
        }
        i += 1;
    }
    best
}

/// Palier automatique : celui dont la hauteur de cellule approche
/// [`AUTO_TARGET_CELL_PX_H`] (`argmin |2·w − 20|`).
#[must_use]
pub fn auto_level() -> usize {
    let mut best = 0usize;
    let mut best_delta = u32::MAX;
    let mut i = 0usize;
    while i < ZOOM_LEVELS.len() {
        let delta = (ZOOM_LEVELS[i] * CELL_ASPECT_INV).abs_diff(AUTO_TARGET_CELL_PX_H);
        if delta < best_delta {
            best_delta = delta;
            best = i;
        }
        i += 1;
    }
    best
}

/// Palier suivant/précédent, borné aux extrémités (jamais d'erreur, §H.3).
#[must_use]
pub fn step_level(level: usize, delta: i8) -> usize {
    let last = ZOOM_LEVELS.len() - 1;
    let level = level.min(last);
    if delta > 0 {
        (level + delta as usize).min(last)
    } else {
        level.saturating_sub(delta.unsigned_abs() as usize)
    }
}

/// Géométrie de grille résultant du zoom et de la surface disponible.
///
/// `origin_px` est le coin haut-gauche de la grille dans le canvas : les marges
/// (letterbox) sont **égales** à un pixel près, la grille est centrée (§E.4).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grid {
    /// Indice du palier retenu (`0..ZOOM_LEVELS.len()`).
    pub level: u8,
    /// Taille d'une cellule en pixels device (`h == 2 × w`).
    pub cell_px: PixelSize,
    /// Colonnes de la grille (`>= 1`).
    pub cols: i32,
    /// Lignes de la grille (`>= 1`).
    pub rows: i32,
    /// Coin haut-gauche de la grille, en pixels device.
    pub origin_px: (i32, i32),
}

impl Grid {
    /// Nombre de cellules de la grille.
    #[must_use]
    pub fn cells(&self) -> u32 {
        u32::try_from(i64::from(self.cols) * i64::from(self.rows)).unwrap_or(u32::MAX)
    }

    /// Taille occupée par la grille, en pixels device.
    #[must_use]
    pub fn used_px(&self) -> PixelSize {
        PixelSize::new(
            self.cell_px.w.saturating_mul(self.cols.max(0) as u32),
            self.cell_px.h.saturating_mul(self.rows.max(0) as u32),
        )
    }
}

/// Calcule la grille pour une surface disponible en pixels **device**.
///
/// `requested_cell_px` = préférence utilisateur (largeur de cellule), ramenée au palier
/// le plus proche ; `None` = palier automatique. Retourne `None` si la surface est
/// dégénérée (0 pixel dans une dimension) : dans ce cas rien n'est affichable.
#[must_use]
pub fn fit(avail: PixelSize, requested_cell_px: Option<u32>) -> Option<Grid> {
    if avail.is_empty() {
        return None;
    }
    let mut level = match requested_cell_px {
        Some(px) => nearest_level(px),
        None => auto_level(),
    };
    let avail = PixelSize::new(avail.w.min(16384), avail.h.min(16384));

    // 1. Descente de palier tant que le budget est dépassé (jamais d'erreur).
    let mut cols;
    let mut rows;
    loop {
        let (c, r) = grid_dims(avail, level);
        cols = c;
        rows = r;
        if cols as u64 * rows as u64 <= u64::from(MAX_CELLS) || level == 0 {
            break;
        }
        level -= 1;
    }

    // 2. Même au palier le plus grossier le budget peut être dépassé (canvas
    //    théorique). On tronque alors les lignes, puis les colonnes.
    if cols as u64 * rows as u64 > u64::from(MAX_CELLS) {
        rows = i32::try_from(MAX_CELLS / cols.max(1) as u32)
            .unwrap_or(1)
            .max(1);
        if cols as u64 * rows as u64 > u64::from(MAX_CELLS) {
            cols = i32::try_from(MAX_CELLS / rows.max(1) as u32)
                .unwrap_or(1)
                .max(1);
        }
    }

    let w = ZOOM_LEVELS[level];
    let h = w * CELL_ASPECT_INV;
    let used_w = cols * w as i32;
    let used_h = rows * h as i32;
    Some(Grid {
        level: u8::try_from(level).unwrap_or(u8::MAX),
        cell_px: PixelSize::new(w, h),
        cols,
        rows,
        // Marges égales, jamais négatives : si la surface est plus petite qu'une
        // cellule (canvas dégénéré), la grille déborde et c'est le `scissor` du
        // présentateur qui coupe.
        origin_px: (
            ((avail.w as i32 - used_w) / 2).max(0),
            ((avail.h as i32 - used_h) / 2).max(0),
        ),
    })
}

/// Dimensions de grille d'un palier pour une surface donnée (`>= 1` dans chaque axe).
fn grid_dims(avail: PixelSize, level: usize) -> (i32, i32) {
    let w = ZOOM_LEVELS[level.min(ZOOM_LEVELS.len() - 1)];
    let h = w * CELL_ASPECT_INV;
    let cols = (avail.w / w).max(1);
    let rows = (avail.h / h).max(1);
    (
        i32::try_from(cols).unwrap_or(1),
        i32::try_from(rows).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn nearest_level_snaps_to_the_ladder() {
        assert_eq!(nearest_level(0), 0);
        assert_eq!(nearest_level(8), 0);
        assert_eq!(nearest_level(11), 1);
        assert_eq!(nearest_level(14), 3);
        assert_eq!(nearest_level(65), ZOOM_LEVELS.len() - 1);
        assert_eq!(nearest_level(u32::MAX), ZOOM_LEVELS.len() - 1);
    }

    #[test]
    fn auto_level_targets_a_20px_cell_height() {
        let idx = auto_level();
        assert_eq!(ZOOM_LEVELS[idx] * 2, AUTO_TARGET_CELL_PX_H);
    }

    #[test]
    fn steps_clamp_at_both_ends() {
        let last = ZOOM_LEVELS.len() - 1;
        assert_eq!(step_level(0, -1), 0);
        assert_eq!(step_level(last, 1), last);
        assert_eq!(step_level(0, 1), 1);
        assert_eq!(step_level(last, -1), last - 1);
        // 20 pas au-delà des bornes : aucune erreur, palier stable (§H.3).
        assert_eq!(step_level(last, 20), last);
        assert_eq!(step_level(0, -20), 0);
    }

    #[test]
    fn empty_surface_has_no_grid() {
        assert!(fit(PixelSize::new(0, 0), None).is_none());
        assert!(fit(PixelSize::new(1920, 0), None).is_none());
    }

    #[test]
    fn fit_letterbox_is_centered_and_exact() {
        let avail = PixelSize::new(1920, 1080);
        let grid = fit(avail, Some(8)).unwrap();
        assert_eq!(grid.cell_px, PixelSize::new(8, 16));
        assert_eq!(grid.cols, 240);
        assert_eq!(grid.rows, 67);
        let used = grid.used_px();
        assert_eq!(used, PixelSize::new(1920, 1072));
        assert_eq!(grid.origin_px, (0, 4));
        // Marges égales à un pixel près.
        let right = avail.w as i32 - (grid.origin_px.0 + used.w as i32);
        let bottom = avail.h as i32 - (grid.origin_px.1 + used.h as i32);
        assert!((right - grid.origin_px.0).abs() <= 1);
        assert!((bottom - grid.origin_px.1).abs() <= 1);
        assert!(right >= 0 && bottom >= 0);
    }

    #[test]
    fn fit_budget_clamp_never_exceeds_a_million_cells() {
        let viewports = [
            PixelSize::new(1920, 1080),
            PixelSize::new(2560, 1440),
            PixelSize::new(3840, 2160),
            PixelSize::new(5120, 2880),
            PixelSize::new(7680, 4320),
            PixelSize::new(16384, 16384),
            PixelSize::new(320, 200),
            PixelSize::new(37, 11),
        ];
        for avail in viewports {
            for requested in [None, Some(8), Some(14), Some(32), Some(64), Some(4096)] {
                let grid = fit(avail, requested).unwrap();
                assert!(
                    grid.cells() <= MAX_CELLS,
                    "{:?} {:?} -> {} cellules",
                    avail,
                    requested,
                    grid.cells()
                );
                assert!(grid.cols >= 1 && grid.rows >= 1);
                assert_eq!(grid.cell_px.h, grid.cell_px.w * 2);
                let used = grid.used_px();
                assert!(grid.origin_px.0 >= 0 && grid.origin_px.1 >= 0);
                if used.w <= avail.w && used.h <= avail.h {
                    assert!(used.w as i32 + grid.origin_px.0 <= avail.w as i32);
                    assert!(used.h as i32 + grid.origin_px.1 <= avail.h as i32);
                }
            }
        }
    }

    #[test]
    fn huge_requested_level_recovers_by_stepping_down() {
        let grid = fit(PixelSize::new(3840, 2160), Some(8)).unwrap();
        assert!(grid.cells() <= MAX_CELLS);
        // Le palier 8 px sur 3840×2160 ferait 480×270 = 129 600 cellules : sous le
        // budget, donc il est conservé.
        assert_eq!(grid.cell_px.w, 8);
    }

    #[test]
    fn fit_auto_is_monotone_in_surface() {
        let small = fit(PixelSize::new(800, 600), None).unwrap();
        let large = fit(PixelSize::new(1920, 1080), None).unwrap();
        assert!(large.cols >= small.cols);
        assert!(large.rows >= small.rows);
    }

    #[test]
    fn dpr_fractional_keeps_integer_cells() {
        // 1280 × 720 CSS à DPR 1,25 -> 1600 × 900 device.
        let grid = fit(PixelSize::new(1600, 900), Some(12)).unwrap();
        assert_eq!(grid.cell_px, PixelSize::new(12, 24));
        assert_eq!(grid.cols, 133);
        assert_eq!(grid.rows, 37);
        assert_eq!(grid.used_px(), PixelSize::new(1596, 888));
        assert_eq!(grid.origin_px, (2, 6));
    }
}
