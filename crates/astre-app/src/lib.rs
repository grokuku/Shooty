//! # astre-app
//!
//! Application Astre — port de `test3Dterminal/demo.c` (C-REF, gelé le 2026-11-24),
//! conformément à `docs/ARCHITECTURE.md` §B.2, §C.3, §C.4, §D et §G.1/G.6.
//!
//! ## Contenu
//!
//! | Module | Origine C | Rôle |
//! | --- | --- | --- |
//! | [`cli`] | `demo.c:{Options,number,dimensions,usage,main}` | parseur maison (§G.6), mêmes bornes/messages/codes |
//! | [`clock`] | nouveau (Phase 3) | horloge **injectée** : `Instant::now()` panique sur wasm32 |
//! | [`orbit`] | `demo.c:{Orbit,reset_camera,keypress,limit}` | état orbital et contraintes caméra |
//! | [`input`] | `demo.c:keypress` | `KeyCode -> Action -> apply`, table remappable |
//! | [`sim`] | remplace `demo.c:{now,wait_frame,live}` | pas fixe, rattrapage borné, interpolation |
//! | [`metrics`] | `demo.c:live` (fenêtre 0,5 s) | `FrameMetrics`, `built_fps`/`presented_fps`, ms par phase |
//! | [`overlay`] | `demo.c:overlay` | HUD et panneaux, libellés contractuels (§G.4) |
//! | [`render`] | `demo.c:render` | pipeline `begin -> compose -> resolve -> HUD` |
//! | [`presenter`] | §C.3 | trait `Presenter` + `PresentConfig`/`PresentOutcome` |
//! | [`app`] | `demo.c:main`/`live` | orchestration sans horloge implicite |
//!
//! ## Ce que le portage change (et ce qu'il ne change pas)
//!
//! - **Timing** : `dt` variable + `wait_frame` du C remplacés par un pas fixe interpolé
//!   (§D.1, §D.2) — la trajectoire reste celle du C pour un même nombre de ticks.
//! - **Compteurs** : `fps` unique du C remplacé par `built_fps`/`presented_fps` et des
//!   millisecondes par phase (§D.5, §D.6).
//! - **Rendu** : strictement identique — mêmes chaînes de HUD, mêmes paliers de largeur,
//!   même scène, même ordre d'appels.
//!
//! Aucun état global, aucun `unsafe`, aucune dépendance externe.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod app;
pub mod cli;
pub mod clock;
pub mod input;
pub mod metrics;
pub mod orbit;
pub mod overlay;
pub mod presenter;
pub mod render;
pub mod sim;

pub use app::App;
pub use cli::{CliError, Command, Options, Parsed, USAGE};
pub use clock::{Clock, NullClock, StdClock};
pub use input::{Action, InputMap, KeyCode};
pub use metrics::{FrameMetrics, Metrics};
pub use orbit::Orbit;
pub use overlay::overlay;
pub use presenter::{PresentConfig, PresentOutcome, Presenter};
pub use render::{render_frame, RenderTimings};
pub use sim::{SceneState, Sim, Step};
