//! Simulation à pas fixe et interpolation du rendu — §D.1 (ACTÉ).
//!
//! Le C avançait la simulation d'un `dt` **variable** borné à `0,12 s`
//! (`demo.c:live`), ce qui rendait toute image non reproductible hors
//! `--snapshot --time`. Ici :
//!
//! - `[`Sim::advance`]` consomme un `frame_dt` mesuré, l'accumule, et exécute au plus
//!   [`MAX_CATCHUP`] pas de `1/tick_hz` ; le retard au-delà est **abandonné** (compteur
//!   `dropped`) plutôt que rattrapé sans fin (spirale de la mort) ;
//! - le rendu consomme [`SceneState::lerp`] entre l'état du tick précédent et celui du
//!   dernier tick, pondéré par `alpha` ;
//! - **l'azimut est interpolé en angle court** (`±π`), sinon un tour complet produirait
//!   un balayage inverse pendant une frame.
//!
//! Aucune horloge murale n'est lue ici : `advance` reçoit `frame_dt` en paramètre, ce
//! qui rend la simulation testable (`sim_is_deterministic`).

use astre_core::Camera;
use astre_scene::camera::TWO_PI;

use crate::orbit::Orbit;

/// Nombre maximal de pas de simulation rattrapés par image (§D.1).
pub const MAX_CATCHUP: u32 = 4;
/// `frame_dt` maximal pris en compte, en secondes (anti-spirale, §D.1).
pub const MAX_FRAME_DT: f32 = 0.25;

/// État interpolable de la scène : ce que le rendu consomme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneState {
    /// Azimut, en radians.
    pub azimuth: f32,
    /// Élévation, en radians.
    pub elevation: f32,
    /// Distance de l'œil, en unités monde.
    pub radius: f32,
    /// Horloge de scène, en secondes (bob de la navette, lampes des balises).
    pub seconds: f64,
}

impl Default for SceneState {
    fn default() -> Self {
        let orbit = Orbit::default();
        Self::from_orbit(&orbit)
    }
}

impl SceneState {
    /// Instantané de l'état orbital courant.
    #[must_use]
    pub fn from_orbit(orbit: &Orbit) -> Self {
        Self {
            azimuth: orbit.azimuth,
            elevation: orbit.elevation,
            radius: orbit.radius,
            seconds: orbit.seconds,
        }
    }

    /// Caméra de cet état, à l'aspect de cellule donné.
    #[must_use]
    pub fn camera(&self, cell_aspect: f32) -> Camera {
        astre_scene::camera_for(self.azimuth, self.elevation, self.radius, cell_aspect)
    }

    /// Interpolation linéaire `prev -> curr` pondérée par `alpha`.
    ///
    /// L'azimut passe par un delta ramené dans `[-π, π]` (angle court) ; les trois
    /// autres champs sont linéaires. `alpha` est borné à `[0, 1]`.
    #[must_use]
    pub fn lerp(prev: Self, curr: Self, alpha: f32) -> Self {
        let t = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            azimuth: prev.azimuth + shortest_delta(curr.azimuth - prev.azimuth) * t,
            elevation: prev.elevation + (curr.elevation - prev.elevation) * t,
            radius: prev.radius + (curr.radius - prev.radius) * t,
            seconds: prev.seconds + (curr.seconds - prev.seconds) * f64::from(t),
        }
    }
}

/// Delta angulaire ramené dans `[-π, π]` (interpolation en angle court).
#[must_use]
pub fn shortest_delta(delta: f32) -> f32 {
    let mut d = delta % TWO_PI;
    if d > TWO_PI * 0.5 {
        d -= TWO_PI;
    } else if d < -TWO_PI * 0.5 {
        d += TWO_PI;
    }
    d
}

/// Résultat d'un appel à [`Sim::advance`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Step {
    /// Nombre de pas de simulation exécutés (0 à [`MAX_CATCHUP`]).
    pub steps: u32,
    /// Fraction de pas en attente, dans `[0, 1]`.
    pub alpha: f32,
    /// `true` si du retard a été abandonné (compteur `sim_dropped`).
    pub dropped: bool,
    /// Total de pas exécutés depuis la création.
    pub ticks: u64,
}

