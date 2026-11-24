//! Compteurs de performance — §D.5/§D.6.
//!
//! Le C n'avait qu'un « FPS » : les images écrites sur le terminal. Le portage sépare
//! ce que le moteur **produit** (`built_fps`) de ce qui est **présenté**
//! (`presented_fps`), plus la santé du pas fixe (`sim_ticks`, `sim_dropped`). Les
//! moyennes sont lissées sur une fenêtre de `0,5 s`, comme `demo.c:live`.
//!
//! Comme dans le C, la fenêtre se referme **même en pause** (le README de la démo le
//! documente : « les mesures sont actualisées toutes les demi-secondes, y compris quand
//! la scène est en pause ») — la spec §D.6 dit « figés quand la scène est en pause »,
//! ce que le code C ne fait pas ; c'est le code C qui fait foi ici (oracle).
//!
//! La fenêtre est datée en **microsecondes monotones** fournies par l'appelant
//! ([`crate::clock::Clock`]) et non par `std::time::Instant` : cette dernière panique
//! sur `wasm32-unknown-unknown`, ce qui rendrait `astre-web` inutilisable.

use crate::orbit::Orbit;

/// Durée de la fenêtre glissante des moyennes (parité `demo.c:live`).
pub const WINDOW_SECS: f64 = 0.5;

/// Compteurs d'une image, `#[repr(C)]` à plat pour être lisible par le présentateur
/// web via `DataView` (§F.3).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameMetrics {
    /// Fréquence du pas fixe.
    pub tick_hz: u32,
    /// Pas de simulation exécutés pendant cette image.
    pub sim_ticks: u32,
    /// Rattrapages abandonnés pendant cette image.
    pub sim_dropped: u32,
    /// Microsecondes passées dans la simulation.
    pub sim_us: u32,
    /// Microsecondes passées dans `Renderer::begin` (ciel compris).
    pub begin_us: u32,
    /// Microsecondes passées dans la composition de la scène.
    pub raster_us: u32,
    /// Microsecondes passées dans `resolve`.
    pub resolve_us: u32,
    /// Microsecondes passées dans le HUD.
    pub hud_us: u32,
    /// Terminal : encodage ANSI ; web : `bufferSubData`.
    pub upload_us: u32,
    /// Terminal : `write(2)` ; web : `drawArraysInstanced`.
    pub draw_us: u32,
    /// Microsecondes de l'image complète (simulation comprise).
    pub frame_us: u32,
    /// Cellules de la grille.
    pub cells: u32,
    /// Tuiles sales à la fin de l'image.
    pub dirty_tiles: u32,
    /// `epoch` du `CellBuffer` (invalidation des caches de géométrie).
    pub epoch: u32,
    /// Images produites par seconde (fenêtre 0,5 s).
    pub built_fps: f32,
    /// Images réellement présentées par seconde (fenêtre 0,5 s).
    pub presented_fps: f32,
}

/// Collecteur de moyennes glissantes (fenêtre de [`WINDOW_SECS`]).
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    window_start_us: Option<u64>,
    frames: u32,
    presented: u32,
    render_us: u64,
    present_us: u64,
    built_fps: f64,
    presented_fps: f64,
    render_ms: f64,
    present_ms: f64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    /// Collecteur vide.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            window_start_us: None,
            frames: 0,
            presented: 0,
            render_us: 0,
            present_us: 0,
            built_fps: 0.0,
            presented_fps: 0.0,
            render_ms: 0.0,
            present_ms: 0.0,
        }
    }

    /// Enregistre une image et referme la fenêtre si `0,5 s` se sont écoulées.
    ///
    /// `now_us` est une date monotone en microsecondes (cf. [`crate::clock`]) ;
    /// `render_us` couvre begin+raster+resolve+HUD (le « RENDU » du moniteur du C),
    /// `present_us` la présentation (le « TERMINAL » du C). Une image non présentée
    /// (`presented == false`) compte quand même dans `frames`, comme dans le C où
    /// `present_total` recevait un incrément même si rien n'était écrit.
    pub fn observe(
        &mut self,
        now_us: u64,
        render_us: u32,
        present_us: u32,
        presented: bool,
    ) -> FrameMetrics {
        let mut out = FrameMetrics::default();

        let start = *self.window_start_us.get_or_insert(now_us);
        self.frames += 1;
        if presented {
            self.presented += 1;
        }
        self.render_us += u64::from(render_us);
        self.present_us += u64::from(present_us);

        let elapsed = now_us.saturating_sub(start) as f64 / 1_000_000.0;
        if self.frames > 0 && elapsed >= WINDOW_SECS {
            self.built_fps = f64::from(self.frames) / elapsed;
            self.presented_fps = f64::from(self.presented) / elapsed;
            self.render_ms = 1000.0 * self.render_us as f64 / f64::from(self.frames);
            self.present_ms = 1000.0 * self.present_us as f64 / f64::from(self.frames);
            self.frames = 0;
            self.presented = 0;
            self.render_us = 0;
            self.present_us = 0;
            self.window_start_us = Some(now_us);
        }

        out.built_fps = self.built_fps as f32;
        out.presented_fps = self.presented_fps as f32;
        out
    }

    /// Images produites par seconde (dernière fenêtre close).
    #[must_use]
    pub const fn built_fps(&self) -> f64 {
        self.built_fps
    }

    /// Images présentées par seconde (dernière fenêtre close).
    #[must_use]
    pub const fn presented_fps(&self) -> f64 {
        self.presented_fps
    }

    /// Millisecondes de rendu moyennes (dernière fenêtre close).
    #[must_use]
    pub const fn render_ms(&self) -> f64 {
        self.render_ms
    }

    /// Millisecondes de présentation moyennes (dernière fenêtre close).
    #[must_use]
    pub const fn present_ms(&self) -> f64 {
        self.present_ms
    }

    /// Recopie les moyennes dans l'état orbital (le HUD du C lit `Orbit::fps`, etc.).
    pub fn sync_orbit(&self, orbit: &mut Orbit) {
        orbit.fps = self.built_fps;
        orbit.render_ms = self.render_ms;
        orbit.present_ms = self.present_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_closes_after_half_a_second() {
        let mut m = Metrics::new();
        for i in 0..10 {
            m.observe(60_000 * i, 4000, 1000, true);
        }
        // Au 10e relevé la fenêtre a dépassé 0,5 s : les moyennes sont publiées.
        assert!(m.built_fps() > 0.0, "fps publié après la fenêtre");
        assert!(m.render_ms() > 0.0);
        assert!(m.present_ms() > 0.0);
    }

    #[test]
    fn presented_fps_counts_only_presented_frames() {
        let mut m = Metrics::new();
        for i in 0..11 {
            let presented = i % 2 == 0;
            m.observe(50_000 * i, 1000, 500, presented);
        }
        assert!(m.built_fps() >= m.presented_fps());
        assert!(m.presented_fps() > 0.0);
    }

    #[test]
    fn sync_orbit_copies_the_averages() {
        let mut m = Metrics::new();
        for i in 0..11 {
            m.observe(50_000 * i, 1000, 500, true);
        }
        let mut o = Orbit::default();
        m.sync_orbit(&mut o);
        assert_eq!(o.fps, m.built_fps());
        assert_eq!(o.render_ms, m.render_ms());
        assert_eq!(o.present_ms, m.present_ms());
    }
}
