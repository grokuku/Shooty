//! Golden de parité C <-> Rust (docs/ARCHITECTURE.md §H.2 (a)).
//!
//! Référence : `test3Dterminal/term3d.c`, **gelé** (oracle immuable, §I.4.3). Le C
//! n'a pas été modifié : il a été **copié** dans `/tmp/astre-oracle/` avec le petit
//! programme `golden.c` ci-dessous (qui n'utilise que l'API publique `term3d.h`),
//! compilé par `cc -std=c99 -O2 -Wall -Wextra -Wpedantic -lm`, et exécuté pour
//! produire les trois valeurs attendues ci-dessous.
//!
//! ## Cas déterministe (aucune horloge, aucun aléa)
//!
//! - grille 40 x 20 ;
//! - caméra de `tests/render.c` : `eye = (0,0,0)`, `target = (0,0,-1)`,
//!   `fov_y = 1.0`, `near_z = 0.5`, `cell_aspect = 0.5` ;
//! - `begin(camera, NULL, NULL)` (ciel noir, comme le C) ;
//! - deux triangles double-face (rouge `250,34,20` à `z = -3`, bleu `10,40,220`
//!   à `z = -2.2`) — profondeurs différentes, donc le test couvre le test `1/z` ;
//! - puis `resolve(ASCII)`, `resolve(BRAILLE)`, et enfin HUD
//!   (`fill(-4, 19, 9, 7, bleu)`, `text(BOTTOM_RIGHT, 1, 1, "HUD")`,
//!   `text(TOP_LEFT, -2, 0, "ABCDE")`) — les trois mêmes appels que
//!   `tests/render.c:hud`, y compris les offsets négatifs.
//!
//! ## Hash
//!
//! FNV-1a 64 bits sur la suite, dans l'ordre ligne par ligne :
//! `glyph`, `fg` (packé `0x00RRGGBB`), `bg`, `flags` (toujours `0` ici), chacun en
//! 4 octets petit-boutistes. Implémentation identique des deux côtés (`hash_cells`
//! dans `golden.c`, [`hash_cells`] ici).
//!
//! ## Régénération
//!
//! ```
//! mkdir -p /tmp/astre-oracle && cd /tmp/astre-oracle
//! cp /projects/Shooty/test3Dterminal/term3d.{c,h} .
//! # ... recopier golden.c depuis l'en-tête de ce fichier ...
//! cc -std=c99 -O2 -o golden golden.c term3d.c -lm && ./golden
//! ```
//!
//! Un diff non nul est une **régression** : le C ne doit pas être retouché (§I.4.3).

use astre_core::cell::{CellBuffer, Mode};
use astre_core::hud::{self, Anchor};
use astre_core::math::{rgb, Vec3};
use astre_core::raster::{BlackSky, Renderer};
use astre_core::Camera;
use astre_core::CellRect;

const RED: u32 = 0x00FA_2214; // rgb(250, 34, 20)
const BLUE: u32 = 0x000A_28DC; // rgb(10, 40, 220)

/// Hash FNV-1a 64 du `CellBuffer` (même alimentation que `golden.c`).
fn hash_cells(buf: &CellBuffer) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for cell in buf.cells() {
        for word in [cell.glyph, cell.fg, cell.bg, u32::from(cell.flags)] {
            for byte in word.to_le_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    h
}

/// Le cas commun : caméra de `tests/render.c`, deux triangles, ciel noir.
fn fixed_scene() -> Renderer {
    let camera = Camera::new(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, -1.0),
        1.0,
        0.5,
        0.5,
    );
    let mut r = match Renderer::new(40, 20) {
        Ok(r) => r,
        Err(e) => panic!("Renderer::new(40, 20) : {e}"),
    };
    r.begin(camera, CellRect::full(40, 20), None::<&mut BlackSky>);
    r.triangle(
        Vec3::new(-1.17, -0.77, -3.0),
        Vec3::new(1.28, -0.61, -3.0),
        Vec3::new(-0.19, 1.07, -3.0),
        RED,
    );
    r.triangle(
        Vec3::new(-2.5, 0.4, -2.2),
        Vec3::new(-0.2, -1.6, -2.2),
        Vec3::new(1.6, 0.9, -2.2),
        BLUE,
    );
    r
}

#[test]
fn golden_ascii_matches_the_c_oracle() {
    let mut r = fixed_scene();
    r.resolve(Mode::Ascii);
    assert_eq!(
        hash_cells(r.cells()),
        0x73e3_5a51_a4b6_0d27,
        "le CellBuffer ASCII diverge de l'oracle C (test3Dterminal/ gelé)"
    );
}

