//! Encodeur ANSI **différentiel** — port de `terminal.c:{utf8,nearest_cube,indexed,ink,
//! emit,equal_cell,t3_present,t3_snapshot}` (C-REF).
//!
//! Parité **octet à octet** exigée par §G.1 et §H.4 :
//!
//! 1. cellules égales sautées (`equal_cell`) et **`fg` ignoré quand `glyph == ' '`** ;
//! 2. cellules modifiées **contiguës** fusionnées en un seul positionnement curseur
//!    `\x1b[{y+1};{x+1}H` ;
//! 3. couleur réémise **seulement si la clé change**, la mémoire `fg`/`bg` étant remise
//!    à `-1` au début de **chaque** image (pas à chaque ligne) ;
//! 4. ordre d'émission : `bg` (48) puis `fg` (38), `fg` omis sur un espace ;
//! 5. redraw **complet** au changement de taille ou de palette ;
//! 6. `previous` **non** mis à jour si l'écriture échoue (§G.8.6).
//!
//! Deux modes de sortie, comme le C :
//!
//! - [`AnsiEncoder::encode_present`] + [`AnsiEncoder::commit`] : présentation live
//!   (différentielle, avec `previous`) ;
//! - [`snapshot`] : image complète, une ligne par rangée, `\x1b[0m` en fin de ligne hors
//!   mode `plain` (sortie de `--snapshot`).

use std::io::{self, Write};

use astre_core::math::rgb_channels;
use astre_core::{Cell, CellBuffer, Palette};

/// Encodeur ANSI avec double buffer (`previous` du C).
///
/// Le terminal est **le seul** backend à garder une copie de l'image précédente : le web
/// ré-upload tout (§A.4).
#[derive(Clone, Debug)]
pub struct AnsiEncoder {
    previous: Vec<Cell>,
    cols: i32,
    rows: i32,
    palette: Palette,
    buffer: Vec<u8>,
    full: bool,
}

impl Default for AnsiEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsiEncoder {
    /// Encodeur vide : la première image sera complète.
    #[must_use]
    pub fn new() -> Self {
        Self {
            previous: Vec::new(),
            cols: 0,
            rows: 0,
            palette: Palette::Rgb,
            buffer: Vec::new(),
            full: true,
        }
    }

    /// Force un redraw complet à la prochaine présentation (reprise après `Ctrl-Z`,
    /// changement de palette hors [`Self::encode_present`]).
    pub fn invalidate(&mut self) {
        self.cols = 0;
        self.rows = 0;
        self.full = true;
    }

    /// Octets encodés par le dernier appel d'encodage.
    #[must_use]
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// `true` si la dernière image était complète (aucun diff possible).
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.full
    }

    /// Encode la présentation différentielle de `cells` dans le buffer interne.
    ///
    /// N'altère **pas** `previous` : c'est [`Self::commit`] qui le fait, après une
    /// écriture réussie (propriété §G.8.6).
    pub fn encode_present(&mut self, cells: &CellBuffer, palette: Palette) {
        let w = cells.cols();
        let h = cells.rows();
        let full = w != self.cols || h != self.rows || palette != self.palette;
        self.full = full;
        self.buffer.clear();

        let mut fg: i32 = -1;
        let mut bg: i32 = -1;
        for y in 0..h {
            let mut contiguous = false;
            for x in 0..w {
                let Some(cell) = cells.get(x, y).copied() else {
                    continue;
                };
                if !full {
                    let previous = self.previous_at(x, y);
                    if cell_equals(&cell, &previous) {
                        contiguous = false;
                        continue;
                    }
                }
                if !contiguous {
                    push_position(&mut self.buffer, y + 1, x + 1);
                }
                emit(&mut self.buffer, &cell, palette, &mut fg, &mut bg);
                contiguous = true;
            }
        }
    }

    /// Valide l'image encodée : recopie les cellules visibles dans `previous`.
    ///
    /// À n'appeler qu'après une écriture réussie.
    pub fn commit(&mut self, cells: &CellBuffer, palette: Palette) {
        let w = cells.cols();
        let h = cells.rows();
        let n = (w.max(0) as usize) * (h.max(0) as usize);
        self.previous.clear();
        self.previous.reserve(n);
        for y in 0..h {
            if let Some(row) = cells.row(y) {
                self.previous.extend_from_slice(row);
            }
        }
        self.cols = w;
        self.rows = h;
        self.palette = palette;
    }

    fn previous_at(&self, x: i32, y: i32) -> Cell {
        let index = y as usize * self.cols.max(0) as usize + x as usize;
        self.previous.get(index).copied().unwrap_or(Cell::BLANK)
    }
}

