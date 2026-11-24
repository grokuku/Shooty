//! Résolution échantillons -> cellule — port fidèle de `resolve_cell` / `t3_resolve`
//! (C-REF), avec les tables en `const` (§C.5.6).
//!
//! Ce que le C faisait et qu'on conserve **tel quel** (§G.3) :
//! - `p[hi]` = échantillon le plus éloigné de `p[0]`, puis `p[lo]` = plus éloigné de
//!   `p[hi]` ; seuil de séparation `160` si la cellule contient de la géométrie
//!   (`surface`), `1500` sinon (ciel) ;
//! - `mask` = `u8`, bit `i` = échantillon `i` dans le groupe 1 (parcours 2 x 4 de
//!   gauche à droite, haut en bas) ;
//! - sommes et moyennes de couleurs en **entiers** (`i32`), division tronquante ;
//! - table ASCII choisie par distance de Hamming minimale, **inversion autorisée**
//!   (`d > 4 => d = 8 - d`), le double sens du champ `mask` du C devenant deux champs
//!   distincts (`ch` + `inverted`) ;
//! - glyphes Braille : `dots[i]` standard, inversion si la grappe d'encre dépasse
//!   4 échantillons ; blocs alignés seulement si les deux bandes sont identiques.
//!
//! Invariant non documenté du C conservé et asserté : `count[0] >= 1` (`p[lo]` ne peut
//! jamais tomber dans le groupe 1), donc la division n'est jamais `0/0` (§G.3.4).

use crate::cell::{Cell, Mode};
use crate::math::{mix, rgb_channels, rgb_pack};

/// Une entrée de la table ASCII : glyphe et polarité.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AsciiShape {
    /// Caractère ASCII imprimable.
    pub ch: u8,
    /// `true` si le masque de l'échantillon doit être complémenté pour correspondre.
    pub inverted: bool,
}

/// `Shape` du C : caractère + masque de référence de 8 bits.
#[derive(Clone, Copy)]
struct Shape {
    ch: u8,
    mask: u8,
}

/// Les 19 motifs du C, dans le même ordre (les égalités de distance sont tranchées
/// par « premier gagnant », comme le `d < best` du C).
const SHAPES: [Shape; 19] = [
    Shape { ch: b'.', mask: 64 },
    Shape { ch: b'\'', mask: 2 },
    Shape { ch: b',', mask: 80 },
    Shape { ch: b'-', mask: 12 },
    Shape {
        ch: b'_',
        mask: 192,
    },
    Shape { ch: b'"', mask: 3 },
    Shape { ch: b':', mask: 68 },
    Shape { ch: b'|', mask: 85 },
    Shape { ch: b'/', mask: 90 },
    Shape {
        ch: b'\\',
        mask: 165,
    },
    Shape {
        ch: b'(',
        mask: 149,
    },
    Shape {
        ch: b')',
        mask: 106,
    },
    Shape { ch: b'+', mask: 93 },
    Shape { ch: b'^', mask: 39 },
    Shape {
        ch: b'v',
        mask: 216,
    },
    Shape {
        ch: b'=',
        mask: 204,
    },
    Shape {
        ch: b'%',
        mask: 105,
    },
    Shape {
        ch: b'#',
        mask: 189,
    },
    Shape { ch: b' ', mask: 0 },
];

/// Table de 256 entrées, calculée **à la compilation** (plus de `static` paresseux).
///
/// `lookup[m]` = motif le plus proche du masque `m`, inversion comprise.
pub const ASCII_LOOKUP: [AsciiShape; 256] = build_ascii_lookup();

const fn popcount8(v: u8) -> u8 {
    let mut n = 0u8;
    let mut x = v;
    while x != 0 {
        x &= x - 1;
        n += 1;
    }
    n
}

const fn build_ascii_lookup() -> [AsciiShape; 256] {
    let mut table = [AsciiShape {
        ch: b' ',
        inverted: false,
    }; 256];
    let mut m = 0usize;
    while m < 256 {
        let mut best = 9u8;
        let mut i = 0usize;
        while i < SHAPES.len() {
            let mut d = popcount8((m as u8) ^ SHAPES[i].mask);
            let mut inverted = false;
            if d > 4 {
                d = 8 - d;
                inverted = true;
            }
            if d < best {
                best = d;
                table[m] = AsciiShape {
                    ch: SHAPES[i].ch,
                    inverted,
                };
            }
            i += 1;
        }
        m += 1;
    }
    table
}

/// Rampes de luminance pour les cellules pleines sans géométrie (mode ASCII).
/// Copie exacte de `"..::-==+**##"` (12 caractères).
const RAMP: [u8; 12] = *b"..::-==+**##";

