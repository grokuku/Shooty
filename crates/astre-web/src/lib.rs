//! # astre-web — cible **WASM** d'Astre (Phase 3)
//!
//! `cdylib` + `rlib` pour `wasm-pack --target web`, conformément à
//! `docs/ARCHITECTURE.md` §B.2, §C.3, §F. **Aucun appel DOM, aucun `web-sys`, aucun
//! `js-sys`** : le GPU et le DOM sont pilotés par la coquille TypeScript (`web/`). Le
//! seul import JS est `performance.now()` (horloge, cf. [`PlatformClock`]).
//!
//! ## Réutilisation, pas duplication
//!
//! Cette crate **n'implémente ni raster ni résolution** : elle branche
//! [`astre_app::App`] (Sim à pas fixe, Renderer, `FrameMetrics`, overlay) sur un ABI
//! plat. Elle ajoute exactement trois choses :
//!
//! 1. la **géométrie de zoom** ([`astre_core::zoom::fit`]) : `set_viewport_px` +
//!    `set_cell_px` -> `cols`/`rows`/`cell_px`/`origin_px`, puis `App::resize` ;
//! 2. le **packing d'instances** 8 o par cellule (`fg | slot<<24`, `bg | flags<<24`) dans
//!    un buffer lu en place par le présentateur TS (§F.3, zéro copie) ;
//! 3. le **pont d'erreurs** : tout échec devient un message (`last_error`) ou un
//!    `JsError`, jamais un panic qui traverserait la frontière.
//!
//! ## Erreurs : état **récupérable**, documenté
//!
//! `panic = "abort"` (§F.1) : un panic wasm **trappe** l'instance, il ne se rattrape
//! pas (`catch_unwind` est donc sans objet, et `wasm32-unknown-unknown` ne déroule pas
//! les piles). La stratégie retenue est donc **« ne jamais paniquer »** (§F.8) :
//!
//! - toutes les entrées numériques sont validées et bornées (`now_ms` fini, viewport
//!   `≤ 16384` px, `code` de touche borné, `dpr` déjà appliqué côté TS) ;
//! - aucun `unwrap`/`expect` (lints du workspace), aucune indexation non vérifiée ;
//! - une erreur pose un message dans [`App::last_error`], renvoie le bit
//!   [`frame_bits::ERROR`] et **laisse l'état sain** : aucune mutation partielle, la
//!   frame suivante repart normalement. C'est le choix « récupérable » (l'autre option,
//!   un état d'erreur définitif, rendrait service à personne : la seule erreur
//!   réaliste est un appelant qui passe `NaN`).
//!
//! ## ABI (une lecture par frame)
//!
//! | Export | Rôle |
//! | --- | --- |
//! | `App::new(configJson)` | `{"cellPx":16,"tickHz":60,"mode":"braille","hud":true}` |
//! | `frame(nowMs) -> u32` | **le seul appel par frame** : sim + raster + resolve + HUD + packing |
//! | `set_viewport_px(w, h)` | taille du canvas en pixels **device** (ResizeObserver) |
//! | `set_cell_px(px)` | palier de zoom (persisté par la coquille) |
//! | `push_key(code, down)` | clavier, hors boucle rAF |
//! | `stats_json()` | compteurs pour le HUD DOM (4 Hz, hors rAF) |
//! | `cells_ptr()` / `cells_len()` | buffer d'instances en place (vue `Uint32Array`) |
//! | `info_ptr()` / `info_words()` | [`PresentInfo`] : géométrie + pointeur, une lecture |
//! | `atlas_slot_count()` | 512 (le TS ne code pas la constante en dur) |
//! | `last_error()` | dernier message, vide si aucune erreur |
//!
//! Le buffer d'instances est réutilisé d'une frame à l'autre : aucune allocation par
//! frame à grille constante (§H.5).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use wasm_bindgen::prelude::*;

