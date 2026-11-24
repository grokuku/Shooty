//! Orchestration d'une application Astre — §D.4 étape par étape.
//!
//! [`App`] possède **tout** l'état mutable : options, état orbital, simulation à pas
//! fixe, [`Renderer`] (donc son `CellBuffer`), compteurs. Aucun état global, aucun
//! `unsafe`, aucune horloge implicite : `frame_dt` et `now` sont des paramètres, ce qui
//! rend la boucle testable sans terminal.
//!
//! La boucle appelante (native `astre`, ou le `frame()` wasm de la phase 3) suit
//! toujours la même séquence :
//!
//! ```text
//! events -> apply_key      (étape 2-3)
//! sim.advance(frame_dt)    (étape 4)
//! render()                 (étapes 5-9)
//! present(presenter)       (étape 10)
//! metrics.observe(...)     (étape 11)
//! ```

use astre_core::{BufferError, CellBuffer, PixelSize, Renderer, ResizePolicy, SurfaceView};

use crate::cli::Options;
use crate::clock::{Clock, NullClock};
use crate::input::{self, KeyCode};
use crate::metrics::{FrameMetrics, Metrics};
use crate::orbit::Orbit;
use crate::presenter::{PresentConfig, PresentOutcome, Presenter};
use crate::render::{render_frame, RenderTimings};
use crate::sim::{SceneState, Sim, Step};

/// Application Astre : état complet d'une session de rendu.
#[derive(Debug)]
pub struct App {
    options: Options,
    orbit: Orbit,
    sim: Sim,
    renderer: Renderer,
    metrics: Metrics,
    frame: FrameMetrics,
    /// Dernier pas de simulation exécuté (compteurs `sim_*` de l'image).
    last_step: Step,
    /// Microsecondes passées dans `Sim::advance` pour l'image courante.
    sim_us: u32,
}

impl App {
    /// Construit l'application aux dimensions et à l'horloge des options.
    ///
    /// # Errors
    ///
    /// [`BufferError`] si `--size` viole les bornes du `CellBuffer` (dimension ou budget
    /// de 1 M cellules). La CLI les borne déjà à `1..=4096` : une erreur ici est un bug
    /// d'appelant, pas un cas utilisateur (§C.5.2).
    pub fn new(options: Options) -> Result<Self, BufferError> {
        let renderer = Renderer::new(options.cols, options.rows)?;
        let orbit = options.initial_orbit();
        let sim = Sim::new(options.tick_hz, SceneState::from_orbit(&orbit));
        Ok(Self {
            options,
            orbit,
            sim,
            renderer,
            metrics: Metrics::new(),
            frame: FrameMetrics {
                tick_hz: options.tick_hz,
                ..FrameMetrics::default()
            },
            last_step: Step::default(),
            sim_us: 0,
        })
    }

    /// Options effectives.
    #[must_use]
    pub const fn options(&self) -> &Options {
        &self.options
    }

    /// État orbital courant (caméra, drapeaux d'affichage, moyennes du HUD).
    #[must_use]
    pub const fn orbit(&self) -> &Orbit {
        &self.orbit
    }

    /// État orbital mutable (la boucle y recopie les moyennes avant de rendre).
    pub fn orbit_mut(&mut self) -> &mut Orbit {
        &mut self.orbit
    }

    /// Simulation à pas fixe.
    #[must_use]
    pub const fn sim(&self) -> &Sim {
        &self.sim
    }

    /// Grille de cellules de la dernière image.
    #[must_use]
    pub fn cells(&self) -> &CellBuffer {
        self.renderer.cells()
    }

    /// Compteurs de la dernière image.
    #[must_use]
    pub const fn frame_metrics(&self) -> &FrameMetrics {
        &self.frame
    }

