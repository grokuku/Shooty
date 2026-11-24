//! Projection `Cell` (16 o) → instance GPU (8 o) et table glyphe → slot d'atlas.
//!
//! Instance (8 o, §F.4 **amendé en Phase 3**) :
//!
//! ```text
//! word0 = fg (0x00RRGGBB, 24 bits bas) | slot[7:0] (8 bits hauts)
//! word1 = bg (0x00RRGGBB, 24 bits bas) | flags[3:0] << 24 | slot[8] << 31
//! ```
//!
//! **Écart assumé avec §F.4.** La spec prévoyait `slot` sur 8 bits et un octet de
//! `flags` entier, tout en fixant par ailleurs (§E.3 / §I.4 décision n°2) un atlas de
//! **512 slots** — soit 9 bits. Les deux ne tiennent pas ensemble : avec 8 bits, les
//! slots 256..=511 (Braille haut et blocs) débordent et se projettent sur des slots
//! ASCII (glyphes parasites), ce que la vérification visuelle de la Phase 3 a
//! effectivement révélé. L'arbitrage retenu garde les **8 o par instance** (§A.4,
//! contrainte dure), les **24 bits par couleur** et les **512 slots** :
//!
//! - `slot` occupe 9 bits : les 8 bits hauts de `word0` + le **bit 31** de `word1` ;
//! - `flags` ne garde que ses **4 bits définis** (`FLAG_UNDERLINE`…) dans les bits
//!   24..=27 de `word1`, les bits 28..=30 restant réservés.
//!
//! `Cell` fait 16 o et l'instance 8 o : la projection reste **lossy assumée** sur
//! `flags` (`u16` → 4 bits utilisés ; le présentateur terminal, lui, garde les 16 bits).
//!
//! ## Table glyphe → slot
//!
//! C'est un contrat d'ABI avec `web/src/atlas.ts`, qui dessine les glyphes dans cet
//! ordre (occupation de §E.3) :
//!
//! | Plage de slots | Contenu | Codepoints |
//! | --- | --- | --- |
//! | `0..=95` | ASCII imprimable | `U+0020..=U+007E` (96 glyphes) |
//! | `96..=351` | Braille | `U+2800..=U+28FF` |
//! | `352..=383` | Blocs | `U+2580..=U+259F` |
//! | `384..=511` | réservé | — |
//!
//! Un glyphe hors de ces plages (ne devrait jamais arriver : §C.5.4 invariant I4) est
//! projeté sur le slot de `'?'`, qui existe toujours.

use astre_core::{Cell, CellBuffer};

/// Slot du glyphe de repli (`'?'`, `U+003F` → slot `31`).
pub const FALLBACK_SLOT: u32 = 31;

/// Nombre de slots ASCII (`U+0020..=U+007E`).
pub const ASCII_SLOTS: u32 = 96;
/// Premier slot Braille (`U+2800`).
pub const BRAILLE_FIRST_SLOT: u32 = ASCII_SLOTS;
/// Premier slot des blocs (`U+2580`).
pub const BLOCK_FIRST_SLOT: u32 = BRAILLE_FIRST_SLOT + 256;

/// Slot d'atlas d'un glyphe (contrat d'ABI avec `atlas.ts`).
#[must_use]
pub const fn slot_for(glyph: u32) -> u32 {
    match glyph {
        0x20..=0x7E => glyph - 0x20,
        0x2800..=0x28FF => BRAILLE_FIRST_SLOT + (glyph - 0x2800),
        0x2580..=0x259F => BLOCK_FIRST_SLOT + (glyph - 0x2580),
        _ => FALLBACK_SLOT,
    }
}

/// Encode une cellule en deux mots d'instance GPU.
///
/// Voir l'en-tête du module pour le layout exact (9 bits de slot répartis sur les deux
/// mots, 4 bits de `flags`).
#[inline]
#[must_use]
pub const fn encode(cell: &Cell) -> (u32, u32) {
    let slot = slot_for(cell.glyph);
    let word0 = (cell.fg & 0x00FF_FFFF) | ((slot & 0xFF) << 24);
    let word1 =
        (cell.bg & 0x00FF_FFFF) | ((cell.flags as u32 & 0x0F) << 24) | (((slot >> 8) & 1) << 31);
    (word0, word1)
}

/// Slot décodé depuis une instance — miroir exact du vertex shader, pour les tests.
#[must_use]
pub const fn slot_of(word0: u32, word1: u32) -> u32 {
    ((word0 >> 24) & 0xFF) | (((word1 >> 31) & 1) << 8)
}

/// `fg` décodé depuis une instance.
#[must_use]
pub const fn fg_of(word0: u32) -> u32 {
    word0 & 0x00FF_FFFF
}

/// `bg` décodé depuis une instance.
#[must_use]
pub const fn bg_of(word1: u32) -> u32 {
    word1 & 0x00FF_FFFF
}

/// `flags` (4 bits) décodés depuis une instance.
#[must_use]
pub const fn flags_of(word1: u32) -> u32 {
    (word1 >> 24) & 0x0F
}

