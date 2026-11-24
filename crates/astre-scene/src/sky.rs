//! Ciel de la démo — port de `demo_scene.c:sky` et `demo_scene.c:hash2` (C-REF).
//!
//! Le ciel est évalué **par échantillon** (2 x 4 par cellule, §A.1.2) : c'est le poste
//! de coût dominant du moteur (`astre_core::raster::Renderer::begin`). Le portage est
//! purement mécanique, à constantes identiques, pour que les goldens C <-> Rust
//! comparent la même image.
//!
//! Trois couches, dans l'ordre du C :
//! 1. dégradé horizon -> crépuscule -> nuit (`h = max(0, d.y)`) ;
//! 2. planète à anneaux, fixe par rapport au paysage (deux axes orthonormés figés) ;
//! 3. étoiles par `hash2` sur une grille de `85 x 95` cellules d'angle.
//!
//! Dernière opération, **contractuelle** : `col & 254` par canal. Le C l'applique pour
//! que de longues plages de ciel se répètent et se compressent mieux en ANSI ; la
//! retirer changerait chaque pixel de ciel.

use astre_core::math::{mix, rgb_pack, Vec3};
use astre_core::Sky;

/// Axe de la planète : `axis` du C (détermine le disque, les anneaux et l'occultation
/// des étoiles).
const PLANET_AXIS: Vec3 = Vec3::new(-0.9582, 0.0840, -0.2734);
/// Base du disque planétaire (`U` du C).
const PLANET_U: Vec3 = Vec3::new(0.2744, 0.0, -0.9616);
/// Base du disque planétaire (`V` du C).
const PLANET_V: Vec3 = Vec3::new(0.0808, 0.9965, 0.0231);

/// Couleur de l'horizon (`horizon` du C).
const HORIZON: u32 = pack(212, 133, 123);
/// Couleur du crépuscule (`dusk` du C).
const DUSK: u32 = pack(94, 85, 126);
/// Couleur de la nuit (`night` du C).
const NIGHT: u32 = pack(22, 39, 69);
/// Couleur de la planète côté ombre (`C(150,106,137)`).
const PLANET_DARK: u32 = pack(150, 106, 137);
/// Couleur de la planète côté jour (`C(255,218,163)`).
const PLANET_LIGHT: u32 = pack(255, 218, 163);
/// Couleur des anneaux (`C(255,216,168)`).
const RING: u32 = pack(255, 216, 168);
/// Couleur d'une étoile (`C(255,224,198)`).
const STAR: u32 = pack(255, 224, 198);

/// `C(r,g,b)` du C en `const` (les canaux y sont toujours dans `0..=255`).
const fn pack(r: u8, g: u8, b: u8) -> u32 {
    rgb_pack(r, g, b)
}

/// `hash2` du C : hash entier 32 bits, arithmétique **wrappante** obligatoire.
///
/// Le C convertit un `int` éventuellement négatif en `uint32_t` (modulo 2^32) : en
/// Rust, `x as u32` fait exactement la même chose (§G.3.11) — surtout **pas**
/// `rem_euclid`.
#[inline]
#[must_use]
pub fn hash2(x: i32, z: i32) -> u32 {
    let mut n = (x as u32).wrapping_mul(0x1f1f_1f1f) ^ (z as u32).wrapping_mul(0x7f4a_7c15);
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n
}

/// Ciel procédural de la démo (`sky` du C), sans état.
#[derive(Clone, Copy, Debug, Default)]
pub struct DemoSky;

impl Sky for DemoSky {
    /// Couleur du ciel pour un rayon monde normalisé.
    fn sample(&mut self, d: Vec3) -> u32 {
        let h = d.y.max(0.0);
        let mut col = if h < 0.18 {
            mix(HORIZON, DUSK, h / 0.18)
        } else {
            mix(DUSK, NIGHT, (h - 0.18) / 0.29)
        };

        // Planète fixe par rapport au paysage : disque + anneaux + bandes.
        if d.dot(PLANET_AXIS) > 0.95 {
            let u = d.dot(PLANET_U);
            let v = d.dot(PLANET_V);
            let r2 = u * u + v * v;
            let ring_y = v - 0.29 * u;
            let belt = (u * u + ring_y * ring_y * 20.0) / (0.144 * 0.144);
            let mut ring = belt > 0.67 && belt < 1.0;
            if r2 < 0.070 * 0.070 {
                let light =
                    (1.0 - r2 / (0.070 * 0.070)).max(0.0).sqrt() * 0.45 + 0.39 - u * 3.4 + v * 1.1;
                let band = ((v + u * 0.10) * 220.0).sin() * 0.5 + 0.5;
                let band = band * 0.10;
                col = mix(PLANET_DARK, PLANET_LIGHT, light - band);
                if ring_y > 0.0 {
                    ring = false;
                }
            }
            if ring {
                col = mix(col, RING, if belt > 0.87 { 0.62 } else { 0.92 });
            }
        }

        // Étoiles : une par cellule d'angle, si elle tombe près du centre.
        if h > 0.10 && d.dot(PLANET_AXIS) < 0.986 {
            let sx = d.x.atan2(d.z) * 85.0;
            let sy = d.y * 95.0;
            let x = sx.floor();
            let y = sy.floor();
            let seed = hash2(x as i32, y as i32);
            if seed % 17 == 0 && (sx - x - 0.5).abs() + (sy - y - 0.5).abs() < 0.30 {
                col = mix(col, STAR, 0.72);
            }
        }

        // Pas de palette : garde les longs aplats de ciel peu coûteux en ANSI.
        let (r, g, b) = astre_core::math::rgb_channels(col);
        rgb_pack(r & 254, g & 254, b & 254)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash2_matches_the_c_oracle() {
        // Valeurs relevées dans le C (mêmes entrées, mêmes entiers).
        assert_eq!(hash2(0, 0), 0x0000_0000);
        assert_eq!(hash2(1, 0), 0xe372_c6e6);
        assert_eq!(hash2(0, 1), 0xebeb_9567);
        assert_eq!(hash2(-3, 5), 0x2aaa_f5b9);
        assert_eq!(hash2(42, -7), 0x9ceb_a28b);
        assert_eq!(hash2(7, 7), 0x1d7e_4aee);
        // Canal bas toujours pair : c'est le `col & 254` du C.
        let c = DemoSky.sample(Vec3::new(0.0, 0.5, 1.0).unit());
        let (r, g, b) = astre_core::math::rgb_channels(c);
        assert_eq!((r & 1, g & 1, b & 1), (0, 0, 0));
    }

    #[test]
    fn sky_is_deterministic() {
        let mut a = DemoSky;
        let mut b = DemoSky;
        let d = Vec3::new(-0.31, 0.42, 0.85).unit();
        assert_eq!(a.sample(d), b.sample(d));
    }
}