    /// Collecteur de moyennes.
    #[must_use]
    pub const fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Redimensionne la grille (`Preserve` par défaut : pas de flash noir).
    ///
    /// Retourne `true` si la taille a effectivement changé (l'appelant force alors un
    /// redraw complet du présentateur).
    ///
    /// # Errors
    ///
    /// [`BufferError`] hors bornes ou hors budget.
    pub fn resize(&mut self, cols: i32, rows: i32) -> Result<bool, BufferError> {
        if cols == self.options.cols && rows == self.options.rows {
            return Ok(false);
        }
        self.renderer.resize(cols, rows, ResizePolicy::Preserve)?;
        self.options.cols = cols;
        self.options.rows = rows;
        Ok(true)
    }

    /// Applique une touche. Retourne `false` quand l'application doit s'arrêter.
    ///
    /// Une touche qui change la caméra recale la simulation ([`Sim::resync`]) : le C
    /// appliquait les touches immédiatement, et l'interpolation ne doit pas retarder
    /// d'un tick un geste discret.
    pub fn apply_key(&mut self, key: KeyCode) -> bool {
        let Some(action) = input::action_for(key) else {
            return true;
        };
        let running = input::apply(&mut self.orbit, action);
        self.sim.resync(&self.orbit);
        running
    }

    /// Applique une action déjà décodée (chemin utilisé par l'ABI web).
    pub fn apply_action(&mut self, action: input::Action) -> bool {
        let running = input::apply(&mut self.orbit, action);
        self.sim.resync(&self.orbit);
        running
    }

    /// Avance la simulation de `frame_dt` secondes et mémorise le coût du pas.
    ///
    /// Le coût est mesuré avec l'horloge **injectée** (§D.6) : `Instant::now()`
    /// paniquerait sur `wasm32-unknown-unknown` (cf. [`crate::clock`]).
    pub fn advance(&mut self, frame_dt: f32, clock: &dyn Clock) -> Step {
        let t0 = clock.now_us();
        let step = self.sim.advance(frame_dt, &mut self.orbit);
        self.sim_us = u32::try_from(clock.now_us().saturating_sub(t0)).unwrap_or(u32::MAX);
        self.last_step = step;
        step
    }

    /// État interpolé à rendre.
    #[must_use]
    pub fn state(&self) -> SceneState {
        self.sim.state()
    }

    /// Rend une image complète et met à jour les compteurs de phase.
    ///
    /// `sim_ticks`/`sim_dropped` décrivent **l'image courante** (dernier pas exécuté),
    /// `sim_us` le coût de `Sim::advance` tel que mesuré par [`Self::advance`].
    pub fn render(&mut self, clock: &dyn Clock) -> RenderTimings {
        let state = self.sim.state();
        let timings = render_frame(&mut self.renderer, &self.options, &self.orbit, state, clock);

        let mut m = FrameMetrics {
            tick_hz: self.sim.tick_hz(),
            sim_ticks: self.last_step.steps,
            sim_dropped: u32::from(self.last_step.dropped),
            sim_us: self.sim_us,
            begin_us: timings.begin_us,
            raster_us: timings.raster_us,
            resolve_us: timings.resolve_us,
            hud_us: timings.hud_us,
            built_fps: self.metrics.built_fps() as f32,
            presented_fps: self.metrics.presented_fps() as f32,
            ..FrameMetrics::default()
        };
        crate::render::fill_grid_metrics(&self.renderer, &mut m);
        self.frame = m;
        timings
    }

    /// Présente l'image courante (étape 10, **seul** point de sortie).
    ///
    /// # Errors
    ///
    /// L'erreur du backend, telle quelle.
    pub fn present<P: Presenter>(&mut self, presenter: &mut P) -> Result<PresentOutcome, P::Error> {
        let mut surfaces = [SurfaceView::full_grid(
            self.renderer.cells(),
            (0, 0),
            // Le terminal n'a pas de notion de pixel : une cellule = une unité.
            PixelSize::new(1, 1),
        )];
        presenter.present(self.renderer.cells(), &mut surfaces, &mut self.frame)
    }