/// Écrit les instances de tout le buffer dans `out` (`cols × rows × 2` mots).
///
/// Le buffer de sortie est réutilisé d'une frame à l'autre (`out` est redimensionné
/// seulement si la grille change) : aucune allocation sur le chemin chaud tant que la
/// taille ne bouge pas (§C.5.7.5, §H.5).
///
/// Le `stride` du `CellBuffer` est respecté : le padding n'est jamais lu (§C.5.4 I8).
pub fn pack_instances(cells: &CellBuffer, out: &mut Vec<u32>) -> usize {
    let cols = cells.cols().max(0) as usize;
    let rows = cells.rows().max(0) as usize;
    let words = cols * rows * 2;
    if out.len() != words {
        out.clear();
        out.resize(words, 0);
    }

    let mut w = 0usize;
    for y in 0..rows {
        let Some(row) = cells.row(y as i32) else {
            continue;
        };
        for cell in row.iter().take(cols) {
            let (word0, word1) = encode(cell);
            if let Some(a) = out.get_mut(w) {
                *a = word0;
            }
            if let Some(b) = out.get_mut(w + 1) {
                *b = word1;
            }
            w += 2;
        }
    }
    words
}

/// Nombre de mots nécessaires pour `cols × rows` cellules.
#[must_use]
pub const fn words_for(cols: u32, rows: u32) -> usize {
    (cols as usize) * (rows as usize) * 2
}

/// Nombre d'octets du buffer d'instances.
#[must_use]
pub const fn bytes_for(cols: u32, rows: u32) -> u32 {
    words_for(cols, rows) as u32 * 4
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn ascii_slots_start_at_zero_and_end_at_95() {
        assert_eq!(slot_for(' ' as u32), 0);
        assert_eq!(slot_for('A' as u32), 33);
        assert_eq!(slot_for('?' as u32), 31);
        assert_eq!(slot_for('~' as u32), 94);
        assert_eq!(slot_for(0x7F), FALLBACK_SLOT);
    }

    #[test]
    fn braille_and_block_ranges_are_contiguous() {
        assert_eq!(slot_for(0x2800), 96);
        assert_eq!(slot_for(0x28FF), 96 + 255);
        assert_eq!(slot_for(0x2580), 352);
        assert_eq!(slot_for(0x259F), 383);
        assert_eq!(slot_for(0x2500), FALLBACK_SLOT); // box drawing : non produit
    }

    #[test]
    fn every_slot_stays_inside_the_atlas() {
        for glyph in 0u32..=0x10FFFF {
            // Balayage complet : un glyphe hors plage ne doit jamais déborder.
            let slot = slot_for(glyph);
            assert!(slot < crate::abi::ATLAS_SLOTS);
        }
    }

    #[test]
    fn encode_packs_colors_and_flags() {
        let cell = Cell::new('A' as u32, 0x00AB_CDEF, 0x0012_3456, 0b0001_0001);
        let (w0, w1) = encode(&cell);
        assert_eq!(fg_of(w0), 0x00AB_CDEF);
        assert_eq!(bg_of(w1), 0x0012_3456);
        assert_eq!(slot_of(w0, w1), slot_for('A' as u32));
        assert_eq!(flags_of(w1), 0b0001);
    }

    #[test]
    fn high_color_bits_never_leak_into_the_slot() {
        let cell = Cell::new('x' as u32, 0xFF12_3456, 0xFF65_4321, 0);
        let (w0, w1) = encode(&cell);
        assert_eq!(slot_of(w0, w1), slot_for('x' as u32));
        assert_eq!(flags_of(w1), 0);
        assert_eq!(fg_of(w0), 0x0012_3456);
        assert_eq!(bg_of(w1), 0x0065_4321);
    }

    #[test]
    fn every_slot_round_trips_through_the_instance_words() {
        // Le bug le plus coûteux de la Phase 3 : un slot sur 8 bits débordait pour
        // `slot >= 256` (Braille haut, blocs) et se projetait sur des slots ASCII.
        for glyph in [0x28u32, 0x7Eu32, 0x2800u32, 0x28FFu32, 0x2580u32, 0x259Fu32] {
            let cell = Cell::new(glyph, 0x0000_00FF, 0x0000_FF00, 0);
            let (w0, w1) = encode(&cell);
            assert_eq!(slot_of(w0, w1), slot_for(glyph), "glyphe U+{glyph:04X}");
            assert!(slot_of(w0, w1) < 512);
            assert_eq!(fg_of(w0), 0x0000_00FF);
            assert_eq!(bg_of(w1), 0x0000_FF00);
        }
    }

    #[test]
    fn all_512_slots_survive_the_encoding() {
        // Tous les slots atteignables par la table, décodés sans collision.
        for slot in 0..512u32 {
            let w0 = slot & 0xFF;
            let w1 = ((slot >> 8) & 1) << 31;
            assert_eq!(slot_of(w0 << 24, w1), slot, "slot {slot}");
        }
    }

    #[test]
    fn pack_respects_the_grid_and_reuses_the_buffer() {
        let mut cells = CellBuffer::new(4, 3).unwrap();
        cells.set(0, 0, Cell::new('A' as u32, 1, 2, 0));
        cells.set(3, 2, Cell::new('B' as u32, 3, 4, 0));
        let mut out: Vec<u32> = Vec::new();
        let words = pack_instances(&cells, &mut out);
        assert_eq!(words, 4 * 3 * 2);
        assert_eq!(out.len(), words);
        assert_eq!(out[0], 1 | (slot_for('A' as u32) << 24));
        // Dernière cellule (x = 3, y = 2).
        let last = (2 * 4 + 3) * 2;
        assert_eq!(out[last], 3 | (slot_for('B' as u32) << 24));
        assert_eq!(bytes_for(4, 3), 96);

        // Même taille : le Vec n'est pas réalloué (capacité conservée).
        let capacity = out.capacity();
        pack_instances(&cells, &mut out);
        assert_eq!(out.capacity(), capacity);
    }
}
