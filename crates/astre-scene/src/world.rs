//! Décor de la démo — port de `demo_scene.c:{terrain,pad,loft,ship,beacon,draw_port}`
//! (C-REF). La scène est **déterministe en `t`** : elle ne lit aucune horloge, tout
//! vient de `seconds` (bob de la navette, lampes des balises) — c'est ce qui rend
//! `--snapshot --time T` reproductible (§H.2).
//!
//! Ordre de composition identique au C (`draw_port`) :
//! `terrain` -> `pad` -> trois `beacon` -> `hangar` -> `ship`.
//!
//! Le C passait `Model` **par valeur** : chaque fonction recevait sa propre copie du
//! décalage et ne polluait donc jamais l'appelant. [`Builder::child`] reproduit cette
//! sémantique sans copie de pointeur.

use astre_core::math::{mix, rgb_channels, Vec3};
use astre_core::raster::Renderer;
use astre_core::Camera;

use crate::camera::PI;
use crate::primitives::{radial, Builder, Section};
use crate::sky::hash2;

use crate::primitives::{DARK, GLASS, IVORY, ORANGE};

/// Couleurs du décor (`C(…)` du C, toutes opaques).
const GROUND_A: u32 = 0x008F_5655;
/// Couleur claire du sable.
const GROUND_B: u32 = 0x00B9_6D5D;
/// Roche de la crête extérieure.
const RIDGE_FAR: u32 = 0x0088_657F;
/// Roche de la crête intérieure.
const RIDGE_NEAR: u32 = 0x0093_535A;
/// Tablier de l'aire d'atterrissage.
const PAD_SIDE: u32 = 0x002E_3846;
/// Couvercle de l'aire d'atterrissage.
const PAD_LID: u32 = 0x004A_535D;
/// Peinture ambre de la piste.
const PAD_AMBER: u32 = 0x00EF_AD59;
/// Peinture grise de la piste.
const PAD_GREY: u32 = 0x0091_A3A5;
/// Marqueurs de l'anneau extérieur (`C(255,208,126)` du C).
const CHEVRON: u32 = 0x00FF_D07E;
/// Chevrons d'approche avant/arrière (`C(227,188,119)` du C) — couleur **distincte**
/// des marqueurs d'anneau : les confondre ne change que 3 cellules à 23x9, ce qui
/// rend le bug invisible à l'œil et parfaitement visible au diff d'oracle.
const APPROACH: u32 = 0x00E3_BC77;
/// Trait de l'axe central.
const CENTRELINE: u32 = 0x00AB_B4B1;
/// Ombre de contact au sol.
const SHADOW: u32 = 0x0033_3B4A;
/// Coque du hangar.
const HANGAR_WALL: u32 = 0x00B3_9687;
/// Toit du hangar.
const HANGAR_ROOF: u32 = 0x0043_4D5A;
/// Lampe de balise (couleur froide).
const LAMP_COLD: u32 = 0x00E8_7F3A;
/// Lampe de balise (couleur chaude).
const LAMP_WARM: u32 = 0x00FF_E393;
/// Flamme de tuyère.
const NOZZLE_FLAME: u32 = 0x0050_E3DF;

/// Sections de la coque, de la queue vers le nez (`hull` du C).
const HULL: [Section; 8] = [
    Section::new(-4.3, 0.0, 0.0, 0.0),
    Section::new(-4.22, 0.9, 0.64, 0.0),
    Section::new(-3.45, 1.10, 0.81, 0.0),
    Section::new(-0.45, 1.02, 0.79, 0.0),
    Section::new(1.1, 0.84, 0.69, 0.02),
    Section::new(3.38, 0.51, 0.42, 0.02),
    Section::new(4.85, 0.06, 0.075, -0.06),
    Section::new(4.94, 0.0, 0.0, -0.06),
];

/// Sections de la verrière (`cockpit` du C).
const COCKPIT: [Section; 6] = [
    Section::new(-0.4, 0.02, 0.02, 0.71),
    Section::new(-0.12, 0.62, 0.46, 0.66),
    Section::new(0.98, 0.57, 0.57, 0.63),
    Section::new(1.2, 0.54, 0.52, 0.62),
    Section::new(2.45, 0.30, 0.19, 0.49),
    Section::new(2.70, 0.0, 0.0, 0.40),
];