    /// Enregistre une image dans les moyennes (§D.5) et recopie le résultat dans l'état
    /// orbital visible par le HUD.
    ///
    /// `now_us` est une date **monotone en microsecondes** (cf. [`crate::clock`]).
    pub fn observe(&mut self, now_us: u64, present_us: u32, presented: bool) {
        let render_us =
            self.frame.begin_us + self.frame.raster_us + self.frame.resolve_us + self.frame.hud_us;
        let totals = self
            .metrics
            .observe(now_us, render_us, present_us, presented);
        self.frame.built_fps = totals.built_fps;
        self.frame.presented_fps = totals.presented_fps;
        self.metrics.sync_orbit(&mut self.orbit);
    }

    /// Configuration de présentation correspondant aux options.
    #[must_use]
    pub fn present_config(&self) -> PresentConfig {
        PresentConfig {
            palette: self.options.palette,
            ..PresentConfig::default()
        }
    }

    /// Rendu **hors boucle** d'une image unique (`--snapshot`) : aucune dépendance à une
    /// horloge, aucune présentation.
    ///
    /// C'est le chemin qui doit rester strictement déterministe (§H.2).
    pub fn render_once(options: &Options) -> Result<Renderer, BufferError> {
        let mut renderer = Renderer::new(options.cols, options.rows)?;
        let orbit = options.initial_orbit();
        let state = SceneState::from_orbit(&orbit);
        // Horloge figée : un snapshot n'a pas de durées de phase, seulement une image.
        render_frame(&mut renderer, options, &orbit, state, &NullClock);
        Ok(renderer)
    }
}

#[cfg(test)]
mod tests {
    // Voir astre-scene : exception locale et justifiée pour le code de test.
    #![allow(clippy::expect_used)]

    use super::*;
    use astre_core::{Mode, Palette};

    use crate::clock::StdClock;

    fn options() -> Options {
        Options {
            cols: 40,
            rows: 20,
            mode: Mode::Braille,
            ..Options::default()
        }
    }

    #[test]
    fn app_renders_a_non_trivial_frame() {
        let mut app = App::new(options()).expect("app");
        app.render(&StdClock::new());
        let glyphs = app
            .cells()
            .cells()
            .iter()
            .filter(|c| c.glyph != u32::from(' '))
            .count();
        assert!(glyphs > 100, "seulement {glyphs} glyphes non vides");
    }

    #[test]
    fn snapshot_path_is_deterministic() {
        let a = App::render_once(&options()).expect("rendu 1");
        let b = App::render_once(&options()).expect("rendu 2");
        assert_eq!(a.cells().cells(), b.cells().cells());
    }

    #[test]
    fn keys_are_applied_immediately() {
        let mut app = App::new(options()).expect("app");
        app.render(&StdClock::new());
        let before = app.orbit().azimuth;
        assert!(app.apply_key(KeyCode::Left));
        assert!(app.orbit().azimuth < before);
        assert_eq!(app.state().azimuth, app.orbit().azimuth);
    }

    #[test]
    fn quit_key_stops_the_loop() {
        let mut app = App::new(options()).expect("app");
        assert!(app.apply_key(KeyCode::Char('x')));
        assert!(!app.apply_key(KeyCode::Char('q')));
    }

    #[test]
    fn resize_preserves_and_reports_change() {
        let mut app = App::new(options()).expect("app");
        assert!(!app.resize(40, 20).expect("no-op"));
        assert!(app.resize(60, 24).expect("resize"));
        assert_eq!(app.cells().cols(), 60);
        assert_eq!(app.cells().rows(), 24);
    }

    #[test]
    fn present_config_carries_the_palette() {
        let mut o = options();
        o.palette = Palette::Ansi256;
        let app = App::new(o).expect("app");
        assert_eq!(app.present_config().palette, Palette::Ansi256);
    }
}