use astre_app::cli::Options;
use astre_app::{App as CoreApp, Clock, KeyCode};
use astre_core::{fit, Grid, PixelSize, ZOOM_LEVELS};

pub mod abi;
pub mod clock;
pub mod config;
pub mod instances;

pub use abi::{frame_bits, PresentInfo, ATLAS_SLOTS};
pub use clock::PlatformClock;
pub use config::Config;
pub use instances::slot_for;

/// Taille maximale d'une dimension de viewport, en pixels device.
///
/// Même borne que `astre-core` (`MAX_DIM` appliqué aux cellules) appliquée aux pixels :
/// au-delà, l'entrée est un bug d'appelant et la valeur est simplement écrêtée.
pub const MAX_VIEWPORT_PX: u32 = 16384;

/// Code de touche « + » (zoom avant, §E.6).
pub const KEY_PLUS: u32 = b'+' as u32;
/// Code de touche « = » (zoom avant sans Maj, §E.6).
pub const KEY_EQUAL: u32 = b'=' as u32;
/// Code de touche « - » (zoom arrière, §E.6).
pub const KEY_MINUS: u32 = b'-' as u32;
/// Code de touche « 0 » (retour au palier automatique).
pub const KEY_ZERO: u32 = b'0' as u32;

/// Codes de touches spéciales, **identiques à l'énumération C**
/// (`test3Dterminal/terminal.h: T3_KEY_UP = 256 …`).
pub mod key {
    /// Flèche haut (`T3_KEY_UP`).
    pub const UP: u32 = 256;
    /// Flèche bas (`T3_KEY_DOWN`).
    pub const DOWN: u32 = 257;
    /// Flèche droite (`T3_KEY_RIGHT`).
    pub const RIGHT: u32 = 258;
    /// Flèche gauche (`T3_KEY_LEFT`).
    pub const LEFT: u32 = 259;
    /// Tabulation (HUD dans les cellules).
    pub const TAB: u32 = 9;
    /// Échap.
    pub const ESC: u32 = 27;
}