/// Table des 16 blocs/quadrants/demi-blocs alignés (masque -> codepoint).
///
/// L'index est `(mask & 3) | ((mask >> 2) & 12)` : seuls les motifs dont les deux
/// bandes de 2 lignes sont identiques y accèdent, d'où l'absence de `▁▂▃…`.
pub const BLOCKS: [u32; 16] = [
    0x0020, 0x2598, 0x259D, 0x2580, 0x2596, 0x258C, 0x259E, 0x259B, 0x2597, 0x259A, 0x2590, 0x259C,
    0x2584, 0x2599, 0x259F, 0x2588,
];

/// Bits braille standard, dans l'ordre des échantillons `2j + col` (C-REF `dots[]`).
pub const BRAILLE_DOTS: [u32; 8] = [1, 8, 2, 16, 4, 32, 64, 128];

/// Bit de base des glyphes braille (`U+2800`).
pub const BRAILLE_BASE: u32 = 0x2800;

/// Distance pondérée du C : `2·dr² + 3·dg² + db²` (`i32`, jamais `f32`).
#[inline]
#[must_use]
pub fn distance(a: u32, b: u32) -> i32 {
    let (ar, ag, ab) = rgb_channels(a);
    let (br, bg, bb) = rgb_channels(b);
    let r = i32::from(ar) - i32::from(br);
    let g = i32::from(ag) - i32::from(bg);
    let bl = i32::from(ab) - i32::from(bb);
    2 * r * r + 3 * g * g + bl * bl
}

#[inline]
fn average(sum: [i32; 3], count: i32) -> u32 {
    debug_assert!(count >= 1, "count[0] >= 1 est un invariant (cf. §G.3.4)");
    // Division entière tronquante, comme le C ; les canaux sont positifs.
    rgb_pack(
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
    )
}