/// Sections des tuyères (`nozzle` du C).
const NOZZLE: [Section; 3] = [
    Section::new(-4.64, 0.36, 0.37, 0.0),
    Section::new(-4.34, 0.47, 0.48, 0.0),
    Section::new(-3.72, 0.49, 0.49, 0.0),
];

/// Sol et deux crêtes de montagnes basses (`terrain` du C).
fn terrain(m: &mut Builder) {
    for z in -6..6 {
        for x in -6..6 {
            let x0 = x as f32 * 12.0;
            let z0 = z as f32 * 12.0;
            let sand = mix(GROUND_A, GROUND_B, (hash2(x, z) % 9) as f32 / 10.0);
            m.quad(
                Vec3::new(x0, -0.1, z0),
                Vec3::new(x0 + 12.0, -0.1, z0),
                Vec3::new(x0 + 12.0, -0.1, z0 + 12.0),
                Vec3::new(x0, -0.1, z0 + 12.0),
                sand,
                0.0,
            );
        }
    }
    for layer in 0..2 {
        for i in 0..80 {
            let r = if layer != 0 { 41.0 } else { 68.0 };
            let a = 2.0 * PI * i as f32 / 80.0;
            let b = 2.0 * PI * (i + 1) as f32 / 80.0;
            let mut y = (hash2(i, layer) % 100) as f32 * 0.065;
            let mut y2 = (hash2((i + 1) % 80, layer) % 100) as f32 * 0.065;
            if layer != 0 {
                y *= 0.63;
                y2 *= 0.63;
            } else {
                y += 3.0;
                y2 += 3.0;
            }
            let p = radial(r, y, a);
            let q = radial(r, y2, b);
            let foot = radial(r - 7.0, -0.12, a);
            let next = radial(r - 7.0, -0.12, b);
            let rock = if layer != 0 { RIDGE_NEAR } else { RIDGE_FAR };
            m.tri(foot, p, q, rock, 0.0);
            m.tri(foot, q, next, rock, 0.0);
        }
    }
}

/// Aire d'atterrissage peinte : tambour, anneaux, chevrons, axe et ombre (`pad` du C).
fn pad(m: &mut Builder) {
    m.drum(12, 7.9, 7.9, 0.0, 0.42, PAD_SIDE, PAD_LID);
    m.annulus(7.54, 7.68, 0.43, 48, PAD_AMBER, 4);
    m.annulus(6.8, 6.85, 0.435, 64, PAD_GREY, 0);
    for i in 0..12 {
        let a = i as f32 * 2.0 * PI / 12.0;
        m.quad(
            radial(7.12, 0.44, a - 0.023),
            radial(7.42, 0.44, a - 0.023),
            radial(7.42, 0.44, a + 0.023),
            radial(7.12, 0.44, a + 0.023),
            CHEVRON,
            1.0,
        );
    }
    // Chevrons d'approche appariés aux bords avant et arrière de l'aire.
    for end in [-1.0f32, 1.0] {
        for mark in 0..2 {
            let z = end * (5.65 + 0.62 * mark as f32);
            for s in [-1.0f32, 1.0] {
                m.quad(
                    Vec3::new(0.0, 0.46, z),
                    Vec3::new(s * 0.72, 0.46, z + end * 0.42),
                    Vec3::new(s * 0.72, 0.46, z + end * 0.56),
                    Vec3::new(0.0, 0.46, z + end * 0.14),
                    APPROACH,
                    1.0,
                );
            }
        }
    }
    // Axe central peint et ombre portée du démonstrateur en vol stationnaire.
    for i in -5..=5 {
        let i = i as f32;
        m.quad(
            Vec3::new(-0.07, 0.445, i),
            Vec3::new(0.07, 0.445, i),
            Vec3::new(0.07, 0.445, i + 0.52),
            Vec3::new(-0.07, 0.445, i + 0.52),
            CENTRELINE,
            1.0,
        );
    }
    for i in 0..24 {
        let a = 2.0 * PI * i as f32 / 24.0;
        let b = 2.0 * PI * (i + 1) as f32 / 24.0;
        m.tri(
            Vec3::new(-0.4, 0.45, -0.25),
            Vec3::new(-0.4 + 2.2 * a.cos(), 0.45, -0.25 + 4.5 * a.sin()),
            Vec3::new(-0.4 + 2.2 * b.cos(), 0.45, -0.25 + 4.5 * b.sin()),
            SHADOW,
            1.0,
        );
    }
}

