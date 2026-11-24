//! Éclairage et brume — port de `demo_scene.c:normal` et `demo_scene.c:paint`.
//!
//! Deux pièges de parité sont traités ici, tous deux vérifiés par les goldens de la
//! Phase 1 et rappelés par §G.3 :
//!
//! 1. `normal` écrit `float k = 1 / sqrtf(...)` : le `1` est un `int`, promu en
//!    `double`, donc la division se fait en **f64** avant troncature en `f32`. Ce n'est
//!    pas équivalent à `1.0f / sqrtf(...)` (une ou deux unités de couleur d'écart).
//! 2. `paint` appelle `t3_rgb(base.r*light, ...)` où le paramètre est un `int` : la
//!    conversion `f32 -> i32` **tronque vers zéro** avant la saturation.

use astre_core::math::{mix, rgb, rgb_channels, Vec3};

/// Direction de la lumière (`(0.37, 0.85, 0.37)` du C).
const LIGHT_DIR: Vec3 = Vec3::new(0.37, 0.85, 0.37);
/// Couleur de la brume au loin (`C(194,136,140)` du C).
const HAZE: u32 = 0x00C2_888C;

/// Normale orientée du triangle `(a, b, c)` — `normal` du C.
///
/// Le `k` reproduit la division **f64** de l'original (voir la note de module).
#[inline]
#[must_use]
pub fn normal(a: Vec3, b: Vec3) -> Vec3 {
    let n = a.cross(b);
    let len = n.dot(n).max(0.000_001).sqrt();
    let k = (1.0f64 / f64::from(len)) as f32;
    Vec3::new(n.x * k, n.y * k, n.z * k)
}

/// Couleur finale d'une face — `paint` du C.
///
/// `eye` est l'œil de la caméra, `offset` le décalage du modèle (navette, hangar,
/// balise), `base` la couleur d'albedo et `emission` le terme auto-éclairé (0 ou 1).
#[must_use]
pub fn paint(eye: Vec3, offset: Vec3, a: Vec3, b: Vec3, c: Vec3, base: u32, emission: f32) -> u32 {
    let n = normal(b - a, c - a);
    let to_eye = eye - (a + offset);
    let facing = if n.dot(to_eye) < 0.0 { -1.0 } else { 1.0 };
    let diffuse = (facing * n.dot(LIGHT_DIR)).max(0.0);
    let mut light = 0.58 + 0.48 * diffuse;
    light += emission * (1.0 - light);

    let (r, g, b) = rgb_channels(base);
    let shaded = rgb(
        (r as f32 * light) as i32,
        (g as f32 * light) as i32,
        (b as f32 * light) as i32,
    );

    let haze = (((to_eye.dot(to_eye)).sqrt() - 33.0) / 105.0).clamp(0.0, 0.65);
    mix(shaded, HAZE, haze)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_is_unit_length_and_zero_safe() {
        let n = normal(Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 2.0, 0.0));
        assert!((n.length() - 1.0).abs() < 1e-5);
        let z = normal(Vec3::ZERO, Vec3::ZERO);
        assert_eq!(z, Vec3::ZERO);
    }

    #[test]
    fn paint_shades_and_blends_haze_with_distance() {
        let a = Vec3::new(-1.0, -1.0, 0.0);
        let b = Vec3::new(1.0, -1.0, 0.0);
        let c = Vec3::new(0.0, 1.0, 0.0);
        let near = paint(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            a,
            b,
            c,
            0x00FF_FFFF,
            0.0,
        );
        let far = paint(
            Vec3::new(0.0, 0.0, 400.0),
            Vec3::ZERO,
            a,
            b,
            c,
            0x00FF_FFFF,
            0.0,
        );
        // À 400 unités la brume est saturée (`haze = 0.65`) : la couleur est plus
        // proche de `HAZE` qu'elle ne l'est à 5 unités, sur les trois canaux.
        let (hr, hg, hb) = rgb_channels(HAZE);
        let (nr, ng, nb) = rgb_channels(near);
        let (fr, fg, fb) = rgb_channels(far);
        let dist = |r: u8, g: u8, b: u8| {
            (i32::from(r) - i32::from(hr)).abs()
                + (i32::from(g) - i32::from(hg)).abs()
                + (i32::from(b) - i32::from(hb)).abs()
        };
        assert!(dist(fr, fg, fb) < dist(nr, ng, nb), "brume non croissante");
        assert!(fr > fg && fr > fb, "teinte brume attendue : {fr},{fg},{fb}");
    }
}
