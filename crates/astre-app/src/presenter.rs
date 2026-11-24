//! Contrat de présentation — §C.3.
//!
//! Deux implémentations seulement, jamais trois :
//!
//! | Cible | Implémentation | Mécanisme |
//! | --- | --- | --- |
//! | Terminal | `astre_term::TerminalPresenter` | diff sur `previous`, écriture ANSI |
//! | Web | `web/src/presenter.ts` (TypeScript) | `bufferSubData` + 1 `drawArraysInstanced` |
//!
//! **Deux écarts assumés et documentés** avec la signature de §C.3 :
//!
//! 1. `present` reçoit aussi la [`CellBuffer`]. La spec ne passait que
//!    `&mut [SurfaceView]` (géométrie), ce qui suffit au présentateur web (qui lit les
//!    cellules par pointeur wasm) mais obligerait le présentateur terminal à garder une
//!    copie ou un état global. Passer le buffer garde `astre-term` sans état partagé.
//! 2. `FrameMetrics` est passé en `&mut` : seul le backend peut mesurer le partage
//!    `upload_us` (encodage ANSI) / `draw_us` (`write`).

use astre_core::{CellBuffer, Palette, PixelSize, SurfaceView};

use crate::metrics::FrameMetrics;

/// Configuration d'un présentateur (`PresentConfig` de §C.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentConfig {
    /// Palette de sortie (vrai couleur, 256, glyphes seuls).
    pub palette: Palette,
    /// `devicePixelRatio` (web) ; `1.0` en terminal.
    pub dpr: f32,
    /// Taille de cellule demandée par l'utilisateur (`None` = auto, §E.2).
    pub requested_cell_px: Option<PixelSize>,
    /// Couleur des zones mortes du letterbox (web).
    pub letterbox_bg: u32,
    /// Terminal : écran alterné activé.
    pub alt_screen: bool,
}

impl Default for PresentConfig {
    fn default() -> Self {
        Self {
            palette: Palette::Rgb,
            dpr: 1.0,
            requested_cell_px: None,
            letterbox_bg: 0,
            alt_screen: true,
        }
    }
}

/// Résultat d'une présentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentOutcome {
    /// Image écrite : nombre d'octets émis.
    Presented {
        /// Octets écrits sur la sortie.
        bytes: u32,
    },
    /// Rien à écrire (aucune cellule modifiée).
    Skipped,
    /// Cible perdue (contexte GPU perdu, terminal disparu).
    Lost,
}

/// Sortie d'image.
pub trait Presenter {
    /// Erreur du backend (E/S, contexte GL, …).
    type Error: core::fmt::Debug;

    /// Applique une configuration. Idempotent, ne fait jamais de rendu.
    ///
    /// # Errors
    ///
    /// Toute erreur du backend (par exemple une taille de terminal incohérente).
    fn configure(&mut self, cfg: PresentConfig) -> Result<(), Self::Error>;

    /// Surface allouable en pixels device, `None` si la notion n'existe pas (terminal).
    fn available_px(&self) -> Option<PixelSize>;

    /// Présente l'image décrite par `cells` sur `surfaces`.
    ///
    /// # Errors
    ///
    /// Erreur d'écriture du backend. Dans ce cas `previous` **n'est pas** mis à jour
    /// (§G.8.6) : la frame suivante réémet tout.
    fn present(
        &mut self,
        cells: &CellBuffer,
        surfaces: &mut [SurfaceView],
        m: &mut FrameMetrics,
    ) -> Result<PresentOutcome, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_the_terminal_baseline() {
        let cfg = PresentConfig::default();
        assert_eq!(cfg.palette, Palette::Rgb);
        assert_eq!(cfg.dpr, 1.0);
        assert!(cfg.alt_screen);
        assert!(cfg.requested_cell_px.is_none());
    }
}