/// Balise lumineuse (`beacon` du C) : socle, mât, anneau orange, lanterne pulsée.
fn beacon(m: &mut Builder, x: f32, z: f32, h: f32, time: f64) {
    let mut m = m.child(Vec3::new(x, 0.0, z));
    m.drum(8, 1.25, 0.9, 0.0, 0.65, DARK, IVORY);
    m.drum(6, 0.38, 0.26, 0.65, h, DARK, IVORY);
    m.drum(6, 0.32, 0.32, h - 1.4, h - 0.90, ORANGE, IVORY);
    m.drum(8, 0.52, 0.51, h, h + 0.20, IVORY, IVORY);
    // `cosf(time*2.5f + x)` : l'argument est converti en `float` (time est un double).
    let phase = ((time * 2.5) + f64::from(x)) as f32;
    let lamp = mix(LAMP_COLD, LAMP_WARM, (1.0 + phase.cos()) * 0.5);
    m.drum(6, 0.32, 0.18, h + 0.20, h + 0.58, lamp, lamp);
}

/// Navette NX-04 : coque, verrière, ailes, tuyères, dérives (`ship` du C).
fn ship(m: &mut Builder, seconds: f64) {
    let bob = 1.65 + 0.13 * ((seconds as f32) * 1.4).sin();
    let mut m = m.child(Vec3::new(0.0, bob, 0.0));
    m.loft(&HULL, false);
    m.loft(&COCKPIT, true);
    for sign in [-1.0f32, 1.0] {
        let s = sign;
        let root = Vec3::new(s * 0.72, 0.10, 1.75);
        let tip = Vec3::new(s * 4.8, -0.23, -2.67);
        let end = Vec3::new(s * 4.65, -0.22, -3.48);
        let aft = Vec3::new(s * 0.78, -0.01, -3.27);
        m.quad(root, tip, end, aft, IVORY, 0.0);
        m.quad(
            tip,
            end,
            end + Vec3::new(0.0, -0.16, 0.0),
            tip + Vec3::new(0.0, -0.16, 0.0),
            DARK,
            0.0,
        );
        m.quad(
            Vec3::new(s * 2.72, -0.032, -0.39),
            Vec3::new(s * 3.34, -0.085, -1.06),
            Vec3::new(s * 3.34, -0.098, -3.40),
            Vec3::new(s * 2.72, -0.073, -3.36),
            ORANGE,
            0.0,
        );
        m.quad(
            Vec3::new(s * 4.23, -0.175, -2.16),
            tip,
            end,
            Vec3::new(s * 4.23, -0.16, -3.42),
            DARK,
            0.0,
        );
        // Ouïes de service de part et d'autre du fuselage.
        for k in 0..4 {
            let z = -2.90 + k as f32 * 0.39;
            let x = s * (1.074 - k as f32 * 0.01);
            m.quad(
                Vec3::new(x, 0.025, z),
                Vec3::new(x, 0.025, z + 0.19),
                Vec3::new(x - s * 0.063, 0.34, z + 0.13),
                Vec3::new(x - s * 0.063, 0.34, z - 0.06),
                DARK,
                0.0,
            );
        }
        // Dérives jumelles inclinées, capuchons sombres et bord d'attaque pâle.
        let f0 = Vec3::new(s * 0.69, 0.36, -1.23);
        let f1 = Vec3::new(s * 1.47, 2.11, -3.36);
        let f2 = Vec3::new(s * 1.53, 2.10, -4.08);
        let f3 = Vec3::new(s * 0.83, 0.33, -3.81);
        m.quad(f0, f1, f2, f3, ORANGE, 0.0);
        m.quad(
            f1,
            f2,
            Vec3::new(s * 1.50, 1.89, -4.04),
            Vec3::new(s * 1.42, 1.89, -3.18),
            DARK,
            0.0,
        );

        let mut engine = m.child(Vec3::new(s * 0.63, bob - 0.09, 0.0));
        for i in 0..10 {
            let a = 2.0 * PI * i as f32 / 10.0;
            let b = 2.0 * PI * (i + 1) as f32 / 10.0;
            for j in 0..2 {
                engine.quad(
                    NOZZLE[j].at(a),
                    NOZZLE[j].at(b),
                    NOZZLE[j + 1].at(b),
                    NOZZLE[j + 1].at(a),
                    DARK,
                    0.0,
                );
            }
            engine.tri(
                Vec3::new(0.0, 0.0, -4.65),
                NOZZLE[0].at(a),
                NOZZLE[0].at(b),
                NOZZLE_FLAME,
                1.0,
            );
        }
    }
}