/// Résout les 8 échantillons d'une cellule en une [`Cell`].
///
/// `surface` = au moins un échantillon a `depth > 0` (dérivé de la profondeur, C-REF).
#[must_use]
pub fn resolve_cell(p: &[u32; 8], surface: bool, mode: Mode) -> Cell {
    // 1. p[hi] : le plus éloigné de p[0].
    let mut hi = 0usize;
    let mut spread = 0i32;
    for (i, &pi) in p.iter().enumerate().skip(1) {
        let d = distance(p[0], pi);
        if d > spread {
            hi = i;
            spread = d;
        }
    }
    // 2. p[lo] : le plus éloigné de p[hi] ; `spread` devient la séparation des groupes.
    spread = 0;
    let mut lo = 0usize;
    for (i, &pi) in p.iter().enumerate() {
        let d = distance(p[hi], pi);
        if d > spread {
            lo = i;
            spread = d;
        }
    }

    let threshold = if surface { 160 } else { 1500 };
    let mut sum = [[0i32; 3]; 2];
    let mut count = [0i32; 2];
    let mut mask = 0u8;
    for (i, &pi) in p.iter().enumerate() {
        let group = u8::from(spread > threshold && distance(pi, p[hi]) < distance(pi, p[lo]));
        let (r, g, b) = rgb_channels(pi);
        sum[group as usize][0] += i32::from(r);
        sum[group as usize][1] += i32::from(g);
        sum[group as usize][2] += i32::from(b);
        count[group as usize] += 1;
        mask |= group << i;
    }
    debug_assert!(count[0] >= 1);
    let bg = average(sum[0], count[0]);

    if mask == 0 {
        // Aucune séparation : cellule unie (ou rampe de luminance en ASCII « surface »).
        if !surface || mode == Mode::Braille {
            return Cell::solid(0x20, bg, bg);
        }
        let (br, bgc, bb) = rgb_channels(bg);
        let light = (i32::from(br) * 2 + i32::from(bgc) * 3 + i32::from(bb)) / 6;
        let ink = mix(bg, rgb_pack(255, 250, 240), 0.14);
        let paper = mix(bg, rgb_pack(0, 0, 0), 0.18);
        let idx = (light * 12 / 256) as usize;
        return Cell::solid(u32::from(RAMP[idx]), ink, paper);
    }

    let fg = average(sum[1], count[1]);

    if mode == Mode::Ascii {
        let m = match ASCII_LOOKUP.get(mask as usize) {
            Some(m) => *m,
            None => AsciiShape {
                ch: b' ',
                inverted: false,
            },
        };
        let (fg, bg) = if m.inverted { (bg, fg) } else { (fg, bg) };
        // Les traits ASCII ne couvrent qu'en partie leur zone d'échantillon : on
        // mélange un peu d'encre dans le papier pour adoucir les marches.
        return Cell::solid(u32::from(m.ch), fg, mix(bg, fg, 0.15));
    }

    // Mode Braille : blocs alignés si les deux bandes de 2 lignes sont identiques.
    if ((mask ^ (mask >> 2)) & 0x33) == 0 {
        let idx = usize::from((mask & 3) | ((mask >> 2) & 12));
        let block = BLOCKS.get(idx).copied().unwrap_or(0x20);
        return Cell::solid(block, fg, bg);
    }

    // Les points occupent moins de surface que le papier : encre = petite grappe.
    let (fg, bg, mask) = if count[1] > 4 {
        (bg, fg, mask ^ 0xFF)
    } else {
        (fg, bg, mask)
    };
    let mut glyph = BRAILLE_BASE;
    for (i, &dot) in BRAILLE_DOTS.iter().enumerate() {
        if (mask >> i) & 1 != 0 {
            glyph |= dot;
        }
    }
    Cell::solid(glyph, fg, bg)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: u32 = 0;
    const RED: u32 = 0x00FA_2214;
    const BLUE: u32 = 0x000A_28DC;

    fn samples(values: [u32; 8]) -> [u32; 8] {
        values
    }

    #[test]
    fn lookup_has_no_empty_or_invalid_glyph() {
        for (mask, entry) in ASCII_LOOKUP.iter().enumerate() {
            assert!(
                (32..127).contains(&entry.ch),
                "masque {mask} -> glyphe non imprimable {}",
                entry.ch
            );
        }
        // Cas connus de la table du C.
        assert_eq!(ASCII_LOOKUP[0].ch, b' ');
        assert!(!ASCII_LOOKUP[0].inverted);
        // 255 (tous les échantillons en groupe 1) : le motif plein est '#' inversé
        // vers ' ', aucune autre entrée n'étant à distance 0.
        assert_eq!(ASCII_LOOKUP[255].ch, b' ');
        assert!(ASCII_LOOKUP[255].inverted);
        // Le masque 189 est exactement '#', sans inversion.
        assert_eq!(
            ASCII_LOOKUP[189],
            AsciiShape {
                ch: b'#',
                inverted: false
            }
        );
        // Son complément est '#' inversé.
        assert_eq!(
            ASCII_LOOKUP[66],
            AsciiShape {
                ch: b'#',
                inverted: true
            }
        );
    }

    #[test]
    fn flat_cell_is_a_space_with_the_average_colour() {
        let cell = resolve_cell(&samples([BLACK; 8]), false, Mode::Ascii);
        assert_eq!(cell.glyph, 0x20);
        assert_eq!(cell.fg, BLACK);
        assert_eq!(cell.bg, BLACK);

        let cell = resolve_cell(&samples([RED; 8]), true, Mode::Braille);
        assert_eq!(cell.glyph, 0x20);
        assert_eq!(cell.bg, RED);
        assert_eq!(cell.fg, RED);
    }

    #[test]
    fn ascii_flat_surface_uses_the_luminance_ramp() {
        // Cellule pleine et lumineuse, sans séparation -> rampe, jamais ' '.
        let cell = resolve_cell(&samples([0x00FF_FFFF; 8]), true, Mode::Ascii);
        assert_ne!(cell.glyph, 0x20);
        assert!((32..127).contains(&cell.glyph));
        // Sombre -> premier caractère de la rampe.
        let cell = resolve_cell(&samples([0x0000_0001; 8]), true, Mode::Ascii);
        assert_eq!(cell.glyph, u32::from(b'.'));
    }

    #[test]
    fn split_cell_emits_an_ascii_stroke_with_two_colours() {
        // 4 échantillons rouges en haut, 4 bleus en bas : deux groupes nets.
        // Le groupe 1 est le bleu (p[hi] = premier bleu, p[lo] = premier rouge), donc
        // encre = bleu, papier = rouge adouci par `mix(bg, fg, 0.15)`.
        // Masque 0xF0 : le motif le plus proche est ',' (distance 2, sans inversion),
        // exactement ce que choisit la table du C.
        let p = samples([RED, RED, RED, RED, BLUE, BLUE, BLUE, BLUE]);
        let cell = resolve_cell(&p, true, Mode::Ascii);
        assert_eq!(cell.glyph, u32::from(b','));
        assert_eq!(cell.fg, BLUE);
        assert_eq!(cell.bg, mix(RED, BLUE, 0.15));
        assert_ne!(cell.fg, cell.bg);
    }

    #[test]
    fn braille_emits_only_standard_dots_or_aligned_blocks() {
        let p = samples([0x00FF_FFFF, 0, 0x00FF_FFFF, 0, 0, 0, 0, 0]);
        let cell = resolve_cell(&p, true, Mode::Braille);
        let is_block = BLOCKS.contains(&cell.glyph);
        let is_braille = (0x2800..=0x28FF).contains(&cell.glyph);
        assert!(is_block || is_braille, "glyphe inattendu {:#X}", cell.glyph);
        // Jamais un demi-bloc vertical non aligné (▁ ▂ ▃ ▄ ▅ ▆ ▇).
        for g in 0x2581..=0x2587 {
            assert_ne!(cell.glyph, g);
        }
    }

    #[test]
    fn braille_block_path_is_taken_when_the_two_rows_of_a_band_agree() {
        // Le test du C est `!((mask ^ (mask >> 2)) & 0x33)` : il compare la rangée
        // paire et la rangée impaire de chaque bande (bits 0,1 vs 2,3 et 4,5 vs 6,7).
        // Ici l'encre (groupe 1) couvre la bande basse entière -> masque 0xF0, aligné,
        // index 12 -> demi-bloc bas `▄`.
        let p = samples([
            0x00FF_FFFF,
            0x00FF_FFFF,
            0x00FF_FFFF,
            0x00FF_FFFF,
            BLACK,
            BLACK,
            BLACK,
            BLACK,
        ]);
        let cell = resolve_cell(&p, true, Mode::Braille);
        assert_eq!(cell.glyph, 0x2584, "attendu un demi-bloc bas");
        assert_eq!(cell.fg, BLACK);
        assert_eq!(cell.bg, 0x00FF_FFFF);

        // Bandes non alignées -> chemin braille.
        let p = samples([
            0x00FF_FFFF,
            BLACK,
            BLACK,
            0x00FF_FFFF,
            0x00FF_FFFF,
            BLACK,
            BLACK,
            0x00FF_FFFF,
        ]);
        let cell = resolve_cell(&p, true, Mode::Braille);
        assert!((0x2800..=0x28FF).contains(&cell.glyph));
    }

    #[test]
    fn braille_bit_mapping_matches_the_standard() {
        // Un seul échantillon allumé (indice 1 = colonne droite, rangée 0) -> point 1
        // du Braille standard, codepoint U+2808.
        for (idx, &dot) in BRAILLE_DOTS.iter().enumerate() {
            let mut p = [BLACK; 8];
            p[idx] = 0x00FF_FFFF;
            let cell = resolve_cell(&p, true, Mode::Braille);
            let expected = BRAILLE_BASE | dot;
            assert!(
                cell.glyph == expected || BLOCKS.contains(&cell.glyph),
                "échantillon {idx} : glyphe {:#X}, attendu {expected:#X}",
                cell.glyph
            );
            if !BLOCKS.contains(&cell.glyph) {
                assert_eq!(cell.glyph & !BRAILLE_BASE, dot);
            }
        }
    }

    #[test]
    fn single_dot_cells_use_the_ink_colour() {
        // Un seul échantillon d'encre : count[1] == 1 <= 4, donc pas d'inversion.
        let mut p = [BLACK; 8];
        p[0] = 0x00FF_FFFF;
        let cell = resolve_cell(&p, true, Mode::Braille);
        assert_eq!(cell.fg, 0x00FF_FFFF);
        assert_eq!(cell.bg, BLACK);
    }

    #[test]
    fn count_zero_is_never_empty() {
        // Invariant §G.3.4 sur des échantillons pseudo-aléatoires déterministes.
        let mut state = 0x1234_5678u32;
        for _ in 0..2000 {
            let mut p = [0u32; 8];
            for slot in &mut p {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *slot = state & 0x00FF_FFFF;
            }
            let a = resolve_cell(&p, true, Mode::Ascii);
            let b = resolve_cell(&p, false, Mode::Braille);
            assert!(a.glyph != 0 && b.glyph != 0);
            assert!(a.fg & 0xFF00_0000 == 0 && a.bg & 0xFF00_0000 == 0);
            assert!(b.fg & 0xFF00_0000 == 0 && b.bg & 0xFF00_0000 == 0);
        }
    }

    #[test]
    fn distance_weights_channels_like_the_c() {
        let black = 0x0000_0000;
        let white = 0x00FF_FFFF;
        assert_eq!(
            distance(black, white),
            2 * 255 * 255 + 3 * 255 * 255 + 255 * 255
        );
        assert_eq!(distance(white, white), 0);
        let red = 0x00FF_0000;
        let green = 0x0000_FF00;
        assert!(distance(black, green) > distance(black, red));
    }
}