/// Une image complète, terminée par un `\n` par rangée (`t3_snapshot` du C).
///
/// Écrit dans `out` ; retourne le nombre d'octets écrits.
///
/// # Errors
///
/// Toute erreur d'écriture du `Write` fourni.
pub fn snapshot<W: Write>(out: &mut W, cells: &CellBuffer, palette: Palette) -> io::Result<u32> {
    let mut buffer = Vec::with_capacity(cells.cols().max(0) as usize * 4);
    let mut fg: i32 = -1;
    let mut bg: i32 = -1;
    for y in 0..cells.rows() {
        let Some(row) = cells.row(y) else {
            continue;
        };
        for cell in row {
            emit(&mut buffer, cell, palette, &mut fg, &mut bg);
        }
        if palette != Palette::Plain {
            buffer.extend_from_slice(b"\x1b[0m");
            fg = -1;
            bg = -1;
        }
        buffer.push(b'\n');
    }
    out.write_all(&buffer)?;
    out.flush()?;
    u32::try_from(buffer.len()).map_err(|_| io::Error::other("image ANSI trop grande"))
}

/// `equal_color` du C : égalité canal par canal.
#[inline]
fn color_equals(a: u32, b: u32) -> bool {
    a == b || rgb_channels(a) == rgb_channels(b)
}

/// `equal_cell` du C : `fg` ignoré sur un espace (la couleur d'encre n'a pas de sens).
#[must_use]
pub fn cell_equals(a: &Cell, b: &Cell) -> bool {
    a.glyph == b.glyph
        && color_equals(a.bg, b.bg)
        && (a.glyph == u32::from(' ') || color_equals(a.fg, b.fg))
}

/// Positionnement curseur `\x1b[{y};{x}H` (coordonnées 1-based).
fn push_position(out: &mut Vec<u8>, y: i32, x: i32) {
    let mut scratch = [0u8; 24];
    let text = format_into(&mut scratch, y, Some(x));
    out.extend_from_slice(text);
}

/// Formate `\x1b[y;xH` sans allocation (les valeurs tiennent en 5 chiffres).
fn format_into(scratch: &mut [u8; 24], a: i32, b: Option<i32>) -> &[u8] {
    let mut cursor = Cursor::new(scratch);
    cursor.bytes(b"\x1b[");
    cursor.decimal(a);
    if let Some(b) = b {
        cursor.byte(b';');
        cursor.decimal(b);
    }
    cursor.byte(b'H');
    cursor.finish()
}

/// Petit écrivain de chiffres dans un slice, sans allocation ni `unsafe`.
struct Cursor<'a> {
    buf: &'a mut [u8; 24],
    len: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a mut [u8; 24]) -> Self {
        Self { buf, len: 0 }
    }

    fn byte(&mut self, b: u8) {
        if let Some(slot) = self.buf.get_mut(self.len) {
            *slot = b;
            self.len += 1;
        }
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte(b);
        }
    }

    fn decimal(&mut self, value: i32) {
        let mut digits = [0u8; 10];
        let mut n = value.max(0) as u32;
        let mut count = 0usize;
        loop {
            digits[count] = b'0' + (n % 10) as u8;
            count += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        while count > 0 {
            count -= 1;
            self.byte(digits[count]);
        }
    }

    fn finish(self) -> &'a [u8] {
        let len = self.len;
        &self.buf[..len]
    }
}

/// `utf8` du C : 1 à 3 octets, jamais plus (les glyphes émis sont `< U+FFFF`).
fn push_utf8(out: &mut Vec<u8>, c: u32) {
    if c < 128 {
        out.push(c as u8);
    } else if c < 2048 {
        out.push(192 | (c >> 6) as u8);
        out.push(128 | (c & 63) as u8);
    } else {
        out.push(224 | (c >> 12) as u8);
        out.push(128 | ((c >> 6) & 63) as u8);
        out.push(128 | (c & 63) as u8);
    }
}