/// Simulation à pas fixe (§D.1).
#[derive(Clone, Copy, Debug)]
pub struct Sim {
    tick_hz: u32,
    dt: f32,
    accumulator: f32,
    prev: SceneState,
    curr: SceneState,
    alpha: f32,
    ticks: u64,
    dropped: u64,
}

impl Sim {
    /// Crée une simulation à `tick_hz` pas par seconde, initialisée sur `initial`.
    #[must_use]
    pub fn new(tick_hz: u32, initial: SceneState) -> Self {
        let hz = tick_hz.max(1);
        Self {
            tick_hz: hz,
            dt: 1.0 / hz as f32,
            accumulator: 0.0,
            prev: initial,
            curr: initial,
            alpha: 0.0,
            ticks: 0,
            dropped: 0,
        }
    }

    /// Fréquence de simulation, en hertz.
    #[must_use]
    pub const fn tick_hz(&self) -> u32 {
        self.tick_hz
    }

    /// Durée d'un pas, en secondes.
    #[must_use]
    pub const fn dt(&self) -> f32 {
        self.dt
    }

    /// Pas exécutés depuis la création.
    #[must_use]
    pub const fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Rattrapages abandonnés depuis la création.
    #[must_use]
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Fraction de pas en attente du dernier `advance`.
    #[must_use]
    pub const fn alpha(&self) -> f32 {
        self.alpha
    }

    /// État courant (dernier tick).
    #[must_use]
    pub const fn current(&self) -> SceneState {
        self.curr
    }

    /// État à rendre : interpolation `prev -> curr` avec `alpha`.
    #[must_use]
    pub fn state(&self) -> SceneState {
        SceneState::lerp(self.prev, self.curr, self.alpha)
    }

    /// Recale la simulation sur l'état orbital (après une entrée utilisateur).
    ///
    /// Le C appliquait une touche **immédiatement** ; pour retrouver ce comportement
    /// sans supprimer l'interpolation, l'état courant et l'état précédent sont alignés
    /// sur `orbit` : la prochaine image montre exactement la nouvelle caméra.
    pub fn resync(&mut self, orbit: &Orbit) {
        let state = SceneState::from_orbit(orbit);
        self.prev = state;
        self.curr = state;
        self.alpha = 0.0;
    }

    /// Avance la simulation de `frame_dt` secondes et rend l'état interpolable.
    ///
    /// `orbit` est l'état **vivant** : `advance` le fait avancer (`Orbit::step`) pour
    /// chaque pas exécuté. En pause, `Orbit::step` ne fait rien, mais les pas sont
    /// quand même comptés (la cadence de simulation reste observable).
    pub fn advance(&mut self, frame_dt: f32, orbit: &mut Orbit) -> Step {
        let dt = if frame_dt.is_finite() {
            frame_dt.clamp(0.0, MAX_FRAME_DT)
        } else {
            0.0
        };
        self.accumulator += dt;

        let mut steps = 0u32;
        while self.accumulator >= self.dt && steps < MAX_CATCHUP {
            self.prev = self.curr;
            orbit.step(self.dt);
            self.curr = SceneState::from_orbit(orbit);
            self.accumulator -= self.dt;
            steps += 1;
            self.ticks += 1;
        }

        let mut dropped = false;
        if steps == MAX_CATCHUP && self.accumulator >= self.dt {
            // Mieux vaut une image en retard qu'une spirale de la mort.
            self.dropped += 1;
            self.accumulator = 0.0;
            dropped = true;
        }

        self.alpha = (self.accumulator / self.dt).clamp(0.0, 1.0);
        Step {
            steps,
            alpha: self.alpha,
            dropped,
            ticks: self.ticks,
        }
    }

