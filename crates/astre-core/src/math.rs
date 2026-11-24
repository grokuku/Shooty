//! Primitives mathématiques — port de `test3Dterminal/term3d.c` (C-REF).
//!
//! Correspondance (§G.1) :
//!
//! | C | Rust |
//! | --- | --- |
//! | `T3Vec`, `t3_vec` | [`Vec3`] |
//! | `dot`, `cross`, `sub`, `unit` | [`Vec3::dot`], [`Vec3::cross`], `Sub`, [`Vec3::unit`] |
//! | `clampf` | [`clamp_sat`] |
//! | `t3_rgb`, `t3_mix` | [`rgb`], [`mix`] |
//!
//! Les couleurs sont des `u32` packés `0x00RRGGBB` (jamais un type distinct, §C.1) :
//! c'est la forme exacte de l'instance GPU de 8 o et celle du [`crate::cell::Cell`].

/// Borne inférieure du `dot(a, a)` dans `unit` (`1e-20f` du C).
pub const EPS_UNIT: f32 = 1e-20;
/// Seuil du repli `right = (1, 0, 0)` (`1e-10f` du C).
pub const EPS_HORIZONTAL: f32 = 1e-10;

/// `fminf(hi, fmaxf(lo, v))` du C, sémantique NaN **et** ordre d'évaluation compris.
///
/// `f32::clamp` est volontairement écarté : il panique si `lo > hi` et propage le
/// NaN de l'entrée, là où `fminf`/`fmaxf` rendent l'autre opérande (§G.1).
#[inline]
#[must_use]
pub fn clamp_sat(v: f32, lo: f32, hi: f32) -> f32 {
    v.max(lo).min(hi)
}

/// Conversion `f32 -> u8` **tronquante et saturante**, nommée (§G.3.2).
///
/// Le C affectait un `float` à un `uint8_t` : troncature vers zéro, résultat indéfini
/// hors `0..=255`. En Rust `as u8` sature et rend `0` pour NaN ; on rend le cas
/// explicite pour que chaque site d'appel soit auditable.
#[inline]
#[must_use]
pub fn trunc_u8_sat(v: f32) -> u8 {
    if v.is_nan() || v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v as u8
    }
}

/// Empaquette trois canaux en `0x00RRGGBB`.
#[inline]
#[must_use]
pub const fn rgb_pack(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// `t3_rgb` : `clampf(r, 0, 255)` puis conversion entière.
#[inline]
#[must_use]
pub fn rgb(r: i32, g: i32, b: i32) -> u32 {
    rgb_pack(
        trunc_u8_sat(clamp_sat(r as f32, 0.0, 255.0)),
        trunc_u8_sat(clamp_sat(g as f32, 0.0, 255.0)),
        trunc_u8_sat(clamp_sat(b as f32, 0.0, 255.0)),
    )
}

/// Décompose `0x00RRGGBB` en `(r, g, b)`.
#[inline]
#[must_use]
pub const fn rgb_channels(c: u32) -> (u8, u8, u8) {
    (
        ((c >> 16) & 0xFF) as u8,
        ((c >> 8) & 0xFF) as u8,
        (c & 0xFF) as u8,
    )
}

/// `t3_mix` : interpolation par canal, `t` borné à `[0, 1]`, conversion tronquante.
#[inline]
#[must_use]
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = clamp_sat(t, 0.0, 1.0);
    let (ar, ag, ab) = rgb_channels(a);
    let (br, bg, bb) = rgb_channels(b);
    let ch = |x: u8, y: u8| trunc_u8_sat(x as f32 + (y as f32 - x as f32) * t);
    rgb_pack(ch(ar, br), ch(ag, bg), ch(ab, bb))
}

/// Vecteur 2D (coordonnées de cellule, de tuile, ou UV de présentateur).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    /// Composante `x`.
    pub x: f32,
    /// Composante `y`.
    pub y: f32,
}