/// `nearest_cube` du C.
fn nearest_cube(value: i32) -> i32 {
    if value < 48 {
        0
    } else if value < 115 {
        1
    } else {
        (value - 35) / 40
    }
}

/// `indexed` du C : couleur 256 la plus proche, cube 6·6·6 ou rampe de gris.
#[must_use]
pub fn indexed(color: u32) -> i32 {
    const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    let (r, g, b) = rgb_channels(color);
    let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
    let (ri, gi, bi) = (nearest_cube(r), nearest_cube(g), nearest_cube(b));
    let dr = r - LEVELS[ri as usize];
    let dg = g - LEVELS[gi as usize];
    let db = b - LEVELS[bi as usize];
    // Division entière tronquante comme le C (le numérateur peut être négatif).
    let gray = (((r + g + b) / 3 - 3) / 10).clamp(0, 23);
    let v = 8 + 10 * gray;
    let (gr, gg, gb) = (r - v, g - v, b - v);
    if gr * gr + gg * gg + gb * gb < dr * dr + dg * dg + db * db {
        232 + gray
    } else {
        16 + 36 * ri + 6 * gi + bi
    }
}

/// `ink` du C : n'émet la couleur que si sa clé change.
fn ink(out: &mut Vec<u8>, channel: u8, color: u32, palette: Palette, last: &mut i32) {
    let key = if palette == Palette::Ansi256 {
        indexed(color)
    } else {
        let (r, g, b) = rgb_channels(color);
        (i32::from(r) << 16) | (i32::from(g) << 8) | i32::from(b)
    };
    if key == *last {
        return;
    }
    *last = key;
    match palette {
        Palette::Ansi256 => {
            out.extend_from_slice(format!("\x1b[{channel};5;{key}m").as_bytes());
        }
        _ => {
            let (r, g, b) = rgb_channels(color);
            out.extend_from_slice(format!("\x1b[{channel};2;{r};{g};{b}m").as_bytes());
        }
    }
}

/// `emit` du C : fond, puis encre (sauf espace), puis le glyphe.
fn emit(out: &mut Vec<u8>, cell: &Cell, palette: Palette, fg: &mut i32, bg: &mut i32) {
    if palette != Palette::Plain {
        ink(out, 48, cell.bg, palette, bg);
        if cell.glyph != u32::from(' ') {
            ink(out, 38, cell.fg, palette, fg);
        }
    }
    push_utf8(out, cell.glyph);
}

#[cfg(test)]
mod tests {
    // Voir astre-scene : exception locale et justifiée pour le code de test.
    #![allow(clippy::expect_used)]

    use super::*;
    use astre_core::math::rgb_pack;

    fn buffer(cols: i32, rows: i32) -> CellBuffer {
        CellBuffer::new(cols, rows).expect("grille de test")
    }

    fn put(buf: &mut CellBuffer, x: i32, y: i32, glyph: char, fg: u32, bg: u32) {
        buf.set(x, y, Cell::solid(u32::from(glyph), fg, bg));
    }

