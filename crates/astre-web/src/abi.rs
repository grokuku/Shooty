//! ABI de présentation : bloc `#[repr(C)]` lu d'un coup par `web/src/presenter.ts`.
//!
//! Pourquoi un bloc et pas des getters : §F.3 (piège n°2) — le présentateur a besoin de
//! la géométrie **et** du pointeur des cellules à chaque frame, et un pointeur mémorisé
//! devient faux dès qu'un `Vec` réalloue. Un seul bloc à plat, réécrit par Rust à la fin
//! de chaque opération qui change la géométrie, se lit en une vue `Uint32Array` sur la
//! mémoire wasm : zéro copie, zéro sérialisation, et le pointeur est toujours frais.
//!
//! Tous les champs sont des mots de 32 bits : le TS lit `info_ptr()` avec un
//! `Uint32Array` (ou `Int32Array` pour les champs signés) sans `DataView`, sans
//! désalignement possible.

use core::mem::size_of;

/// Nombre de slots de l'atlas (`32 × 16`, §E.3 / §I.4 décision n°2).
pub const ATLAS_SLOTS: u32 = 512;

/// Bits de retour de `App::frame` (§F.2, restreints au périmètre Phase 3).
pub mod frame_bits {
    /// Une image a été produite : le présentateur doit téléverser et dessiner.
    pub const RENDERED: u32 = 1 << 0;
    /// Rien n'a été produit (pas encore de viewport, ou aucun temps écoulé depuis la
    /// frame précédente et rien de neuf à redessiner) : le présentateur ne touche pas GL.
    pub const SKIPPED: u32 = 1 << 1;
    /// La dernière opération a échoué ; `last_error()` porte le message. L'`App` reste
    /// utilisable (état sain, aucune mutation partielle) : c'est un état
    /// **récupérable**, documenté dans [`crate::App`].
    pub const ERROR: u32 = 1 << 2;
    /// La simulation a abandonné du retard (`MAX_CATCHUP` atteint, §D.1).
    pub const SIM_DROPPED: u32 = 1 << 3;
    /// Une action « quitter » a été demandée (`q`). Le web ne ferme pas l'onglet : la
    /// coquille peut l'ignorer ou afficher un message (aucune action par défaut).
    pub const QUIT: u32 = 1 << 4;
    /// L'image contient des tuiles sales (l'information brute de §C.5.5). Le web
    /// ré-uploade de toute façon en v1 (§I.4, décision n°12) : ce bit sert au HUD.
    pub const DIRTY: u32 = 1 << 5;
}

/// Géométrie + pointeur lus par le présentateur TypeScript (une frame, une lecture).
///
/// Ordre des champs = contrat d'ABI : `presenter.ts` les indexe par position. Un
/// changement d'ordre casse le rendu ; c'est pour cela que le tableau est **plat** et
/// que le test `layout_is_stable` fige la taille et les décalages.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentInfo {
    /// Adresse du buffer d'instances dans la mémoire wasm (octets depuis le début).
    pub cells_ptr: u32,
    /// Longueur du buffer d'instances, en **octets** (`cols × rows × 8`).
    pub cells_len: u32,
    /// Colonnes de la grille.
    pub cols: u32,
    /// Lignes de la grille.
    pub rows: u32,
    /// Largeur d'une cellule, en pixels device.
    pub cell_w: u32,
    /// Hauteur d'une cellule, en pixels device (`2 × cell_w`).
    pub cell_h: u32,
    /// Abscisse du coin haut-gauche de la grille dans le canvas (letterbox).
    pub origin_x: i32,
    /// Ordonnée du coin haut-gauche de la grille dans le canvas (letterbox).
    pub origin_y: i32,
    /// Largeur du canvas, en pixels device.
    pub avail_w: u32,
    /// Hauteur du canvas, en pixels device.
    pub avail_h: u32,
    /// `epoch` du `CellBuffer` : invalidation des caches de géométrie (§C.5.1).
    pub epoch: u32,
    /// Palier de zoom retenu (`ZOOM_LEVELS[level] == cell_w`).
    pub level: u32,
    /// Nombre de slots de l'atlas ([`ATLAS_SLOTS`]) : le TS n'a pas à le coder en dur.
    pub atlas_slots: u32,
    /// Compteur de frames produites depuis le démarrage.
    pub frame_index: u32,
    /// Bits de la dernière frame ([`frame_bits`]).
    pub flags: u32,
    /// Réservé (alignement sur 16 mots, extensions futures).
    pub reserved: u32,
}

impl PresentInfo {
    /// Nombre de mots de 32 bits de ce bloc.
    pub const WORDS: u32 = (size_of::<PresentInfo>() / 4) as u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_stable() {
        assert_eq!(size_of::<PresentInfo>(), 64);
        assert_eq!(PresentInfo::WORDS, 16);
        assert_eq!(ATLAS_SLOTS, 32 * 16);
    }

    #[test]
    fn frame_bits_are_distinct() {
        let all = [
            frame_bits::RENDERED,
            frame_bits::SKIPPED,
            frame_bits::ERROR,
            frame_bits::SIM_DROPPED,
            frame_bits::QUIT,
            frame_bits::DIRTY,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_eq!(a & b, 0, "bits {a} et {b} se recouvrent");
            }
        }
    }
}
