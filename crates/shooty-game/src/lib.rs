//! # shooty-game — la scène du jeu
//!
//! **Placeholder — le contenu du jeu sera défini ici.**
//!
//! Ce crate ne contient que le **contenu** : il implémente [`het_app::Scene`] et ne
//! sait rien de la boucle, du rendu, du HUD chrome, du terminal ni du navigateur. Le
//! moteur vient de [HET](https://github.com/grokuku/HET) (`het-core`, `het-app`) ; ce
//! dépôt ne fournit que le jeu.
//!
//! ## Ce que la scène dessine aujourd'hui
//!
//! Un sol gris (deux triangles) et une caisse qui tourne autour de son axe vertical,
//! pour prouver que la chaîne scène → rasterisation → résolution → HUD fonctionne. Pas
//! de ciel (ciel noir, comme le `sky = NULL` du moteur), pas de HUD propre au jeu : le
//! chrome générique du moteur suffit.
//!
//! ## Déterminisme
//!
//! [`ShootyScene::compose`] ne lit **aucune** horloge : la rotation vient d'un angle
//! interne avancé par [`het_app::Scene::tick`], donc cadencé par la simulation à pas
//! fixe (pas par les images affichées). Deux rendus de la même image à la même heure
//! sont identiques à l'octet — c'est ce qui rend `shooty --snapshot` testable.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use het_app::{Scene as SceneTrait, Tick};
use het_core::{rgb, Camera, Renderer, Vec3};

/// Demi-étendue du sol, en unités monde (le sol va de `-GROUND` à `+GROUND`).
const GROUND: f32 = 14.0;
/// Demi-arête de la caisse.
const BOX_HALF: f32 = 1.0;
/// Ordonnée du centre de la caisse : elle repose sur le sol (`BOX_HALF` au-dessus).
const BOX_CENTER_Y: f32 = BOX_HALF;
/// Vitesse de rotation de la caisse, en radians par seconde.
const SPIN: f32 = 0.9;

/// Couleur du sol (`0x00RRGGBB`), gris bleuté sombre.
const GROUND_RGB: (i32, i32, i32) = (70, 76, 86);
/// Couleur de base de la caisse, bois clair.
const BOX_RGB: (i32, i32, i32) = (178, 142, 96);

/// Direction de la lumière (normalisée à l'emploi) : au-dessus, à droite, devant.
const LIGHT: Vec3 = Vec3::new(0.42, 0.82, 0.39);

/// Les cinq faces visibles de la caisse (la face inférieure repose sur le sol).
///
/// Chaque entrée donne les quatre coins de la face dans l'ordre du contour, puis la
/// normale **locale** (avant rotation autour de `+Y`). Encodage d'un coin :
/// `ix | iy << 1 | iz << 2`, où `0` = côté négatif et `1` = côté positif de l'axe.
const FACES: [([usize; 4], [f32; 3]); 5] = [
    ([1, 3, 7, 5], [1.0, 0.0, 0.0]),  // +X
    ([0, 2, 6, 4], [-1.0, 0.0, 0.0]), // -X
    ([2, 3, 7, 6], [0.0, 1.0, 0.0]),  // +Y (dessus)
    ([4, 5, 7, 6], [0.0, 0.0, 1.0]),  // +Z
    ([0, 1, 3, 2], [0.0, 0.0, -1.0]), // -Z
];

/// La scène du jeu Shooty.
///
/// Passe à [`het_app::App`] (ou à [`het_web::Engine`](https://docs.rs/het-web)) pour
/// obtenir l'application complète : `App::new(options, ShootyScene::default())`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShootyScene {
    /// Rotation de la caisse autour de `+Y`, en radians (`0` au démarrage).
    angle: f32,
}

impl ShootyScene {
    /// Rotation courante de la caisse, en radians.
    #[must_use]
    pub fn angle(&self) -> f32 {
        self.angle
    }
}

impl SceneTrait for ShootyScene {
    fn compose(&mut self, renderer: &mut Renderer, _camera: Camera, _seconds: f64) {
        draw_ground(renderer);
        draw_box(renderer, self.angle);
    }

    fn tick(&mut self, tick: Tick) {
        // Pas de simulation fixe : l'angle avance de `dt`, jamais du temps des images.
        if tick.dt.is_finite() {
            self.angle = (self.angle + tick.dt * SPIN).rem_euclid(het_core::TWO_PI);
        }
    }
}

/// Deux triangles : le sol, vu de dessus, sous la caisse.
fn draw_ground(renderer: &mut Renderer) {
    let g = GROUND;
    let a = Vec3::new(-g, 0.0, -g);
    let b = Vec3::new(g, 0.0, -g);
    let c = Vec3::new(g, 0.0, g);
    let d = Vec3::new(-g, 0.0, g);
    let color = shade(GROUND_RGB, Vec3::UP);
    renderer.triangle(a, b, c, color);
    renderer.triangle(a, c, d, color);
}

/// Cinq quads (dix triangles) : la caisse posée sur le sol, tournée de `angle`.
fn draw_box(renderer: &mut Renderer, angle: f32) {
    let corners = box_corners(angle);
    for (quad, normal) in FACES {
        let n = rotate_y(angle, Vec3::new(normal[0], normal[1], normal[2]));
        let color = shade(BOX_RGB, n);
        let [i0, i1, i2, i3] = quad;
        renderer.triangle(corners[i0], corners[i1], corners[i2], color);
        renderer.triangle(corners[i0], corners[i2], corners[i3], color);
    }
}

