//! Présentateur terminal — implémentation de [`astre_app::Presenter`] au-dessus de
//! l'encodeur ANSI de [`crate::ansi`].
//!
//! Séparation des responsabilités, identique au découpage du C :
//!
//! | Rôle | C | Rust |
//! | --- | --- | --- |
//! | session (brut, écran alterné, taille, touches) | `T3Terminal` | [`crate::session::AlternateScreen`] |
//! | encodage différentiel + écriture | `t3_present` | [`TerminalPresenter`] + [`crate::ansi::AnsiEncoder`] |
//!
//! Le présentateur ne prend **aucun** verrou sur la session et ne connaît ni `termios` ni
//! les signaux : il écrit dans un `Write`, ce qui le rend testable avec un `Vec<u8>`.

use std::io::{self, Write};
use std::time::Instant;

use astre_app::{FrameMetrics, PresentConfig, PresentOutcome, Presenter};
use astre_core::{CellBuffer, Palette, PixelSize, SurfaceView};

use crate::ansi::{snapshot, AnsiEncoder};

/// Présentateur terminal : encodeur différentiel + flux de sortie.
#[derive(Clone, Debug)]
pub struct TerminalPresenter<W: Write> {
    encoder: AnsiEncoder,
    out: W,
    config: PresentConfig,
}

impl<W: Write> TerminalPresenter<W> {
    /// Crée un présentateur qui écrit dans `out` (`stdout` en pratique).
    #[must_use]
    pub fn new(out: W) -> Self {
        Self {
            encoder: AnsiEncoder::new(),
            out,
            config: PresentConfig::default(),
        }
    }

    /// Force un redraw complet à la prochaine présentation (reprise après `Ctrl-Z`,
    /// redimensionnement, changement de palette).
    pub fn invalidate(&mut self) {
        self.encoder.invalidate();
    }

    /// Écrit une image complète terminée par un `\n` par ligne (`--snapshot`).
    ///
    /// # Errors
    ///
    /// Toute erreur d'écriture du flux de sortie.
    pub fn snapshot(&mut self, cells: &CellBuffer, palette: Palette) -> io::Result<u32> {
        snapshot(&mut self.out, cells, palette)
    }

    /// Accès au flux sous-jacent (tests, `flush` final).
    pub fn inner(&self) -> &W {
        &self.out
    }

    /// `true` si la dernière image encodée était complète.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.encoder.is_full()
    }
}

impl<W: Write> Presenter for TerminalPresenter<W> {
    type Error = io::Error;

    fn configure(&mut self, cfg: PresentConfig) -> Result<(), Self::Error> {
        if cfg.palette != self.config.palette {
            // Changement de palette = redraw complet (parité `t3_present`).
            self.encoder.invalidate();
        }
        self.config = cfg;
        Ok(())
    }

    fn available_px(&self) -> Option<PixelSize> {
        // Le terminal n'a pas de notion de pixel device (§C.3, §E.2).
        None
    }

    fn present(
        &mut self,
        cells: &CellBuffer,
        _surfaces: &mut [SurfaceView],
        m: &mut FrameMetrics,
    ) -> Result<PresentOutcome, Self::Error> {
        let palette = self.config.palette;

        let t_upload = Instant::now();
        self.encoder.encode_present(cells, palette);
        m.upload_us = u32::try_from(t_upload.elapsed().as_micros()).unwrap_or(u32::MAX);

        if self.encoder.buffer().is_empty() && !self.encoder.is_full() {
            // Aucune cellule modifiée : rien n'est écrit, `previous` reste valide.
            m.draw_us = 0;
            return Ok(PresentOutcome::Skipped);
        }

        let bytes = u32::try_from(self.encoder.buffer().len()).unwrap_or(u32::MAX);
        let t_draw = Instant::now();
        let written = self
            .out
            .write_all(self.encoder.buffer())
            .and_then(|()| self.out.flush());
        m.draw_us = u32::try_from(t_draw.elapsed().as_micros()).unwrap_or(u32::MAX);

        match written {
            // Échec : `previous` n'est pas mis à jour, la frame suivante réémet tout
            // (§G.8.6).
            Err(e) => Err(e),
            Ok(()) => {
                self.encoder.commit(cells, palette);
                Ok(PresentOutcome::Presented { bytes })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Voir astre-scene : exception locale et justifiée pour le code de test.
    #![allow(clippy::expect_used)]

    use super::*;
    use astre_core::math::rgb_pack;
    use astre_core::Cell;

    fn fixture() -> CellBuffer {
        let mut b = CellBuffer::new(2, 2).expect("grille");
        b.set(
            0,
            0,
            Cell::solid(u32::from('A'), rgb_pack(1, 2, 3), rgb_pack(4, 5, 6)),
        );
        b.set(
            1,
            0,
            Cell::solid(u32::from('B'), rgb_pack(1, 2, 3), rgb_pack(4, 5, 6)),
        );
        b
    }

    #[test]
    fn present_then_skip_when_nothing_changes() {
        let cells = fixture();
        let mut p = TerminalPresenter::new(Vec::new());
        let mut m = FrameMetrics::default();
        let first = p.present(&cells, &mut [], &mut m).expect("présentation");
        match first {
            PresentOutcome::Presented { bytes } => {
                assert_eq!(bytes as usize, p.inner().len());
            }
            other => panic!("attendu Presented, obtenu {other:?}"),
        }
        let second = p.present(&cells, &mut [], &mut m).expect("seconde image");
        assert_eq!(second, PresentOutcome::Skipped);
    }

    #[test]
    fn configure_forces_a_full_redraw_on_palette_change() {
        let cells = fixture();
        let mut p = TerminalPresenter::new(Vec::new());
        let mut m = FrameMetrics::default();
        p.present(&cells, &mut [], &mut m).expect("image 1");
        p.configure(PresentConfig {
            palette: Palette::Ansi256,
            ..PresentConfig::default()
        })
        .expect("configuration");
        let mut m2 = FrameMetrics::default();
        assert!(matches!(
            p.present(&cells, &mut [], &mut m2).expect("image 2"),
            PresentOutcome::Presented { .. }
        ));
        assert!(p.is_full());
        assert_eq!(p.available_px(), None);
    }

    #[test]
    fn snapshot_writes_lines() {
        let cells = fixture();
        let mut p = TerminalPresenter::new(Vec::new());
        let n = p.snapshot(&cells, Palette::Plain).expect("snapshot");
        assert_eq!(p.inner(), b"AB\n  \n");
        assert_eq!(n as usize, p.inner().len());
    }
}