/// Hangar et ses trois portes vitrées (`box` du C, décalage `(-8, 0, -22)`).
fn hangar(m: &mut Builder) {
    let mut m = m.child(Vec3::new(-8.0, 0.0, -22.0));
    m.cuboid(
        Vec3::new(-3.0, 0.0, -2.0),
        Vec3::new(3.0, 2.7, 2.0),
        HANGAR_WALL,
    );
    m.cuboid(
        Vec3::new(-3.2, 2.7, -2.2),
        Vec3::new(3.2, 3.0, 2.2),
        HANGAR_ROOF,
    );
    m.cuboid(Vec3::new(-2.4, 0.0, 2.0), Vec3::new(2.4, 2.0, 2.06), DARK);
    for i in 0..3 {
        let i = i as f32;
        m.cuboid(
            Vec3::new(3.01, 1.2, -1.65 + i * 1.13),
            Vec3::new(3.06, 2.0, -0.78 + i * 1.13),
            GLASS,
        );
    }
}

/// Assemblage complet de la scène de démonstration (`draw_port` du C).
///
/// `seconds` est le temps de scène, en secondes : **seule** entrée non issue de la
/// caméra, ce qui garantit le déterminisme à `t` fixé (`scene_determinism`, §H.1).
#[derive(Clone, Copy, Debug)]
pub struct Scene {
    eye: Vec3,
    seconds: f64,
}

impl Scene {
    /// Scène vue depuis `camera` à l'instant `seconds`.
    #[must_use]
    pub const fn new(camera: Camera, seconds: f64) -> Self {
        Self {
            eye: camera.eye,
            seconds,
        }
    }

    /// Œil utilisé pour l'éclairage.
    #[must_use]
    pub const fn eye(&self) -> Vec3 {
        self.eye
    }

    /// Compose la scène dans `renderer` (déjà démarré par `Renderer::begin`).
    pub fn compose(&self, renderer: &mut Renderer) {
        let seconds = self.seconds;
        let mut m = Builder::new(renderer, self.eye);
        terrain(&mut m);
        pad(&mut m);
        beacon(&mut m, -12.0, 5.0, 3.85, seconds);
        beacon(&mut m, 5.0, -13.0, 5.1, seconds);
        beacon(&mut m, -7.0, 10.0, 2.9, seconds);
        hangar(&mut m);
        ship(&mut m, seconds);
    }
}

/// Raccourci : compose la scène complète (`draw_port` du C).
pub fn compose(renderer: &mut Renderer, camera: Camera, seconds: f64) {
    Scene::new(camera, seconds).compose(renderer);
}

/// Couleur d'une face du décor, exposée pour les tests de déterminisme.
#[must_use]
pub fn ground_tint(x: i32, z: i32) -> (u8, u8, u8) {
    rgb_channels(mix(GROUND_A, GROUND_B, (hash2(x, z) % 9) as f32 / 10.0))
}

#[cfg(test)]
mod tests {
    // Les lints du workspace interdisent `expect`/`unwrap` hors tests ; dans un test,
    // paniquer sur un échec est précisément le comportement attendu.
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn scene_composition_is_deterministic() {
        let cam = crate::camera::camera_for(1.01, 0.30, 14.2, 0.5);
        let render = || {
            let mut r = Renderer::new(40, 20).expect("renderer 40x20");
            r.begin(
                cam,
                astre_core::CellRect::full(40, 20),
                Some(&mut crate::sky::DemoSky),
            );
            compose(&mut r, cam, 0.0);
            r.resolve(astre_core::Mode::Braille);
            let mut hash = 0xcbf2_9ce4_8422_2325u64;
            for c in r.cells().cells() {
                for b in [c.glyph, c.fg, c.bg, u32::from(c.flags)] {
                    hash ^= u64::from(b);
                    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
            hash
        };
        assert_eq!(render(), render());
        assert_ne!(render(), 0);
    }
}