/// Les huit coins de la caisse, déjà tournés de `angle` autour de `+Y`.
fn box_corners(angle: f32) -> [Vec3; 8] {
    let mut corners = [Vec3::ZERO; 8];
    for (index, slot) in corners.iter_mut().enumerate() {
        let sx = if index & 1 == 0 { -1.0 } else { 1.0 };
        let sy = if index & 2 == 0 { -1.0 } else { 1.0 };
        let sz = if index & 4 == 0 { -1.0 } else { 1.0 };
        let local = Vec3::new(sx * BOX_HALF, BOX_CENTER_Y + sy * BOX_HALF, sz * BOX_HALF);
        *slot = rotate_y(angle, local);
    }
    corners
}

/// Rotation d'un point autour de l'axe `+Y` (sens trigonométrique vu de dessus).
fn rotate_y(angle: f32, v: Vec3) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    Vec3::new(v.x * cos + v.z * sin, v.y, -v.x * sin + v.z * cos)
}

/// Éclaire `base` selon la normale de la face : facteur diffus `0,35 … 1,0`.
///
/// Les triangles sont double-face côté moteur : `abs` évite qu'une face retournée
/// devienne noire d'un coup.
fn shade(base: (i32, i32, i32), normal: Vec3) -> u32 {
    let diffuse = (normal.unit().dot(LIGHT.unit())).abs().min(1.0);
    let k = 0.35 + 0.65 * diffuse;
    let channel = |value: i32| (value as f32 * k) as i32;
    rgb(channel(base.0), channel(base.1), channel(base.2))
}

#[cfg(test)]
mod tests {
    // Exception locale et justifiée pour le code de test.
    #![allow(clippy::expect_used)]

    use super::*;
    use het_app::Options;

    /// Nombre de cellules portant un glyphe non blanc.
    fn drawn(renderer: &Renderer) -> usize {
        renderer
            .cells()
            .cells()
            .iter()
            .filter(|cell| cell.glyph != u32::from(b' ') && cell.glyph != 0)
            .count()
    }

    #[test]
    fn the_placeholder_scene_fills_part_of_the_grid() {
        let renderer =
            het_app::App::<ShootyScene>::render_once(&Options::default(), ShootyScene::default())
                .expect("grille par défaut");
        let glyphs = drawn(&renderer);
        assert!(glyphs > 100, "scène trop pauvre : {glyphs} cellules");
        assert!(
            glyphs < renderer.cells().cells().len(),
            "le ciel doit rester noir : {glyphs} cellules dessinées"
        );
    }

    #[test]
    fn tick_advances_the_rotation_and_wraps() {
        let mut scene = ShootyScene::default();
        assert_eq!(scene.angle(), 0.0);
        scene.tick(Tick {
            dt: 0.5,
            paused: false,
            index: 0,
        });
        assert!(
            (scene.angle() - 0.5 * SPIN).abs() < 1e-6,
            "{}",
            scene.angle()
        );
        // Un pas énorme revient dans `[0, 2π)` au lieu de dériver.
        scene.tick(Tick {
            dt: 1.0e6,
            paused: false,
            index: 1,
        });
        assert!((0.0..het_core::TWO_PI).contains(&scene.angle()));
    }

    #[test]
    fn a_non_finite_step_is_ignored() {
        let mut scene = ShootyScene::default();
        scene.tick(Tick {
            dt: f32::NAN,
            paused: false,
            index: 0,
        });
        assert_eq!(scene.angle(), 0.0);
    }

    #[test]
    fn compose_is_deterministic() {
        let first =
            het_app::App::<ShootyScene>::render_once(&Options::default(), ShootyScene::default())
                .expect("grille par défaut");
        let second =
            het_app::App::<ShootyScene>::render_once(&Options::default(), ShootyScene::default())
                .expect("grille par défaut");
        let hash = |renderer: &Renderer| {
            renderer
                .cells()
                .cells()
                .iter()
                .fold(0xcbf2_9ce4_8422_2325_u64, |hash, cell| {
                    (hash ^ u64::from(cell.glyph)).wrapping_mul(0x0000_0100_0000_01b3)
                })
        };
        assert_eq!(hash(&first), hash(&second));
    }

    #[test]
    fn the_box_rotates_with_the_angle() {
        let still =
            het_app::App::<ShootyScene>::render_once(&Options::default(), ShootyScene::default())
                .expect("grille par défaut");
        let mut turned = ShootyScene::default();
        turned.tick(Tick {
            dt: 1.0,
            paused: false,
            index: 0,
        });
        let turned = het_app::App::<ShootyScene>::render_once(&Options::default(), turned)
            .expect("grille par défaut");
        let cells_a: Vec<u32> = still.cells().cells().iter().map(|c| c.glyph).collect();
        let cells_b: Vec<u32> = turned.cells().cells().iter().map(|c| c.glyph).collect();
        assert_ne!(cells_a, cells_b, "la rotation doit changer l'image");
    }
}
