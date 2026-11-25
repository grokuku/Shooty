//! Pipeline de rendu d'une image — étapes 5 à 9 de §D.4.
//!
//! ```text
//! 6. renderer.begin(camera, full_rect, sky)   [t_begin]   (ciel par échantillon)
//! 7. Scene::compose(renderer, camera, t)      [t_raster]
//! 8. renderer.resolve(orbit.mode)             [t_resolve]
//! 9. overlay(cells_mut, orbit, eye.y)         [t_hud]
//! ```
//!
//! `present` (étape 10) est **le seul** point de sortie : rien ici ne connaît le
//! backend. Les temps sont mesurés par l'horloge injectée ([`Clock`]) et exposés en
//! microsecondes (§D.6). L'injection n'est pas cosmétique : `Instant::now()` panique
//! sur `wasm32-unknown-unknown` (cf. [`crate::clock`]).

use astre_core::{CellRect, Renderer};
use astre_scene::{compose, DemoSky};

use crate::cli::Options;
use crate::clock::Clock;
use crate::metrics::FrameMetrics;
use crate::orbit::Orbit;
use crate::overlay::overlay;
use crate::sim::SceneState;

/// Microsecondes écoulées depuis `t0`, saturées sur `u32`.
#[inline]
fn micros(clock: &dyn Clock, t0: u64) -> u32 {
    u32::try_from(clock.now_us().saturating_sub(t0)).unwrap_or(u32::MAX)
}

/// Temps par phase d'une image, en microsecondes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderTimings {
    /// `Renderer::begin` (ciel compris).
    pub begin_us: u32,
    /// Composition de la scène.
    pub raster_us: u32,
    /// Résolution échantillons -> cellules.
    pub resolve_us: u32,
    /// HUD et panneaux.
    pub hud_us: u32,
}

impl RenderTimings {
    /// Somme des phases mesurées (le « RENDU » du moniteur du C).
    #[must_use]
    pub const fn total_us(&self) -> u32 {
        self.begin_us + self.raster_us + self.resolve_us + self.hud_us
    }
}

/// Rend une image complète dans `renderer` (déjà dimensionné).
///
/// La caméra vient de `state` (état interpolé), l'horloge de scène aussi : `render` ne
/// modifie **jamais** l'état de simulation (§D.1).
///
/// Le mode de résolution (ASCII/Braille) est celui de l'état **runtime**
/// (`orbit.mode`, touché par `M`), et non `options.mode` : les options ne portent que la
/// valeur **initiale** ([`Options::initial_orbit`]), un `--mode ascii` au démarrage ne
/// devant pas figer le rendu.
pub fn render_frame(
    renderer: &mut Renderer,
    options: &Options,
    orbit: &Orbit,
    state: SceneState,
    clock: &dyn Clock,
) -> RenderTimings {
    let camera = state.camera(options.cell_aspect);
    let cols = renderer.cells().cols();
    let rows = renderer.cells().rows();

    let t_begin = clock.now_us();
    renderer.begin(camera, CellRect::full(cols, rows), Some(&mut DemoSky));
    let begin_us = micros(clock, t_begin);

    let t_raster = clock.now_us();
    compose(renderer, camera, state.seconds);
    let raster_us = micros(clock, t_raster);

    let t_resolve = clock.now_us();
    renderer.resolve(orbit.mode);
    let resolve_us = micros(clock, t_resolve);

    // Le HUD est dessiné dans les cellules (§D.6) : identique sur toutes les cibles.
    let t_hud = clock.now_us();
    if orbit.hud || orbit.monitor || orbit.help {
        overlay(renderer.cells_mut(), orbit, camera.eye.y);
    }
    let hud_us = micros(clock, t_hud);

    RenderTimings {
        begin_us,
        raster_us,
        resolve_us,
        hud_us,
    }
}

/// Complète un [`FrameMetrics`] avec les compteurs de taille de grille.
pub fn fill_grid_metrics(renderer: &Renderer, m: &mut FrameMetrics) {
    let cells = renderer.cells();
    m.cells = u32::try_from(cells.cols().max(0) * cells.rows().max(0)).unwrap_or(u32::MAX);
    m.dirty_tiles = cells.dirty_count();
    m.epoch = cells.epoch();
}

#[cfg(test)]
mod tests {
    // Voir astre-scene : exception locale et justifiée pour le code de test.
    #![allow(clippy::expect_used)]

    use super::*;
    use astre_core::{CellBuffer, Mode};

    use crate::clock::NullClock;

    /// Hash FNV-1a 64 des cellules (même alimentation que `golden_c_parity.rs`).
    fn hash_cells(buf: &CellBuffer) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for cell in buf.cells() {
            for word in [cell.glyph, cell.fg, cell.bg, u32::from(cell.flags)] {
                for byte in word.to_le_bytes() {
                    h ^= u64::from(byte);
                    h = h.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
        }
        h
    }

    /// Rend une image déterministe (horloge nulle, HUD masqué) au mode **runtime** donné.
    ///
    /// `options.mode` reste à sa valeur par défaut : seule l'orbite porte le mode, ce qui
    /// est exactement le contrat de `render_frame` (le rendu suit l'état, pas les options).
    fn render_hash(cols: i32, rows: i32, mode: Mode) -> u64 {
        let options = Options {
            cols,
            rows,
            hud: false,
            ..Options::default()
        };
        let mut renderer = Renderer::new(options.cols, options.rows).expect("renderer");
        let mut orbit = options.initial_orbit();
        orbit.mode = mode;
        let state = SceneState::from_orbit(&orbit);
        render_frame(&mut renderer, &options, &orbit, state, &NullClock);
        hash_cells(renderer.cells())
    }

    #[test]
    fn the_rendered_mode_comes_from_the_runtime_state() {
        // Même caméra, même temps, mêmes options : seul `orbit.mode` change. Si le rendu
        // lisait `options.mode` (figé), ces deux hashes seraient égaux.
        let ascii = render_hash(40, 20, Mode::Ascii);
        let braille = render_hash(40, 20, Mode::Braille);
        assert_ne!(
            ascii, braille,
            "ASCII et Braille doivent produire des CellBuffer différents"
        );
    }

    #[test]
    fn the_initial_mode_still_comes_from_the_options() {
        let options = Options {
            cols: 40,
            rows: 20,
            hud: false,
            mode: Mode::Ascii,
            ..Options::default()
        };
        let mut renderer = Renderer::new(options.cols, options.rows).expect("renderer");
        let orbit = options.initial_orbit();
        assert_eq!(orbit.mode, Mode::Ascii);
        render_frame(
            &mut renderer,
            &options,
            &orbit,
            SceneState::from_orbit(&orbit),
            &NullClock,
        );
        assert_eq!(
            hash_cells(renderer.cells()),
            render_hash(40, 20, Mode::Ascii),
            "`--mode ascii` doit rendre la même image qu'un mode runtime ASCII"
        );
    }
}