#[test]
fn golden_braille_matches_the_c_oracle() {
    let mut r = fixed_scene();
    r.resolve(Mode::Braille);
    assert_eq!(
        hash_cells(r.cells()),
        0x62e7_7785_a161_ae81,
        "le CellBuffer Braille diverge de l'oracle C (test3Dterminal/ gelé)"
    );
}

#[test]
fn golden_hud_matches_the_c_oracle() {
    // Parité `tests/render.c:hud` : mêmes appels, mêmes offsets, même ordre.
    let mut r = fixed_scene();
    r.resolve(Mode::Ascii);
    {
        let cells = r.cells_mut();
        hud::fill(cells, -4, 19, 9, 7, BLUE);
        hud::text(cells, Anchor::BottomRight, 1, 1, "HUD", BLUE, RED);
        hud::text(cells, Anchor::TopLeft, -2, 0, "ABCDE", RED, BLUE);
    }
    assert_eq!(
        hash_cells(r.cells()),
        0x616c_35d2_dd6e_6620,
        "le CellBuffer HUD diverge de l'oracle C (test3Dterminal/ gelé)"
    );
}

#[test]
fn golden_small_grid_hud_matches_the_c_oracle() {
    // Parité exacte de `tests/render.c:hud(23, 9)` : triangle plein écran, ASCII,
    // puis `fill(-4, 8, 9, 7)` et les deux textes ancrés.
    let camera = Camera::new(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, -1.0),
        1.0,
        0.5,
        0.5,
    );
    let mut r = match Renderer::new(23, 9) {
        Ok(r) => r,
        Err(e) => panic!("Renderer::new(23, 9) : {e}"),
    };
    r.begin(camera, CellRect::full(23, 9), None::<&mut BlackSky>);
    r.triangle(
        Vec3::new(-2.0, -2.0, -1.0),
        Vec3::new(2.0, -2.0, -1.0),
        Vec3::new(0.0, 2.0, -1.0),
        RED,
    );
    r.resolve(Mode::Ascii);
    {
        let cells = r.cells_mut();
        hud::fill(cells, -4, 8, 9, 7, BLUE);
        hud::text(cells, Anchor::BottomRight, 1, 1, "HUD", BLUE, RED);
        hud::text(cells, Anchor::TopLeft, -2, 0, "ABCDE", RED, BLUE);
    }
    assert_eq!(
        hash_cells(r.cells()),
        0x46f5_288a_be06_36c6,
        "le HUD 23x9 diverge de l'oracle C"
    );
    // Les assertions de `tests/render.c:hud` deviennent des tests nommés.
    for (i, expect) in b"HUD".iter().enumerate() {
        let cell = r
            .cells()
            .get(19 + i as i32, 7)
            .copied()
            .unwrap_or(astre_core::Cell::blank());
        assert_eq!(cell.glyph, u32::from(*expect));
        assert_eq!(cell.fg, BLUE);
        assert_eq!(cell.bg, RED);
    }
    assert_eq!(
        r.cells()
            .get(4, 8)
            .copied()
            .unwrap_or(astre_core::Cell::blank())
            .bg,
        BLUE
    );
    assert_ne!(
        r.cells()
            .get(5, 8)
            .copied()
            .unwrap_or(astre_core::Cell::blank())
            .bg,
        BLUE
    );
    assert_eq!(
        r.cells()
            .get(0, 0)
            .copied()
            .unwrap_or(astre_core::Cell::blank())
            .glyph,
        u32::from(b'C')
    );
}

#[test]
fn golden_small_grid_braille_matches_the_c_oracle() {
    let camera = Camera::new(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, -1.0),
        1.0,
        0.5,
        0.5,
    );
    let mut r = match Renderer::new(23, 9) {
        Ok(r) => r,
        Err(e) => panic!("Renderer::new(23, 9) : {e}"),
    };
    r.begin(camera, CellRect::full(23, 9), None::<&mut BlackSky>);
    r.triangle(
        Vec3::new(-1.17, -0.77, -3.0),
        Vec3::new(1.28, -0.61, -3.0),
        Vec3::new(-0.19, 1.07, -3.0),
        RED,
    );
    r.resolve(Mode::Braille);
    assert_eq!(
        hash_cells(r.cells()),
        0x971b_37ae_1a98_19d4,
        "le Braille 23x9 diverge de l'oracle C"
    );
    assert!(r.cells().cells().iter().any(|c| c.glyph > 127));
}

#[test]
fn golden_constants_are_the_c_colours() {
    // Garde-fou : si quelqu'un « corrige » les littéraux, le golden doit le dire
    // avant de devenir un mystère.
    assert_eq!(rgb(250, 34, 20), RED);
    assert_eq!(rgb(10, 40, 220), BLUE);
}