    /// Délai à attendre avant le prochain pas, en secondes (pour le dodelinement natif).
    ///
    /// Remplace `demo.c:wait_frame`, qui visait une cadence d'**image** : ici on vise le
    /// prochain **tick** (§D.2), la présentation n'étant pas verrouillée.
    #[must_use]
    pub fn time_to_next_tick(&self) -> f32 {
        (self.dt - self.accumulator).clamp(0.0, self.dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_state() -> SceneState {
        let o = Orbit {
            seconds: 0.0,
            ..Orbit::default()
        };
        SceneState::from_orbit(&o)
    }

    #[test]
    fn advance_runs_one_tick_per_period() {
        let mut o = Orbit::default();
        let mut sim = Sim::new(60, SceneState::from_orbit(&o));
        let step = sim.advance(1.0 / 60.0, &mut o);
        assert_eq!(step.steps, 1);
        assert_eq!(sim.ticks(), 1);
        assert!(step.alpha < 1e-6);
    }

    #[test]
    fn catch_up_is_bounded_and_drops_the_excess() {
        let mut o = Orbit::default();
        let mut sim = Sim::new(60, SceneState::from_orbit(&o));
        let step = sim.advance(1.0, &mut o);
        assert_eq!(step.steps, MAX_CATCHUP);
        assert!(step.dropped);
        assert_eq!(sim.dropped(), 1);
        assert!(sim.time_to_next_tick() > 0.0);
    }

    #[test]
    fn frame_dt_is_clamped() {
        let mut o = Orbit::default();
        let mut sim = Sim::new(60, SceneState::from_orbit(&o));
        // 10 s demandées, 0,25 s prises en compte -> 4 pas max (et non 600).
        let step = sim.advance(10.0, &mut o);
        assert_eq!(step.steps, MAX_CATCHUP);
        let nan = sim.advance(f32::NAN, &mut o);
        assert_eq!(nan.steps, 0);
    }

    #[test]
    fn interpolation_uses_the_short_angle() {
        let prev = SceneState {
            azimuth: 6.0,
            ..fixed_state()
        };
        let curr = SceneState {
            azimuth: 0.2,
            ..fixed_state()
        };
        // 6.0 -> 0.2 : il faut passer par 6.28 (et non reculer par 0).
        let mid = SceneState::lerp(prev, curr, 0.5);
        assert!(mid.azimuth > 6.0, "azimut interpolé = {}", mid.azimuth);
        assert!(mid.azimuth < 7.0);
    }

    #[test]
    fn sim_is_deterministic_for_a_given_tick_sequence() {
        let run = |tick_hz: u32, frames: &[f32]| {
            let mut o = Orbit::default();
            let mut sim = Sim::new(tick_hz, SceneState::from_orbit(&o));
            let mut s = 0.0f64;
            for dt in frames {
                let step = sim.advance(*dt, &mut o);
                s += f64::from(step.alpha);
            }
            (sim.ticks(), sim.state(), o.seconds, s)
        };
        let frames = [1.0 / 60.0; 8];
        assert_eq!(run(60, &frames), run(60, &frames));
    }

    #[test]
    fn pause_freezes_the_state_but_ticks_still_count() {
        let mut o = Orbit {
            paused: true,
            ..Orbit::default()
        };
        let mut sim = Sim::new(60, SceneState::from_orbit(&o));
        let before = sim.current();
        sim.advance(1.0 / 30.0, &mut o);
        assert_eq!(sim.ticks(), 2);
        assert_eq!(sim.current(), before);
    }

    #[test]
    fn resync_snaps_the_rendered_state() {
        let mut o = Orbit::default();
        let mut sim = Sim::new(60, SceneState::from_orbit(&o));
        o.azimuth = 2.5;
        sim.resync(&o);
        assert_eq!(sim.state().azimuth, 2.5);
        assert_eq!(sim.alpha(), 0.0);
    }

    #[test]
    fn shortest_delta_stays_in_half_turn() {
        for d in [-10.0f32, -3.0, -1.0, 0.0, 1.0, 3.0, 10.0] {
            let s = shortest_delta(d);
            assert!(s.abs() <= core::f32::consts::PI + 1e-5, "delta {d} -> {s}");
        }
    }
}
