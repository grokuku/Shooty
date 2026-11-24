//! Caméra orbitale — port de `demo.c:camera_for`, `demo.c:reset_camera` et
//! `demo.c:limit` (C-REF, §G.1).
//!
//! Toutes les constantes sont celles du C, à la décimale près : `azimuth 1.01`,
//! `elevation 0.30`, `radius 14.2`, `fov_y 0.79`, `near_z 0.15`, `radius ∈ [3.4, 32]`,
//! `elevation ∈ [0.03, 1.42]`, pas `0.075` (azimut) / `0.045` (élévation), orbite auto
//! `0.095 rad/s`, recentrage `target.y += 0.118 · radius · cos(elevation)`.
//!
//! `camera_for` est **déterministe** : mêmes entrées, mêmes octets en sortie (c'est ce
//! qui rend `--snapshot --time T` reproductible, §H.2).

use astre_core::math::clamp_sat;
use astre_core::Camera;

/// `PI` du C : le littéral `3.14159265359f` s'arrondit exactement à cette constante
/// `f32`, écrite ici sous sa forme canonique pour rester lisible et lint-clean.
pub const PI: f32 = core::f32::consts::PI;
/// `2 * PI` arrondi en `f32` — c'est cette valeur précise que le C passait à `fmodf`.
pub const TWO_PI: f32 = 2.0 * PI;

/// Azimut initial (`reset_camera`).
pub const DEFAULT_AZIMUTH: f32 = 1.01;
/// Élévation initiale (`reset_camera`).
pub const DEFAULT_ELEVATION: f32 = 0.30;
/// Rayon initial (`reset_camera`).
pub const DEFAULT_RADIUS: f32 = 14.2;
/// Ordonnée du point visé avant le recentrage (`camera_for`).
pub const TARGET_Y: f32 = 1.55;
/// Champ de vision vertical en radians (`camera_for`).
pub const FOV_Y: f32 = 0.79;
/// Plan proche en unités monde (`camera_for`).
pub const NEAR_Z: f32 = 0.15;

/// Borne basse du rayon caméra (`keypress` : `limit(radius, 3.4f, 32)`).
pub const RADIUS_MIN: f32 = 3.4;
/// Borne haute du rayon caméra.
pub const RADIUS_MAX: f32 = 32.0;
/// Borne basse de l'élévation.
pub const ELEVATION_MIN: f32 = 0.03;
/// Borne haute de l'élévation.
pub const ELEVATION_MAX: f32 = 1.42;

/// Pas d'azimut par appui sur ←/→.
pub const AZIMUTH_STEP: f32 = 0.075;
/// Pas d'élévation par appui sur ↑/↓.
pub const ELEVATION_STEP: f32 = 0.045;
/// Pas de dolly par appui sur `+`/`-` (`0.65` dans le C).
pub const DOLLY_STEP: f32 = 0.65;
/// Vitesse de l'orbite automatique, en radians par seconde.
pub const ORBIT_SPEED: f32 = 0.095;

/// `limit` du C : `fminf(high, fmaxf(x, low))`, sémantique NaN comprise.
#[inline]
#[must_use]
pub fn limit(x: f32, low: f32, high: f32) -> f32 {
    clamp_sat(x, low, high)
}

/// `camera_for` du C : caméra orbitale autour de l'aire de vol.
///
/// L'ordre des opérations est celui de l'original (le `cos(elevation)` du recentrage
/// est recalculé **après** le calcul de l'œil, sans effet sur la valeur mais conservé
/// pour que toute évolution future du C se relise ligne à ligne).
#[must_use]
pub fn camera_for(azimuth: f32, elevation: f32, radius: f32, cell_aspect: f32) -> Camera {
    let mut target_y = TARGET_Y;
    let r = radius * elevation.cos();
    let eye = astre_core::math::Vec3::new(
        r * azimuth.sin(),
        target_y + radius * elevation.sin(),
        r * azimuth.cos(),
    );
    // Laisse du dégagement pour l'horizon tout en gardant la navette au centre bas.
    target_y += 0.118 * radius * elevation.cos();
    Camera::new(
        eye,
        astre_core::math::Vec3::new(0.0, target_y, 0.0),
        FOV_Y,
        NEAR_Z,
        cell_aspect,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_camera_matches_the_c_values() {
        let cam = camera_for(DEFAULT_AZIMUTH, DEFAULT_ELEVATION, DEFAULT_RADIUS, 0.5);
        // Valeurs figées depuis le C (oracle `camera_for(1.01f, 0.30f, 14.2f, 0.5f)`).
        assert!(
            (cam.eye.x - 11.487_933).abs() < 1e-4,
            "eye.x = {}",
            cam.eye.x
        );
        assert!(
            (cam.eye.y - 5.746_386_5).abs() < 1e-4,
            "eye.y = {}",
            cam.eye.y
        );
        assert!(
            (cam.eye.z - 7.215_104_6).abs() < 1e-4,
            "eye.z = {}",
            cam.eye.z
        );
        assert!(
            (cam.target.y - 3.150_761_6).abs() < 1e-4,
            "target.y = {}",
            cam.target.y
        );
        assert_eq!(cam.fov_y, FOV_Y);
        assert_eq!(cam.near_z, NEAR_Z);
        assert_eq!(cam.cell_aspect, 0.5);
    }

    #[test]
    fn limit_clamps_and_ignores_nan_like_the_c() {
        assert_eq!(limit(1.0, 3.4, 32.0), 3.4);
        assert_eq!(limit(99.0, 3.4, 32.0), 32.0);
        assert_eq!(limit(f32::NAN, 3.4, 32.0), 3.4);
    }
}
