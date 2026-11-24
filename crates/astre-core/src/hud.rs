//! Primitives de HUD — port de `t3_fill` et `t3_text` (C-REF, §G.1).
//!
//! Contrat conservé au caractère près (§G.4) : texte **ASCII imprimable seulement**
//! (`c >= 32 && c < 127`, sinon `'?'`), écriture opaque (une cellule = un glyphe +
//! un fond), offsets potentiellement négatifs bornés **ici** (§G.3.5), et
//! `y = rows - dy - 1` pour les ancres basses.
//!
//! Les libellés du HUD (`" ASTRE "`, `"RELAIS 07  /  AIRE DE VOL"`, …) restent sans
//! accents : ce sont eux que comparent les goldens texte, et le C n'autorisait que
//! l'ASCII (`astre-app::overlay`, Phase 2).

use crate::cell::{Cell, CellBuffer, CellRect};

/// Ancre d'alignement du texte (`T3Anchor` du C).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Anchor {
    /// Coin haut-gauche.
    TopLeft,
    /// Coin haut-droit (`x = cols - dx - len`).
    TopRight,
    /// Coin bas-gauche (`y = rows - dy - 1`).
    BottomLeft,
    /// Coin bas-droit.
    BottomRight,
}

impl Anchor {
    #[inline]
    #[must_use]
    /// `true` pour une ancre à droite.
    pub const fn is_right(self) -> bool {
        matches!(self, Self::TopRight | Self::BottomRight)
    }

    #[inline]
    #[must_use]
    /// `true` pour une ancre en bas.
    pub const fn is_bottom(self) -> bool {
        matches!(self, Self::BottomLeft | Self::BottomRight)
    }
}

/// Remplit un rectangle de cellules `' '` avec `fg == bg == bg` (parité `t3_fill`).
///
/// `x`/`y` peuvent être négatifs : le bornage est fait une seule fois, ici.
pub fn fill(buf: &mut CellBuffer, x: i32, y: i32, w: i32, h: i32, bg: u32) {
    if w <= 0 || h <= 0 {
        return;
    }
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = x.saturating_add(w).min(buf.cols());
    let y1 = y.saturating_add(h).min(buf.rows());
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    buf.fill_bg(CellRect::new(x0, y0, x1 - x0, y1 - y0), bg);
}

