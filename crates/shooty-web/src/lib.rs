//! # shooty-web — shim WASM du jeu
//!
//! L'ABI `#[wasm_bindgen]` exposée à la coquille TypeScript (`web/`), reproduite à
//! l'identique de celle de HET (`het-demo`) pour que le code web n'ait rien à
//! réapprendre : `App`, `new`, `set_viewport_px`, `set_cell_px`, `frame`, `push_key`,
//! `stats_json`, `atlas_slot_count`, `cell_px`, `cells_ptr`, `cells_len`, `info_ptr`,
//! `info_words`, `last_error`.
//!
//! Chaque méthode délègue en une ligne à [`het_web::Engine`] paramétré par
//! [`ShootyScene`] ; **aucune logique ici**. Le moteur (rasterisation, zoom, packing des
//! instances, HUD) vient de HET ; ce crate ne choisit que la scène.
//!
//! Compilation :
//!
//! ```sh
//! wasm-pack build --release --target web crates/shooty-web --out-dir ../../web/pkg
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use het_web::Engine;
use wasm_bindgen::prelude::*;

use shooty_game::ShootyScene;

/// Application web complète : moteur HET + scène du jeu.
///
/// La coquille TypeScript ne voit que cette façade (`new App(json)`, puis `frame`).
#[wasm_bindgen]
pub struct App {
    inner: Engine<ShootyScene>,
}

#[wasm_bindgen]
impl App {
    /// Construit l'application depuis une configuration JSON plate (`{}` = défauts).
    ///
    /// # Errors
    ///
    /// `JsError` si `config_json` n'est pas un objet JSON, ou si la grille initiale est
    /// refusée par `CellBuffer` (bug d'appelant : les options par défaut sont valides).
    #[wasm_bindgen(constructor)]
    pub fn new(config_json: &str) -> Result<App, JsError> {
        Engine::new(config_json, ShootyScene::default())
            .map(|inner| Self { inner })
            .map_err(|message| JsError::new(&message))
    }

    /// Fixe la taille du canvas en pixels **device** (`css × devicePixelRatio`).
    pub fn set_viewport_px(&mut self, w: u32, h: u32) {
        self.inner.set_viewport_px(w, h);
    }

    /// Choisit le palier de zoom (largeur de cellule en pixels device).
    pub fn set_cell_px(&mut self, px: u32) {
        self.inner.set_cell_px(px);
    }

    /// Produit une image. Retourne les bits de `frame_bits` (`1` = RENDERED).
    pub fn frame(&mut self, now_ms: f64) -> u32 {
        self.inner.frame(now_ms)
    }

    /// Pousse une touche (clavier DOM → file Rust), hors boucle rAF.
    pub fn push_key(&mut self, code: u32, down: bool) {
        self.inner.push_key(code, down);
    }

    /// Compteurs et géométrie pour le HUD DOM, en JSON (appelé à ~4 Hz, hors rAF).
    #[must_use]
    pub fn stats_json(&self) -> String {
        self.inner.stats_json()
    }

    /// Nombre de slots de l'atlas (512).
    #[must_use]
    pub fn atlas_slot_count(&self) -> u32 {
        self.inner.atlas_slot_count()
    }

    /// Largeur de cellule courante, en pixels device (`0` sans viewport).
    #[must_use]
    pub fn cell_px(&self) -> u32 {
        self.inner.cell_px()
    }

    /// Adresse du buffer d'instances dans la mémoire wasm (vue `Uint32Array`).
    #[must_use]
    pub fn cells_ptr(&self) -> *const u8 {
        self.inner.cells_ptr()
    }

    /// Longueur du buffer d'instances, en **octets** (`cols × rows × 8`).
    #[must_use]
    pub fn cells_len(&self) -> u32 {
        self.inner.cells_len()
    }

    /// Adresse du bloc `PresentInfo` (géométrie + pointeur, une lecture par frame).
    #[must_use]
    pub fn info_ptr(&self) -> *const u8 {
        self.inner.info_ptr()
    }

    /// Nombre de mots de 32 bits du bloc `PresentInfo`.
    #[must_use]
    pub fn info_words(&self) -> u32 {
        self.inner.info_words()
    }

    /// Dernier message d'erreur (`""` si aucune).
    #[must_use]
    pub fn last_error(&self) -> String {
        self.inner.last_error()
    }
}
