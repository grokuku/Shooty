//! État orbital et contrôle caméra — port de `demo.c:{Orbit,reset_camera,keypress}` et
//! de `demo_scene.c:camera_for`.
//!
//! L'`Orbit` du C est scindé en deux : cet état (azimut, élévation, rayon, horloge,
//! drapeaux d'affichage) et son **instantané interpolable** ([`crate::sim::SceneState`]).
//! La caméra elle-même est construite par [`astre_scene::camera_for`] : une seule
//! formule, partagée avec les golden tests de scène.
//!
//! Écart assumé avec le C, acté par §E.7 : `+`/`-` restent le **dolly** (rayon) sur le
//! natif, comme dans l'original ; le zoom de cellule (§E) n'existe que côté web.

use astre_core::{Camera, Mode};
use astre_scene::camera::{
    limit, DEFAULT_AZIMUTH, DEFAULT_ELEVATION, DEFAULT_RADIUS, ELEVATION_MAX, ELEVATION_MIN,
    ORBIT_SPEED, RADIUS_MAX, RADIUS_MIN, TWO_PI,
};

/// État de la caméra orbitale et de l'affichage (`Orbit` du C).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    /// Azimut courant, en radians.
    pub azimuth: f32,
    /// Élévation courante, en radians.
    pub elevation: f32,
    /// Distance de l'œil au point visé, en unités monde.
    pub radius: f32,
    /// Horloge de scène, en secondes (pilotée par le pas fixe).
    pub seconds: f64,
    /// Orbite automatique (`true` tant que l'utilisateur n'a pas tourné la caméra).
    pub automatic: bool,
    /// Animation figée (`Espace`), la caméra restant contrôlable.
    pub paused: bool,
    /// HUD visible (`Tab`).
    pub hud: bool,
    /// Aide visible (`H`).
    pub help: bool,
    /// Moniteur de performances visible (`P`).
    pub monitor: bool,
    /// Mode de résolution des glyphes (`M`).
    pub mode: Mode,
    /// Images par seconde mesurées, `0.0` tant qu'aucune fenêtre n'est close.
    pub fps: f64,
    /// Millisecondes de rendu (raster + resolve + HUD), moyenne de la fenêtre.
    pub render_ms: f64,
    /// Millisecondes de présentation, moyenne de la fenêtre.
    pub present_ms: f64,
}

impl Default for Orbit {
    fn default() -> Self {
        Self::new(Mode::Braille, true)
    }
}

impl Orbit {
    /// Caméra par défaut (`reset_camera` du C), HUD et mode donnés.
    #[must_use]
    pub fn new(mode: Mode, hud: bool) -> Self {
        Self {
            azimuth: DEFAULT_AZIMUTH,
            elevation: DEFAULT_ELEVATION,
            radius: DEFAULT_RADIUS,
            seconds: 0.0,
            automatic: true,
            paused: false,
            hud,
            help: false,
            monitor: false,
            mode,
            fps: 0.0,
            render_ms: 0.0,
            present_ms: 0.0,
        }
    }

    /// `reset_camera` du C : cadrage initial et retour à l'orbite automatique.
    pub fn reset_camera(&mut self) {
        self.azimuth = DEFAULT_AZIMUTH;
        self.elevation = DEFAULT_ELEVATION;
        self.radius = DEFAULT_RADIUS;
        self.automatic = true;
    }

    /// Caméra correspondant à l'état courant (`camera_for` du C).
    #[must_use]
    pub fn camera(&self, cell_aspect: f32) -> Camera {
        astre_scene::camera_for(self.azimuth, self.elevation, self.radius, cell_aspect)
    }

    /// Avance l'animation d'un pas de simulation (`demo.c:live`, sans le `dt` variable).
    ///
    /// En pause, **rien** ne bouge : ni l'horloge, ni l'azimut (parité C).
    pub fn step(&mut self, dt: f32) {
        if self.paused {
            return;
        }
        self.seconds += f64::from(dt);
        if self.automatic {
            self.azimuth = (self.azimuth + ORBIT_SPEED * dt) % TWO_PI;
        }
    }

    /// Azimut ramené dans `[0, 2π)` (le C ne le faisait qu'en orbite automatique ;
    /// l'affichage du cap le suppose, d'où ce normaliseur utilisé par le HUD).
    #[must_use]
    pub fn bearing_degrees(&self) -> i32 {
        let degrees = (self.azimuth * 180.0 / astre_scene::PI).round() as i32;
        (degrees % 360 + 360) % 360
    }

    /// Applique un pas de rotation à l'azimut (flèches ←/→).
    pub fn orbit(&mut self, delta: f32) {
        self.automatic = false;
        self.azimuth += delta;
    }

    /// Applique un pas d'élévation borné (flèches ↑/↓).
    pub fn elevate(&mut self, delta: f32) {
        self.automatic = false;
        self.elevation = limit(self.elevation + delta, ELEVATION_MIN, ELEVATION_MAX);
    }

    /// Rapproche (`delta < 0`) ou éloigne la caméra, à l'intérieur des bornes du C.
    pub fn dolly(&mut self, delta: f32) {
        self.radius = limit(self.radius + delta, RADIUS_MIN, RADIUS_MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_restores_the_c_framing() {
        let mut o = Orbit::default();
        o.orbit(3.0);
        o.elevate(-0.2);
        o.dolly(5.0);
        assert!(!o.automatic);
        o.reset_camera();
        assert_eq!(o.azimuth, DEFAULT_AZIMUTH);
        assert_eq!(o.elevation, DEFAULT_ELEVATION);
        assert_eq!(o.radius, DEFAULT_RADIUS);
        assert!(o.automatic);
    }

    #[test]
    fn clamps_follow_the_c_limits() {
        let mut o = Orbit::default();
        for _ in 0..40 {
            o.elevate(1.0);
            o.dolly(10.0);
        }
        assert_eq!(o.elevation, ELEVATION_MAX);
        assert_eq!(o.radius, RADIUS_MAX);
        for _ in 0..40 {
            o.elevate(-1.0);
            o.dolly(-10.0);
        }
        assert_eq!(o.elevation, ELEVATION_MIN);
        assert_eq!(o.radius, RADIUS_MIN);
    }

    #[test]
    fn step_freezes_when_paused() {
        let mut o = Orbit::default();
        o.step(0.5);
        let t = o.seconds;
        let a = o.azimuth;
        assert!(t > 0.0);
        assert_ne!(a, DEFAULT_AZIMUTH);
        o.paused = true;
        o.step(0.5);
        assert_eq!(o.seconds, t);
        assert_eq!(o.azimuth, a);
    }

    #[test]
    fn bearing_is_normalised() {
        let o = Orbit {
            azimuth: -1.01,
            ..Orbit::default()
        };
        assert_eq!(o.bearing_degrees(), 302);
        let o = Orbit {
            azimuth: TWO_PI * 2.0,
            ..Orbit::default()
        };
        assert_eq!(o.bearing_degrees(), 0);
    }
}