impl Vec2 {
    /// Vecteur nul.
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    #[inline]
    #[must_use]
    /// Construit un vecteur 2D.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[inline]
    #[must_use]
    /// Produit scalaire.
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y
    }

    #[inline]
    #[must_use]
    /// Carré de la norme.
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    #[must_use]
    /// Norme euclidienne.
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[inline]
    #[must_use]
    /// `true` si les deux composantes sont finies.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Vecteur 3D, axe monde `+Y` vers le haut (comme le C).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    /// Composante `x`.
    pub x: f32,
    /// Composante `y`.
    pub y: f32,
    /// Composante `z`.
    pub z: f32,
}

impl Vec3 {
    /// Vecteur nul.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    /// Axe haut du monde (`t3_vec(0, 1, 0)` du C).
    pub const UP: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    #[inline]
    #[must_use]
    /// Construit un vecteur 3D (équivalent de `t3_vec`).
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    #[must_use]
    /// Produit scalaire.
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    /// Produit vectoriel `self × o` (même convention que le `cross` du C).
    #[inline]
    #[must_use]
    pub fn cross(self, o: Self) -> Self {
        Self {
            x: self.y * o.z - self.z * o.y,
            y: self.z * o.x - self.x * o.z,
            z: self.x * o.y - self.y * o.x,
        }
    }

    #[inline]
    #[must_use]
    /// Carré de la norme.
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    #[must_use]
    /// Norme euclidienne.
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// `unit` du C : `s = 1 / sqrt(max(1e-20, dot(a, a)))` puis `a * s`.
    ///
    /// Le `max` (et non un rejet) est contractuel : un vecteur nul donne `(0, 0, 0)`
    /// et **jamais** un NaN, ce qui évite de propager des NaN jusqu'aux couleurs.
    #[inline]
    #[must_use]
    pub fn unit(self) -> Self {
        let s = 1.0f32 / self.dot(self).max(EPS_UNIT).sqrt();
        Self::new(self.x * s, self.y * s, self.z * s)
    }

    #[inline]
    #[must_use]
    /// `true` si les trois composantes sont finies.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    #[inline]
    #[must_use]
    /// Plus petite composante (`fminf` en chaîne).
    pub fn min_component(self) -> f32 {
        self.x.min(self.y).min(self.z)
    }

    #[inline]
    #[must_use]
    /// Plus grande composante (`fmaxf` en chaîne).
    pub fn max_component(self) -> f32 {
        self.x.max(self.y).max(self.z)
    }
}

impl core::ops::Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl core::ops::Add for Vec3 {
    type Output = Self;
    #[inline]
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl core::ops::Mul<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

/// Caméra du moteur — analogue de `T3Camera` (`term3d.h`).
///
/// Placée dans `astre-core` parce que [`crate::raster::Renderer::begin`] en a besoin ;
/// la construction de la caméra orbitale (`camera_for` de `demo.c`) appartient à
/// `astre-app::orbit` / `astre-scene::camera` (Phase 2).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Position de l'œil, en unités monde.
    pub eye: Vec3,
    /// Point visé, en unités monde (`eye != target` pour une base stable).
    pub target: Vec3,
    /// Champ de vision vertical, en radians (`0.79` par défaut dans la démo).
    pub fov_y: f32,
    /// Plan proche en unités monde ; borné à [`Camera::NEAR_MIN`] par `begin`.
    pub near_z: f32,
    /// Largeur / hauteur d'une cellule (`0.5` = aspect strict du C).
    pub cell_aspect: f32,
}

impl Camera {
    /// `fmaxf(0.001f, near_z)` du C.
    pub const NEAR_MIN: f32 = 0.001;
    /// Bornes du `fov_y` appliquées par `begin` (le champ stocké n'est pas modifié).
    pub const FOV_MIN: f32 = 0.05;
    /// Borne haute du `fov_y` appliquée par `begin`.
    pub const FOV_MAX: f32 = 3.05;
    /// Bornes du `cell_aspect` appliquées par `begin`.
    pub const ASPECT_MIN: f32 = 0.1;
    /// Borne haute du `cell_aspect` appliquée par `begin`.
    pub const ASPECT_MAX: f32 = 3.0;