/// Application web complète : cœur Rust + géométrie de zoom + buffer d'instances.
///
/// La coquille TypeScript ne voit que cette façade (`new App(json)`, puis `frame`).
#[wasm_bindgen]
pub struct App {
    /// Cœur réutilisé tel quel (`Sim`, `Renderer`, `FrameMetrics`, overlay).
    core: CoreApp,
    /// Horloge de la cible (`performance.now()` en wasm, `Instant` sur l'hôte).
    clock: PlatformClock,
    /// Surface disponible, en pixels device.
    avail: PixelSize,
    /// Largeur de cellule demandée (`None` = palier automatique).
    requested_cell_px: Option<u32>,
    /// Grille courante (`None` tant qu'aucun viewport valide n'a été fourni).
    grid: Option<Grid>,
    /// Instances GPU (`cols × rows × 2` mots de 32 bits).
    instances: Vec<u32>,
    /// Bloc lu par le présentateur TS.
    info: PresentInfo,
    /// Dernier horodatage rAF, en millisecondes.
    last_now_ms: Option<f64>,
    /// Message de la dernière erreur (vide si aucune).
    last_error: String,
    /// Compteur de frames produites.
    frame_index: u32,
    /// Rendu nécessaire même sans temps écoulé (resize, zoom, touche).
    needs_render: bool,
    /// Une action « quitter » a été demandée (le web ne ferme pas l'onglet).
    quit_requested: bool,
    /// Bits de la dernière frame.
    last_bits: u32,
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
        Self::build(config_json).map_err(|message| JsError::new(&message))
    }

    /// Fixe la taille du canvas en pixels **device** (`css × devicePixelRatio`).
    ///
    /// Idempotent : même taille ⇒ aucun travail, aucun `epoch++`. C'est le seul chemin
    /// qui change `cols`/`rows` avec `set_cell_px` (§F.6).
    pub fn set_viewport_px(&mut self, w: u32, h: u32) {
        let w = w.min(MAX_VIEWPORT_PX);
        let h = h.min(MAX_VIEWPORT_PX);
        let avail = PixelSize::new(w, h);
        if avail == self.avail {
            return;
        }
        self.avail = avail;
        self.refit();
    }

    /// Choisit le palier de zoom (largeur de cellule en pixels device, §E.2).
    ///
    /// La valeur est ramenée au palier le plus proche : aucune valeur n'est refusée.
    pub fn set_cell_px(&mut self, px: u32) {
        let requested = Some(px.min(u32::from(u8::MAX)));
        if requested == self.requested_cell_px {
            return;
        }
        self.requested_cell_px = requested;
        self.refit();
    }

    /// Produit une image. **Le seul appel wasm par frame.**
    ///
    /// `now_ms` est l'horodatage `requestAnimationFrame` (même base pour toutes les
    /// frames). Retourne les bits de [`frame_bits`] : `RENDERED` ⇒ le présentateur
    /// téléverse et dessine, `SKIPPED` ⇒ il ne touche pas à GL.
    pub fn frame(&mut self, now_ms: f64) -> u32 {
        if !now_ms.is_finite() {
            return self.fail("frame() : now_ms n'est pas fini");
        }
        let Some(grid) = self.grid else {
            // Aucun viewport valide : rien à rendre (le TS a oublié `set_viewport_px`).
            self.last_bits = frame_bits::SKIPPED;
            self.sync_info(self.last_bits);
            return self.last_bits;
        };

        let dt = match self.last_now_ms {
            Some(prev) if now_ms > prev => ((now_ms - prev) / 1000.0) as f32,
            _ => 0.0,
        };
        self.last_now_ms = Some(now_ms);

        // Aucun temps écoulé et rien de neuf : ne rien refaire (l'App décide de sauter).
        if dt <= 0.0 && !self.needs_render {
            self.last_bits = frame_bits::SKIPPED | self.persistent_bits();
            self.sync_info(self.last_bits);
            return self.last_bits;
        }
        self.needs_render = false;

        // Étape 4 : simulation à pas fixe (le coût est mesuré par l'horloge injectée).
        let step = self.core.advance(dt, &self.clock);

        // Étapes 5-9 : rendu complet (begin, scène, resolve, HUD).
        self.core.render(&self.clock);

        // Projection Cell -> instance GPU, en place dans `self.instances`.
        let dirty = self.core.cells().dirty_count();
        instances::pack_instances(self.core.cells(), &mut self.instances);

        // Étape 11 : compteurs (le web ré-uploade chaque image, donc « présentée »).
        self.core.observe(self.clock.now_us(), 0, true);
        self.frame_index = self.frame_index.wrapping_add(1);

        let mut bits = frame_bits::RENDERED;
        if dirty > 0 {
            bits |= frame_bits::DIRTY;
        }
        if step.dropped {
            bits |= frame_bits::SIM_DROPPED;
        }
        bits |= self.persistent_bits();
        debug_assert_eq!(
            self.instances.len(),
            instances::words_for(grid.cols.max(0) as u32, grid.rows.max(0) as u32)
        );
        self.last_bits = bits;
        self.sync_info(bits);
        bits
    }

    /// Pousse une touche (clavier DOM → file Rust), **hors** boucle rAF (§F.5).
    ///
    /// `code` suit l'énumération C : ASCII pour les caractères imprimables, [`key`] pour
    /// les touches spéciales. Les touches agissent à l'**enfoncement** (`down == true`) ;
    /// les relâchements sont acceptés (l'API les admet) et sans effet, les actions étant
    /// discrètes.
    ///
    /// Sur le web, `+`/`=`/`-`/`0` changent le **zoom d'affichage** et non la caméra
    /// (§E.7) : ils sont interceptés ici, avant `astre-app::input`.
    pub fn push_key(&mut self, code: u32, down: bool) {
        if !down {
            return;
        }
        match code {
            KEY_PLUS | KEY_EQUAL => {
                self.zoom(1);
                return;
            }
            KEY_MINUS => {
                self.zoom(-1);
                return;
            }
            KEY_ZERO => {
                self.requested_cell_px = None;
                self.refit();
                return;
            }
            _ => {}
        }
        let Some(key) = key_from_code(code) else {
            return;
        };
        if !self.core.apply_key(key) {
            // `q`/`Q` : le C quittait la boucle. Le web ne ferme jamais l'onglet ;
            // on signale la demande, la coquille décide.
            self.quit_requested = true;
        }
        self.needs_render = true;
    }

    /// Compteurs et géométrie pour le HUD DOM, en JSON (appelé à ~4 Hz, hors rAF).
    #[must_use]
    pub fn stats_json(&self) -> String {
        let m = self.core.frame_metrics();
        let (cols, rows, cell_w, cell_h, level, origin) = match self.grid {
            Some(g) => (
                g.cols,
                g.rows,
                g.cell_px.w,
                g.cell_px.h,
                u32::from(g.level),
                g.origin_px,
            ),
            None => (0, 0, 0, 0, 0, (0, 0)),
        };
        format!(
            concat!(
                "{{\"cols\":{},\"rows\":{},\"cellPx\":{},\"cellPxH\":{},\"level\":{},",
                "\"originX\":{},\"originY\":{},\"availW\":{},\"availH\":{},",
                "\"cells\":{},\"dirtyTiles\":{},\"epoch\":{},",
                "\"tickHz\":{},\"simTicks\":{},\"simDropped\":{},",
                "\"builtFps\":{:.1},\"presentedFps\":{:.1},",
                "\"simMs\":{:.2},\"beginMs\":{:.2},\"rasterMs\":{:.2},",
                "\"resolveMs\":{:.2},\"hudMs\":{:.2},\"frameMs\":{:.2},",
                "\"atlasSlots\":{},\"frameIndex\":{},\"flags\":{},",
                "\"requestedCellPx\":{},\"zoomMin\":{},\"zoomMax\":{},\"quit\":{}}}"
            ),
            cols,
            rows,
            cell_w,
            cell_h,
            level,
            origin.0,
            origin.1,
            self.avail.w,
            self.avail.h,
            m.cells,
            m.dirty_tiles,
            m.epoch,
            m.tick_hz,
            m.sim_ticks,
            m.sim_dropped,
            f64::from(m.built_fps),
            f64::from(m.presented_fps),
            f64::from(m.sim_us) / 1000.0,
            f64::from(m.begin_us) / 1000.0,
            f64::from(m.raster_us) / 1000.0,
            f64::from(m.resolve_us) / 1000.0,
            f64::from(m.hud_us) / 1000.0,
            f64::from(m.frame_us) / 1000.0,
            ATLAS_SLOTS,
            self.frame_index,
            self.last_bits,
            match self.requested_cell_px {
                Some(px) => px.to_string(),
                None => "null".to_string(),
            },
            ZOOM_LEVELS[0],
            ZOOM_LEVELS[ZOOM_LEVELS.len() - 1],
            self.quit_requested,
        )
    }

    /// Nombre de slots de l'atlas (512 : `32 × 16`, §E.3).
    #[must_use]
    pub fn atlas_slot_count(&self) -> u32 {
        ATLAS_SLOTS
    }

    /// Largeur de cellule courante, en pixels device (`0` sans viewport) : c'est la
    /// valeur que la coquille persiste (`localStorage["astre.cellPx"]`).
    #[must_use]
    pub fn cell_px(&self) -> u32 {
        self.grid.map_or(0, |g| g.cell_px.w)
    }

    /// Adresse du buffer d'instances dans la mémoire wasm (vue `Uint32Array`, §F.3).
    #[must_use]
    pub fn cells_ptr(&self) -> *const u8 {
        self.instances.as_ptr() as *const u8
    }

    /// Longueur du buffer d'instances, en **octets** (`cols × rows × 8`).
    #[must_use]
    pub fn cells_len(&self) -> u32 {
        u32::try_from(self.instances.len() * core::mem::size_of::<u32>()).unwrap_or(u32::MAX)
    }

    /// Adresse du bloc [`PresentInfo`] (géométrie + pointeur, une lecture par frame).
    #[must_use]
    pub fn info_ptr(&self) -> *const u8 {
        (&self.info as *const PresentInfo) as *const u8
    }

    /// Nombre de mots de 32 bits du bloc [`PresentInfo`] (lu une fois au démarrage).
    #[must_use]
    pub fn info_words(&self) -> u32 {
        PresentInfo::WORDS
    }

    /// Dernier message d'erreur (`""` si aucune).
    #[must_use]
    pub fn last_error(&self) -> String {
        self.last_error.clone()
    }
}