/// Écrit `text` depuis `anchor`, à `dx`/`dy` cellules **vers l'intérieur** de l'ancre.
///
/// - ancre droite : `x = cols - dx - len` ; ancre basse : `y = rows - dy - 1` ;
/// - tronqué à `cols` (les caractères qui dépassent à droite sont ignorés) ;
/// - les positions négatives sont sautées, pas décalées (test `hud` du C :
///   `t3_text(TOP_LEFT, -2, 0, "ABCDE")` affiche `C` en colonne 0) ;
/// - tout octet hors `32..127` devient `'?'`.
pub fn text(buf: &mut CellBuffer, anchor: Anchor, dx: i32, dy: i32, text: &str, fg: u32, bg: u32) {
    let len = text.len() as i32;
    let cols = buf.cols();
    let rows = buf.rows();
    let x = if anchor.is_right() {
        cols - dx - len
    } else {
        dx
    };
    let y = if anchor.is_bottom() {
        rows - dy - 1
    } else {
        dy
    };
    if y < 0 || y >= rows {
        return;
    }
    for (i, byte) in text.bytes().enumerate() {
        let xi = x.saturating_add(i as i32);
        if xi >= cols {
            break;
        }
        if xi < 0 {
            continue;
        }
        let glyph = if (32..127).contains(&byte) {
            u32::from(byte)
        } else {
            u32::from(b'?')
        };
        buf.set(xi, y, Cell::solid(glyph, fg, bg));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: u32 = 0x00FA_2214;
    const BLUE: u32 = 0x000A_28DC;

    fn buffer(cols: i32, rows: i32) -> CellBuffer {
        match CellBuffer::new(cols, rows) {
            Ok(b) => b,
            Err(e) => panic!("buffer {cols}x{rows} : {e}"),
        }
    }

    fn glyph_at(b: &CellBuffer, x: i32, y: i32) -> u32 {
        b.get(x, y).copied().map(|c| c.glyph).unwrap_or(0)
    }

    #[test]
    fn fill_is_bounded_and_opaque() {
        let mut b = buffer(10, 5);
        fill(&mut b, -4, 4, 9, 7, BLUE);
        // Colonnes 0..4 de la dernière rangée sont peintes, la 5e non.
        assert_eq!(b.get(4, 4).copied().unwrap_or(Cell::BLANK).bg, BLUE);
        assert_ne!(b.get(5, 4).copied().unwrap_or(Cell::BLANK).bg, BLUE);
        assert_eq!(b.get(0, 4).copied().unwrap_or(Cell::BLANK).glyph, 0x20);
        // Rectangle vide : aucun effet.
        fill(&mut b, 0, 0, 0, 3, RED);
        assert_eq!(b.get(0, 0).copied().unwrap_or(Cell::BLANK).bg, 0);
    }

    #[test]
    fn bottom_right_anchor_counts_inward_from_the_edge() {
        // Parité `tests/render.c:hud` : "HUD" en BOTTOM_RIGHT (1, 1) sur 40 x 20
        // occupe les colonnes 36..39 de la rangée 18.
        let mut b = buffer(40, 20);
        text(&mut b, Anchor::BottomRight, 1, 1, "HUD", BLUE, RED);
        for (i, expect) in b"HUD".iter().enumerate() {
            let cell = b.get(36 + i as i32, 18).copied().unwrap_or(Cell::BLANK);
            assert_eq!(cell.glyph, u32::from(*expect));
            assert_eq!(cell.fg, BLUE);
            assert_eq!(cell.bg, RED);
        }
    }

    #[test]
    fn negative_dx_drops_leading_characters() {
        // Parité `tests/render.c:hud` : `C` arrive en colonne 0.
        let mut b = buffer(40, 20);
        text(&mut b, Anchor::TopLeft, -2, 0, "ABCDE", RED, BLUE);
        assert_eq!(glyph_at(&b, 0, 0), u32::from(b'C'));
        assert_eq!(glyph_at(&b, 2, 0), u32::from(b'E'));
        assert_eq!(glyph_at(&b, 3, 0), 0x20);
    }

    #[test]
    fn text_is_truncated_at_cols() {
        let mut b = buffer(4, 2);
        text(&mut b, Anchor::TopLeft, 2, 0, "ABCD", RED, BLUE);
        assert_eq!(glyph_at(&b, 2, 0), u32::from(b'A'));
        assert_eq!(glyph_at(&b, 3, 0), u32::from(b'B'));
        // Rien ne déborde (pas de panique, pas d'écriture hors grille).
        assert!(b.dirty_count() > 0);
    }

    #[test]
    fn non_printable_bytes_become_question_marks() {
        let mut b = buffer(8, 2);
        text(&mut b, Anchor::TopLeft, 0, 0, "a\u{7f}\u{0}e", RED, BLUE);
        assert_eq!(glyph_at(&b, 0, 0), u32::from(b'a'));
        assert_eq!(glyph_at(&b, 1, 0), u32::from(b'?'));
        assert_eq!(glyph_at(&b, 2, 0), u32::from(b'?'));
        assert_eq!(glyph_at(&b, 3, 0), u32::from(b'e'));
        // Un caractère hors ASCII est un octet UTF-8 (>127) -> '?' par octet,
        // exactement comme le C (qui itérait sur `char`).
        let mut b = buffer(4, 2);
        text(&mut b, Anchor::TopLeft, 0, 0, "\u{00e9}", RED, BLUE);
        assert_eq!(glyph_at(&b, 0, 0), u32::from(b'?'));
        assert_eq!(glyph_at(&b, 1, 0), u32::from(b'?'));
    }

    #[test]
    fn out_of_grid_rows_are_ignored() {
        let mut b = buffer(10, 3);
        b.clear_dirty();
        text(&mut b, Anchor::TopLeft, 0, 5, "ABC", RED, BLUE);
        assert_eq!(b.dirty_count(), 0);
        text(&mut b, Anchor::BottomLeft, 0, 9, "ABC", RED, BLUE);
        assert_eq!(b.dirty_count(), 0);
    }

    #[test]
    fn all_four_anchors_on_23x9() {
        let mut b = buffer(23, 9);
        text(&mut b, Anchor::TopLeft, 0, 0, "TL", RED, BLUE);
        text(&mut b, Anchor::TopRight, 0, 0, "TR", RED, BLUE);
        text(&mut b, Anchor::BottomLeft, 0, 0, "BL", RED, BLUE);
        text(&mut b, Anchor::BottomRight, 0, 0, "BR", RED, BLUE);
        assert_eq!(glyph_at(&b, 0, 0), u32::from(b'T'));
        assert_eq!(glyph_at(&b, 21, 0), u32::from(b'T'));
        assert_eq!(glyph_at(&b, 22, 0), u32::from(b'R'));
        assert_eq!(glyph_at(&b, 0, 8), u32::from(b'B'));
        assert_eq!(glyph_at(&b, 1, 8), u32::from(b'L'));
        assert_eq!(glyph_at(&b, 21, 8), u32::from(b'B'));
        assert_eq!(glyph_at(&b, 22, 8), u32::from(b'R'));
    }
}