    /// Caméra de référence des tests de parité (`tests/render.c`).
    #[must_use]
    pub const fn new(eye: Vec3, target: Vec3, fov_y: f32, near_z: f32, cell_aspect: f32) -> Self {
        Self {
            eye,
            target,
            fov_y,
            near_z,
            cell_aspect,
        }
    }
}

/// Base orthonormée de la caméra, calculée comme `t3_begin` (deux replis).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraBasis {
    /// Axe de visée normalisé.
    pub forward: Vec3,
    /// Axe droite normalisé (ou repli `(1, 0, 0)`).
    pub right: Vec3,
    /// Axe haut, `right × forward`.
    pub up: Vec3,
}

/// `forward = unit(target - eye)` ; `right = unit(forward × +Y)` avec repli
/// `(1, 0, 0)` si `|forward × +Y|² <= 1e-10` ; `up = right × forward`.
///
/// Conserve l'absence de NaN du C (§G.3.8) : `eye == target` donne
/// `forward = (0, 0, 0)`, donc `horizontal` nul, donc `right = (1, 0, 0)`.
#[must_use]
pub fn camera_basis(eye: Vec3, target: Vec3) -> CameraBasis {
    let forward = (target - eye).unit();
    let horizontal = forward.cross(Vec3::UP);
    let right = if horizontal.dot(horizontal) > EPS_HORIZONTAL {
        horizontal.unit()
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    let up = right.cross(forward);
    CameraBasis { forward, right, up }
}

/// `view` du C : passage monde -> espace vue (axes de la caméra, origine = `eye`).
#[inline]
#[must_use]
pub fn to_view(eye: Vec3, basis: &CameraBasis, p: Vec3) -> Vec3 {
    let d = p - eye;
    Vec3::new(d.dot(basis.right), d.dot(basis.up), d.dot(basis.forward))
}

/// Matrice 4x4 en colonnes majeures (convention GLSL / WebGL2, §F.4).
///
/// **Non utilisée par le chemin de rendu** : le portage du C conserve `fx`/`fy` et la
/// division par `z` (`project`), strictement identique à l'original. `Mat4` est fourni
/// pour la Phase 2 (`astre-app`) et la coquille web, pas pour changer le pipeline.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    /// Colonnes de la matrice (`cols[c][r]`).
    pub cols: [[f32; 4]; 4],
}

impl Mat4 {
    /// Matrice identité.
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    #[inline]
    #[must_use]
    /// Construit une matrice depuis ses quatre colonnes.
    pub const fn from_cols(c0: [f32; 4], c1: [f32; 4], c2: [f32; 4], c3: [f32; 4]) -> Self {
        Self {
            cols: [c0, c1, c2, c3],
        }
    }

    #[inline]
    #[must_use]
    /// Matrice de translation.
    pub const fn from_translation(t: Vec3) -> Self {
        Self::from_cols(
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [t.x, t.y, t.z, 1.0],
        )
    }