impl App {
    /// Construction **sans** `JsError` : testable sur l'hôte.
    ///
    /// `JsError` ne peut pas être construit hors wasm (`wasm-bindgen` panique), donc
    /// toute la validation vit ici et `App::new` ne fait que traduire le message.
    fn build(config_json: &str) -> Result<App, String> {
        let config = config::parse(config_json)
            .ok_or_else(|| "configuration invalide : objet JSON attendu".to_string())?;

        let mut options = Options::default();
        if let Some(hz) = config.tick_hz {
            options.tick_hz = hz;
        }
        if let Some(mode) = config.mode {
            options.mode = mode;
        }
        if let Some(hud) = config.hud {
            options.hud = hud;
        }

        let core = CoreApp::new(options).map_err(|e| {
            format!(
                "grille initiale {}x{} refusée : {e}",
                options.cols, options.rows
            )
        })?;

        let mut app = Self {
            core,
            clock: clock::platform_clock(),
            avail: PixelSize::new(0, 0),
            requested_cell_px: config.cell_px,
            grid: None,
            instances: Vec::new(),
            info: PresentInfo::default(),
            last_now_ms: None,
            last_error: String::new(),
            frame_index: 0,
            needs_render: true,
            quit_requested: false,
            last_bits: 0,
        };
        app.sync_info(frame_bits::SKIPPED);
        Ok(app)
    }

