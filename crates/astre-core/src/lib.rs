//! # astre-core
//!
//! Cœur du moteur Astre — port Rust de `test3Dterminal/term3d.c` (C-REF, gelé le
//! 2026-11-24), conformément à `docs/ARCHITECTURE.md` §B.2 et §G.1.
//!
//! ## Contenu
//!
//! | Module | Origine C | Rôle |
//! | --- | --- | --- |
//! | [`math`] | `t3_vec`, `t3_rgb`, `t3_mix`, `clampf`, `dot`/`cross`/`unit` | vecteurs, couleurs, caméra, matrice 4x4 |
//! | [`cell`] | `T3Cell`, `t3_resize`, `t3_cols`/`t3_rows`/`t3_cells` | `Cell`, `CellRect`, `CellBuffer`, dirty tiles |
//! | [`clip`] | boucle de clipping de `t3_triangle` | Sutherland–Hodgman plan proche |
//! | [`raster`] | `t3_begin`, `view`, `project`, `edge`, `raster` | cible d'échantillons 2 x 4, profondeur `1/z` |
//! | [`resolve`] | `shapes`, `bits`, `distance`, `resolve_cell` | glyphes ASCII / Braille / blocs |
//! | [`hud`] | `t3_fill`, `t3_text` | rectangles et texte ASCII minimal |
//! | [`viewport`] | nouveau | `PixelSize`, `Viewport`, `SurfaceView` (pixels device, présentateur) |
//! | [`zoom`] | nouveau (Phase 3) | paliers de zoom discrets, `fit` (grille + letterbox, §E) |
//!
//! ## Non porté (Phase 1)
//!
//! - `t3_import_rgb565` : réservé à `test3Dterminal/quake/`, hors périmètre (§G.1).
//! - La caméra orbitale (`demo.c:camera_for`), le ciel et le décor : `astre-scene`
//!   et `astre-app`, Phases 2+ (crates présentes, squelettes seulement).
//!
//! ## Écart assumé avec la spec, tranché ici
//!
//! §G.7 / §I.3 (6) prévoyaient `no_std` + la crate `libm` pour le déterminisme
//! inter-cibles. La Phase 1 applique la consigne de portage : **aucune dépendance
//! externe**, donc pas de `libm`, et la trigonométrie passe par la `f32` de la `std`
//! (`f32::tan`, `f32::sqrt`). Conséquences :
//!
//! - `astre-core` est un crate `std` (le reste de l'API n'utilise que `core`) ;
//! - sur une même cible (Linux/x86-64, la cible des goldens), `f32::tan` appelle
//!   `tanf` de la libm système — donc **exactement** la fonction utilisée par le C
//!   compilé par `gcc`. Les goldens C <-> Rust restent valables ;
//! - entre cibles, une divergence d'un ULP sur `tan`/`atan2` reste possible : c'est
//!   le risque P1 de §I.1, à traiter en Phase 2 si `astre-web` doit partager les
//!   goldens natifs (bascule vers `libm` derrière la feature `std-math` de §B.5.1).
//!
//! ## Règles tenues
//!
//! - aucune dépendance, aucun état global, aucun `unsafe` (lints du workspace) ;
//! - aucune entrée hors bornes ne panique : `Result`/`Option` partout ;
//! - toutes les tables de correspondance sont des `const` calculées au compile.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cell;
pub mod clip;
pub mod hud;
pub mod math;
pub mod raster;
pub mod resolve;
pub mod viewport;
pub mod zoom;

pub use cell::{
    BufferError, Cell, CellBuffer, CellRect, DirtyTiles, Mode, Palette, ResizePolicy, MAX_CELLS,
    MAX_DIM, TILE_H, TILE_W,
};
pub use clip::near_plane;
pub use hud::Anchor;
pub use math::{rgb, rgb_channels, rgb_pack, Camera, CameraBasis, Vec2, Vec3};
pub use raster::{BlackSky, Projected, RasterTarget, Renderer, Sky, SUBSAMPLE_X, SUBSAMPLE_Y};
pub use resolve::{resolve_cell, AsciiShape, ASCII_LOOKUP};
pub use viewport::{PixelSize, SurfaceView, Viewport};
pub use zoom::{auto_level, fit, nearest_level, step_level, Grid, ZOOM_LEVELS};