    #[inline]
    #[must_use]
    /// Matrice d'échelle.
    pub const fn from_scale(s: Vec3) -> Self {
        Self::from_cols(
            [s.x, 0.0, 0.0, 0.0],
            [0.0, s.y, 0.0, 0.0],
            [0.0, 0.0, s.z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        )
    }

    /// Produit matriciel `self * o` (colonne majeure), sans division perspective.
    #[must_use]
    pub fn multiply(self, o: Self) -> Self {
        let mut out = [[0.0f32; 4]; 4];
        for (ci, col) in out.iter_mut().enumerate().take(4) {
            for ri in 0..4 {
                let mut acc = 0.0f32;
                for k in 0..4 {
                    acc += self.cols[k][ri] * o.cols[ci][k];
                }
                col[ri] = acc;
            }
        }
        Self { cols: out }
    }

    /// Transforme un point (`w = 1`), sans division perspective.
    #[must_use]
    pub fn transform_point(self, p: Vec3) -> Vec3 {
        let c = &self.cols;
        Vec3::new(
            c[0][0] * p.x + c[1][0] * p.y + c[2][0] * p.z + c[3][0],
            c[0][1] * p.x + c[1][1] * p.y + c[2][1] * p.z + c[3][1],
            c[0][2] * p.x + c[1][2] * p.y + c[2][2] * p.z + c[3][2],
        )
    }
}

impl core::ops::Mul for Mat4 {
    type Output = Self;
    #[inline]
    fn mul(self, o: Self) -> Self {
        self.multiply(o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nearly(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn clamp_nan_matches_fminf_fmaxf() {
        // `fmaxf(lo, NaN) == lo` puis `fminf(hi, lo) == lo`.
        assert_eq!(clamp_sat(f32::NAN, 0.05, 3.05), 0.05);
        assert_eq!(clamp_sat(f32::NAN, 0.0, 1.0), 0.0);
        assert_eq!(clamp_sat(1.5, 0.0, 1.0), 1.0);
        assert_eq!(clamp_sat(-2.0, 0.0, 1.0), 0.0);
        assert_eq!(clamp_sat(f32::INFINITY, 0.0, 1.0), 1.0);
        assert_eq!(clamp_sat(f32::NEG_INFINITY, 0.0, 1.0), 0.0);
    }

    #[test]
    fn trunc_conversion_truncates_toward_zero() {
        assert_eq!(trunc_u8_sat(254.999), 254);
        assert_eq!(trunc_u8_sat(255.0), 255);
        assert_eq!(trunc_u8_sat(-3.0), 0);
        assert_eq!(trunc_u8_sat(f32::NAN), 0);
        assert_eq!(trunc_u8_sat(0.0), 0);
    }

    #[test]
    fn mix_is_truncating_like_the_c() {
        let a = rgb_pack(10, 20, 30);
        let b = rgb_pack(11, 21, 31);
        // 10 + 1 * 0.999 = 10.999 -> 10 (troncature, pas arrondi)
        assert_eq!(mix(a, b, 0.999), rgb_pack(10, 20, 30));
        assert_eq!(mix(a, b, 1.0), b);
        // t est borné.
        assert_eq!(mix(a, b, 17.0), b);
        assert_eq!(mix(a, b, -1.0), a);
    }

    #[test]
    fn unit_of_zero_is_zero_and_never_nan() {
        let u = Vec3::ZERO.unit();
        assert_eq!(u, Vec3::ZERO);
        assert!(u.is_finite());
    }

    #[test]
    fn basis_fallbacks_are_nan_free() {
        // eye == target -> forward nul -> right = (1,0,0).
        let b = camera_basis(Vec3::ZERO, Vec3::ZERO);
        assert_eq!(b.right, Vec3::new(1.0, 0.0, 0.0));
        assert!(b.up.is_finite());

        // Visée verticale -> forward x +Y nul -> repli (1,0,0).
        let b = camera_basis(Vec3::ZERO, Vec3::new(0.0, 5.0, 0.0));
        assert_eq!(b.right, Vec3::new(1.0, 0.0, 0.0));

        // Cas nominal : base orthonormée. `forward = (0,0,-1)`, donc
        // `right = unit(forward x +Y) = (1,0,0)` (même signe que le C).
        let b = camera_basis(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
        assert!(nearly(b.forward.z, -1.0, 1e-6));
        assert!(nearly(b.right.x, 1.0, 1e-6));
        assert!(nearly(b.up.y, 1.0, 1e-6));
        assert!(nearly(b.forward.dot(b.right), 0.0, 1e-6));
    }

    #[test]
    fn mat4_identity_and_translation() {
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(Mat4::IDENTITY.transform_point(p), p);
        let m = Mat4::IDENTITY * Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        assert_eq!(m.transform_point(p), Vec3::new(11.0, 2.0, 3.0));
        let s = Mat4::from_scale(Vec3::new(2.0, 2.0, 2.0));
        assert_eq!(s.transform_point(p), Vec3::new(2.0, 4.0, 6.0));
    }
}