    /// Applique un cran de zoom (`+1`/`-1`) et refait la géométrie.
    fn zoom(&mut self, delta: i8) {
        let current = self
            .grid
            .map_or_else(astre_core::auto_level, |g| usize::from(g.level));
        let level = astre_core::step_level(current, delta);
        self.requested_cell_px = Some(ZOOM_LEVELS[level]);
        self.refit();
    }

    /// Recalcule la grille (`fit`, §E.4/§E.5) et redimensionne le cœur si besoin.
    ///
    /// Aucune erreur : le budget est clampé, une surface dégénérée laisse `grid = None`
    /// (les frames suivantes répondent `SKIPPED`).
    fn refit(&mut self) {
        self.grid = fit(self.avail, self.requested_cell_px);
        if let Some(grid) = self.grid {
            if let Err(e) = self.core.resize(grid.cols, grid.rows) {
                // Impossible par construction (`fit` borne à 1 M cellules et à
                // `MAX_DIM`) : on le signale sans casser l'état.
                self.last_error = format!("grille {}x{} refusée : {e}", grid.cols, grid.rows);
            }
        }
        self.needs_render = true;
        self.sync_info(self.last_bits);
    }

    /// Recopie géométrie et pointeurs dans le bloc lu par le TS.
    fn sync_info(&mut self, bits: u32) {
        self.info.cells_ptr = self.instances.as_ptr() as u32;
        self.info.cells_len = u32::try_from(self.instances.len() * 4).unwrap_or(u32::MAX);
        self.info.atlas_slots = ATLAS_SLOTS;
        self.info.frame_index = self.frame_index;
        self.info.flags = bits;
        self.info.avail_w = self.avail.w;
        self.info.avail_h = self.avail.h;
        match self.grid {
            Some(g) => {
                self.info.cols = g.cols.max(0) as u32;
                self.info.rows = g.rows.max(0) as u32;
                self.info.cell_w = g.cell_px.w;
                self.info.cell_h = g.cell_px.h;
                self.info.origin_x = g.origin_px.0;
                self.info.origin_y = g.origin_px.1;
                self.info.level = u32::from(g.level);
            }
            None => {
                self.info.cols = 0;
                self.info.rows = 0;
                self.info.cell_w = 0;
                self.info.cell_h = 0;
                self.info.origin_x = 0;
                self.info.origin_y = 0;
                self.info.level = 0;
            }
        }
        self.info.epoch = self.core.cells().epoch();
    }

