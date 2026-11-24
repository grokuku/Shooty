//! # astre-scene
//!
//! Scène de la démonstration Astre — port de `test3Dterminal/demo_scene.c` et de la
//! caméra orbitale de `test3Dterminal/demo.c` (C-REF, gelé le 2026-11-24), conformément
//! à `docs/ARCHITECTURE.md` §B.2 et §G.1/G.2.
//!
//! ## Contenu
//!
//! | Module | Origine C | Rôle |
//! | --- | --- | --- |
//! | [`camera`] | `demo.c:{camera_for,reset_camera,limit}` | caméra orbitale et constantes |
//! | [`sky`] | `demo_scene.c:{sky,hash2}` | dégradé, planète à anneaux, étoiles |
//! | [`lighting`] | `demo_scene.c:{normal,paint}` | éclairage diffus et brume |
//! | [`primitives`] | `demo_scene.c:{tri,quad,box,drum,annulus,loft,section,radial}` | primitives et [`primitives::Builder`] |
//! | [`world`] | `demo_scene.c:{terrain,pad,loft,ship,beacon,draw_port}` | décor, navette, [`world::Scene::compose`] |
//!
//! ## Déterminisme
//!
//! La scène n'appelle **aucune** horloge : tout dépend de `(caméra, seconds)`. Deux
//! compositions avec les mêmes entrées produisent des cellules identiques, ce qui rend
//! `astre --snapshot --time T` reproductible et testable (§H.1 `scene_determinism`).
//!
//! ## Parité
//!
//! Les pièges de portage de §G.3 sont traités là où ils apparaissent et signalés en
//! commentaire : division `f64` dans `normal`, troncature `f32 -> i32` de `t3_rgb`,
//! `hash2` en arithmétique wrappante, `2*PI` évalué en `f32` et `mask`/`col & 254`
//! du ciel. Aucune constante n'a été « améliorée ».

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod camera;
pub mod lighting;
pub mod primitives;
pub mod sky;
pub mod world;

pub use camera::{
    camera_for, limit, DEFAULT_AZIMUTH, DEFAULT_ELEVATION, DEFAULT_RADIUS, ELEVATION_MAX,
    ELEVATION_MIN, FOV_Y, NEAR_Z, ORBIT_SPEED, PI, RADIUS_MAX, RADIUS_MIN, TARGET_Y, TWO_PI,
};
pub use lighting::{normal, paint};
pub use primitives::{radial, Builder, Section};
pub use sky::{hash2, DemoSky};
pub use world::{compose, Scene};