    /// Grille 2 x 2 de référence des tests de séquence exacte.
    fn fixture() -> CellBuffer {
        let mut b = buffer(2, 2);
        put(
            &mut b,
            0,
            0,
            'A',
            rgb_pack(0x11, 0x22, 0x33),
            rgb_pack(0x44, 0x55, 0x66),
        );
        put(
            &mut b,
            1,
            0,
            'B',
            rgb_pack(0x11, 0x22, 0x33),
            rgb_pack(0x44, 0x55, 0x66),
        );
        put(
            &mut b,
            0,
            1,
            ' ',
            rgb_pack(0, 0, 0),
            rgb_pack(0x77, 0x88, 0x99),
        );
        put(
            &mut b,
            1,
            1,
            ' ',
            rgb_pack(0xAA, 0xBB, 0xCC),
            rgb_pack(0x77, 0x88, 0x99),
        );
        b
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn first_frame_is_complete_and_byte_exact() {
        let cells = fixture();
        let mut enc = AnsiEncoder::new();
        enc.encode_present(&cells, Palette::Rgb);
        let expected = concat!(
            "\x1b[1;1H",
            "\x1b[48;2;68;85;102m",
            "\x1b[38;2;17;34;51m",
            "AB",
            "\x1b[2;1H",
            "\x1b[48;2;119;136;153m",
            "  ",
        );
        assert_eq!(text(enc.buffer()), expected);
        assert!(enc.is_full());
    }

    #[test]
    fn diff_emits_one_positioning_and_one_cell() {
        let cells = fixture();
        let mut enc = AnsiEncoder::new();
        enc.encode_present(&cells, Palette::Rgb);
        enc.commit(&cells, Palette::Rgb);

        let mut changed = cells.clone();
        put(
            &mut changed,
            1,
            0,
            'C',
            rgb_pack(0x11, 0x22, 0x33),
            rgb_pack(0x44, 0x55, 0x66),
        );
        enc.encode_present(&changed, Palette::Rgb);
        let expected = concat!(
            "\x1b[1;2H",
            "\x1b[48;2;68;85;102m",
            "\x1b[38;2;17;34;51m",
            "C",
        );
        assert_eq!(text(enc.buffer()), expected);
        assert!(!enc.is_full());
        // Une seule séquence de positionnement.
        assert_eq!(text(enc.buffer()).matches('\x1b').count() - 2, 1);
    }

    #[test]
    fn contiguous_changes_share_one_positioning() {
        let mut cells = buffer(4, 1);
        for x in 0..4 {
            put(&mut cells, x, 0, '.', rgb_pack(1, 2, 3), rgb_pack(4, 5, 6));
        }
        let mut enc = AnsiEncoder::new();
        enc.encode_present(&cells, Palette::Rgb);
        enc.commit(&cells, Palette::Rgb);

        for x in 1..4 {
            put(&mut cells, x, 0, '#', rgb_pack(1, 2, 3), rgb_pack(4, 5, 6));
        }
        enc.encode_present(&cells, Palette::Rgb);
        let out = text(enc.buffer());
        assert_eq!(out.matches("\x1b[1;2H").count(), 1, "{out:?}");
        assert_eq!(out.matches("\x1b[").count(), 3, "{out:?}"); // position + fg + bg
        assert!(out.ends_with("###"), "{out:?}");
    }

    #[test]
    fn colour_memory_spans_the_whole_frame() {
        let mut cells = buffer(4, 1);
        put(&mut cells, 0, 0, 'a', rgb_pack(1, 2, 3), rgb_pack(4, 5, 6));
        put(&mut cells, 1, 0, ' ', rgb_pack(9, 9, 9), rgb_pack(4, 5, 6));
        put(&mut cells, 2, 0, 'b', rgb_pack(1, 2, 3), rgb_pack(4, 5, 6));
        put(&mut cells, 3, 0, 'c', rgb_pack(1, 2, 3), rgb_pack(4, 5, 6));
        let mut enc = AnsiEncoder::new();
        enc.encode_present(&cells, Palette::Rgb);
        let out = text(enc.buffer());
        assert_eq!(out.matches("\x1b[48;2;4;5;6m").count(), 1, "{out:?}");
        assert_eq!(out.matches("\x1b[38;2;1;2;3m").count(), 1, "{out:?}");
    }

    #[test]
    fn fg_is_ignored_on_spaces() {
        let cells = fixture();
        let mut enc = AnsiEncoder::new();
        enc.encode_present(&cells, Palette::Rgb);
        enc.commit(&cells, Palette::Rgb);

        let mut changed = cells.clone();
        // Même glyphe espace, même fond, encre différente : rien à réémettre.
        put(
            &mut changed,
            1,
            1,
            ' ',
            rgb_pack(1, 1, 1),
            rgb_pack(0x77, 0x88, 0x99),
        );
        enc.encode_present(&changed, Palette::Rgb);
        assert!(enc.buffer().is_empty(), "{:?}", text(enc.buffer()));
        let a = changed.get(1, 1).copied().unwrap_or(Cell::BLANK);
        let b = cells.get(1, 1).copied().unwrap_or(Cell::BLANK);
        assert!(cell_equals(&a, &b));
    }

    #[test]
    fn size_or_palette_change_forces_a_full_redraw() {
        let cells = fixture();
        let mut enc = AnsiEncoder::new();
        enc.encode_present(&cells, Palette::Rgb);
        enc.commit(&cells, Palette::Rgb);

        enc.encode_present(&cells, Palette::Ansi256);
        assert!(enc.is_full());
        assert!(!enc.buffer().is_empty());
        enc.commit(&cells, Palette::Ansi256);

        let bigger = buffer(3, 2);
        enc.encode_present(&bigger, Palette::Ansi256);
        assert!(enc.is_full());
        enc.commit(&bigger, Palette::Ansi256);

        // Une invalidation explicite (reprise après Ctrl-Z) force aussi le complet.
        enc.invalidate();
        enc.encode_present(&bigger, Palette::Ansi256);
        assert!(enc.is_full());
    }

    #[test]
    fn plain_palette_writes_glyphs_only() {
        let cells = fixture();
        let mut out = Vec::new();
        let n = snapshot(&mut out, &cells, Palette::Plain).expect("snapshot");
        assert_eq!(text(&out), "AB\n  \n");
        assert_eq!(n as usize, out.len());
    }

    #[test]
    fn snapshot_ends_each_line_with_a_reset() {
        let cells = fixture();
        let mut out = Vec::new();
        snapshot(&mut out, &cells, Palette::Rgb).expect("snapshot");
        let text = text(&out);
        // `t3_snapshot` n'émet aucun positionnement curseur : seulement les couleurs
        // et les glyphes, ligne par ligne.
        assert!(
            text.starts_with("\x1b[48;2;68;85;102m\x1b[38;2;17;34;51mAB\x1b[0m\n"),
            "{text:?}"
        );
        assert!(!text.contains('H'));
        assert_eq!(text.matches("\x1b[0m\n").count(), 2, "{text:?}");
        assert_eq!(text.matches('\n').count(), 2);
    }

    #[test]
    fn braille_glyphs_serialise_on_three_bytes() {
        let mut cells = buffer(1, 1);
        put(&mut cells, 0, 0, '\u{28FF}', 0, 0);
        let mut out = Vec::new();
        snapshot(&mut out, &cells, Palette::Plain).expect("snapshot");
        assert_eq!(&out[..3], &[0xE2, 0xA3, 0xBF]);
    }

    #[test]
    fn indexed_matches_the_c_reference_values() {
        // Valeurs calculées avec le C (`terminal.c:indexed`).
        assert_eq!(indexed(rgb_pack(0, 0, 0)), 16);
        assert_eq!(indexed(rgb_pack(255, 255, 255)), 231);
        assert_eq!(indexed(rgb_pack(128, 128, 128)), 244);
        assert_eq!(indexed(rgb_pack(68, 85, 102)), 240);
        assert_eq!(indexed(rgb_pack(119, 136, 153)), 102);
        assert_eq!(indexed(rgb_pack(4, 5, 6)), 232);
        assert_eq!(indexed(rgb_pack(125, 228, 206)), 116);
        assert_eq!(indexed(rgb_pack(236, 234, 217)), 254);
        assert_eq!(indexed(rgb_pack(20, 32, 46)), 234);
    }

    #[test]
    fn failed_write_keeps_the_previous_frame() {
        /// `Write` qui échoue une fois, puis accepte.
        struct Flaky {
            fail: bool,
            data: Vec<u8>,
        }
        impl Write for Flaky {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if self.fail {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "test"));
                }
                self.data.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let cells = fixture();
        let mut enc = AnsiEncoder::new();
        let mut sink = Flaky {
            fail: true,
            data: Vec::new(),
        };

        // Première présentation : l'écriture échoue -> pas de commit.
        enc.encode_present(&cells, Palette::Rgb);
        let first = enc.buffer().to_vec();
        assert!(sink.write_all(&first).is_err());
        // La frame est toujours considérée comme jamais présentée.
        enc.encode_present(&cells, Palette::Rgb);
        assert_eq!(enc.buffer(), first.as_slice());
        assert!(enc.is_full());

        sink.fail = false;
        enc.encode_present(&cells, Palette::Rgb);
        sink.write_all(enc.buffer()).expect("écriture");
        enc.commit(&cells, Palette::Rgb);
        enc.encode_present(&cells, Palette::Rgb);
        assert!(enc.buffer().is_empty());
    }
}