    /// Bits toujours vrais de la session (arrêt demandé).
    fn persistent_bits(&self) -> u32 {
        if self.quit_requested {
            frame_bits::QUIT
        } else {
            0
        }
    }

    /// Enregistre une erreur **récupérable** et retourne les bits correspondants.
    fn fail(&mut self, message: &str) -> u32 {
        self.last_error = message.to_string();
        let bits = frame_bits::ERROR | self.persistent_bits();
        self.last_bits = bits;
        bits
    }
}

/// Traduit un code de touche numérique en [`KeyCode`] du cœur (table pure, §F.5).
#[must_use]
pub fn key_from_code(code: u32) -> Option<KeyCode> {
    match code {
        key::UP => Some(KeyCode::Up),
        key::DOWN => Some(KeyCode::Down),
        key::RIGHT => Some(KeyCode::Right),
        key::LEFT => Some(KeyCode::Left),
        key::TAB => Some(KeyCode::Tab),
        key::ESC => Some(KeyCode::Esc),
        32..=126 => char::from_u32(code).map(KeyCode::Char),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn app(viewport: (u32, u32)) -> App {
        let mut app = App::build(r#"{"cellPx":16}"#).unwrap();
        app.set_viewport_px(viewport.0, viewport.1);
        app
    }

    /// Nombre de cellules portant un glyphe Braille (`U+2800..=U+28FF`).
    fn braille_glyphs(buf: &astre_core::CellBuffer) -> usize {
        buf.cells()
            .iter()
            .filter(|c| (0x2800..=0x28FF).contains(&c.glyph))
            .count()
    }

    #[test]
    fn new_accepts_an_empty_document() {
        assert!(App::build("{}").is_ok());
        assert!(App::build("").is_ok());
        assert!(App::build("pas du json").is_err());
    }

    #[test]
    fn viewport_sets_a_grid_with_letterbox() {
        let app = app((1920, 1080));
        let grid = app.grid.unwrap();
        assert_eq!(grid.cell_px, PixelSize::new(16, 32));
        assert_eq!(grid.cols, 120);
        assert_eq!(grid.rows, 33);
        assert_eq!(grid.origin_px.0, 0);
        assert_eq!(grid.cell_px.h, grid.cell_px.w * 2);
        assert_eq!(app.core.cells().cols(), 120);
        assert_eq!(app.core.cells().rows(), 33);
    }

    #[test]
    fn frame_before_viewport_is_skipped() {
        let mut app = App::build("{}").unwrap();
        assert_eq!(app.frame(0.0) & frame_bits::SKIPPED, frame_bits::SKIPPED);
    }

    #[test]
    fn frame_renders_and_packs_the_instances() {
        let mut app = app((1280, 720));
        let bits = app.frame(16.0);
        assert_eq!(bits & frame_bits::RENDERED, frame_bits::RENDERED);
        assert_eq!(bits & frame_bits::ERROR, 0);
        let cells = app.core.cells();
        let expected = cells.cols() as usize * cells.rows() as usize * 2;
        assert_eq!(app.instances.len(), expected);
        assert_eq!(app.cells_len(), expected as u32 * 4);
        assert_eq!(app.info_ptr(), &app.info as *const PresentInfo as *const u8);
        assert!(app.last_error().is_empty(), "aucune erreur attendue");
    }

    #[test]
    fn a_frame_without_elapsed_time_is_skipped() {
        let mut app = app((800, 600));
        let first = app.frame(100.0);
        assert_eq!(first & frame_bits::RENDERED, frame_bits::RENDERED);
        let second = app.frame(100.0);
        assert_eq!(second & frame_bits::SKIPPED, frame_bits::SKIPPED);
        // Une touche force un nouveau rendu même sans temps écoulé.
        app.push_key(key::LEFT, true);
        let third = app.frame(100.0);
        assert_eq!(third & frame_bits::RENDERED, frame_bits::RENDERED);
    }

    #[test]
    fn nan_timestamp_is_an_error_but_the_app_survives() {
        let mut app = app((800, 600));
        let bits = app.frame(f64::NAN);
        assert_eq!(bits & frame_bits::ERROR, frame_bits::ERROR);
        assert!(!app.last_error().is_empty());
        // L'état reste sain : la frame suivante rend normalement.
        let bits = app.frame(10.0);
        assert_eq!(bits & frame_bits::RENDERED, frame_bits::RENDERED);
    }

    #[test]
    fn zoom_keys_walk_the_ladder_and_clamp() {
        let mut app = app((1600, 900));
        app.set_cell_px(ZOOM_LEVELS[0]);
        let first = app.cell_px();
        assert_eq!(first, ZOOM_LEVELS[0]);
        // 40 crans vers le bas : on reste au palier minimal, sans erreur.
        for _ in 0..40 {
            app.push_key(KEY_MINUS, true);
        }
        assert_eq!(app.cell_px(), ZOOM_LEVELS[0]);
        // 40 crans vers le haut : palier maximal.
        for _ in 0..40 {
            app.push_key(KEY_PLUS, true);
        }
        assert_eq!(app.cell_px(), ZOOM_LEVELS[ZOOM_LEVELS.len() - 1]);
        // `0` revient au palier automatique.
        app.push_key(KEY_ZERO, true);
        let level = astre_core::auto_level();
        assert_eq!(app.cell_px(), ZOOM_LEVELS[level]);
        assert!(app.core.cells().cols() * app.core.cells().rows() <= 1_000_000);
    }

    #[test]
    fn zoom_never_exceeds_the_cell_budget() {
        let mut app = app((4096, 2304));
        app.set_cell_px(ZOOM_LEVELS[0]);
        let cells = app.core.cells();
        assert!(cells.cols() * cells.rows() <= 1_000_000);
        assert_eq!(
            app.grid.unwrap().cells(),
            (cells.cols() * cells.rows()) as u32
        );
    }

    #[test]
    fn set_viewport_is_idempotent_and_epoch_tracks_resizes() {
        let mut app = app((800, 600));
        let epoch = app.core.cells().epoch();
        app.set_viewport_px(800, 600);
        assert_eq!(app.core.cells().epoch(), epoch);
        app.set_viewport_px(1024, 768);
        assert!(app.core.cells().epoch() > epoch);
    }

    #[test]
    fn degenerate_viewport_leaves_no_grid() {
        let mut app = app((0, 0));
        assert!(app.grid.is_none());
        assert_eq!(app.frame(1.0) & frame_bits::SKIPPED, frame_bits::SKIPPED);
        app.set_viewport_px(640, 480);
        assert!(app.grid.is_some());
    }

    #[test]
    fn quit_key_sets_the_flag_and_keeps_running() {
        let mut app = app((640, 480));
        app.push_key(b'q' as u32, true);
        let bits = app.frame(16.0);
        assert_eq!(bits & frame_bits::QUIT, frame_bits::QUIT);
        assert_eq!(bits & frame_bits::RENDERED, frame_bits::RENDERED);
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut app = app((640, 480));
        app.frame(16.0);
        let before = app.core.orbit().azimuth;
        app.push_key(key::LEFT, false);
        assert_eq!(app.core.orbit().azimuth, before);
        app.push_key(key::LEFT, true);
        assert_ne!(app.core.orbit().azimuth, before);
    }

    /// `M` doit changer le buffer d'instances GPU (chemin web complet : clé -> `M` ->
    /// `astre-app::input` -> `orbit.mode` -> `render_frame` -> `resolve` -> packing).
    ///
    /// HUD masqué dans la configuration : la seule différence possible vient de la
    /// **résolution**, pas du libellé « BRAILLE/ASCII » du bandeau. Aucun temps ne s'écoule
    /// entre les frames (`now_ms` constant, `push_key` force le rendu) : la scène est
    /// identique, seul le mode change.
    #[test]
    fn m_key_toggles_the_glyphs_in_the_instance_buffer() {
        let mut app = App::build(r#"{"cellPx":16,"hud":false}"#).unwrap();
        app.set_viewport_px(640, 480);
        assert_eq!(app.frame(16.0) & frame_bits::RENDERED, frame_bits::RENDERED);
        let before = app.instances.clone();
        assert!(
            braille_glyphs(app.core.cells()) > 0,
            "mode initial = Braille"
        );

        app.push_key(b'M' as u32, true);
        assert_eq!(app.frame(16.0) & frame_bits::RENDERED, frame_bits::RENDERED);
        assert_ne!(app.instances, before, "`M` doit changer les instances GPU");
        assert_eq!(
            braille_glyphs(app.core.cells()),
            0,
            "le mode ASCII ne doit produire aucun glyphe Braille"
        );

        // Aller-retour : `m` retrouve exactement le buffer Braille.
        app.push_key(b'm' as u32, true);
        app.frame(16.0);
        assert_eq!(app.instances, before);
    }

    #[test]
    fn stats_json_is_well_formed_and_carries_the_grid() {
        let mut app = app((1280, 720));
        app.frame(16.0);
        app.frame(33.0);
        let json = app.stats_json();
        assert!(json.starts_with('{') && json.ends_with('}'));
        assert!(json.contains("\"cols\":"));
        assert!(json.contains("\"cellPx\":16"));
        assert!(json.contains("\"atlasSlots\":512"));
        // Aucun `NaN`/`Infinity` : le JSON doit rester parseable par `JSON.parse`.
        assert!(!json.contains("NaN"));
        assert!(!json.contains("inf"));
        assert!(!json.contains("e-"));
    }

    #[test]
    fn atlas_and_cell_exports_are_coherent() {
        let app = app((1024, 768));
        assert_eq!(app.atlas_slot_count(), 512);
        assert_eq!(app.info_words(), 16);
        assert_eq!(PresentInfo::WORDS, app.info_words());
    }

    #[test]
    fn slot_table_matches_the_abi() {
        assert_eq!(slot_for(' ' as u32), 0);
        assert_eq!(instances::BRAILLE_FIRST_SLOT, 96);
        assert_eq!(instances::BLOCK_FIRST_SLOT, 352);
        assert!(slot_for(0x28FF) < app((10, 10)).atlas_slot_count());
    }

    #[test]
    fn key_codes_follow_the_c_enumeration() {
        assert_eq!(key_from_code(256), Some(KeyCode::Up));
        assert_eq!(key_from_code(257), Some(KeyCode::Down));
        assert_eq!(key_from_code(258), Some(KeyCode::Right));
        assert_eq!(key_from_code(259), Some(KeyCode::Left));
        assert_eq!(key_from_code(32), Some(KeyCode::Char(' ')));
        assert_eq!(key_from_code(126), Some(KeyCode::Char('~')));
        assert_eq!(key_from_code(0), None);
        assert_eq!(key_from_code(u32::MAX), None);
    }
}
