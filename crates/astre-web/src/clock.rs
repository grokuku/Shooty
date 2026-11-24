//! Horloge monotone de la cible WASM — `performance.now()` importé par `wasm-bindgen`.
//!
//! `std::time::Instant::now()` **panique** sur `wasm32-unknown-unknown` (trap
//! `unreachable`, PAL `std` sans horloge) : c'est la raison d'être du trait
//! [`astre_app::Clock`]. Côté web, la seule horloge disponible sans `web-sys` est
//! `performance.now()`, importée directement par `wasm-bindgen` (autorisé : ce n'est ni
//! `web-sys` ni `js-sys`, §B.5.2) :
//!
//! ```js
//! // ce que le glue wasm-bindgen génère, à peu près
//! import { performance } from 'node:perf_hooks'; // (navigateur : global)
//! ```
//!
//! Sur l'hôte (tests `cargo test -p astre-web`), l'import n'existe pas : la même API
//! publique est servie par `astre_app::StdClock`.

#[cfg(target_arch = "wasm32")]
use astre_app::Clock;

/// Horloge `performance.now()`, en microsecondes monotones.
///
/// Une valeur non finie ou négative (navigateur exotique, horloge bridée) est ramenée à
/// `0` : le chronométrage des phases est alors faux, jamais l'image.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default)]
pub struct PlatformClock;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    /// `performance.now()` — millisecondes flottantes depuis l'origine de la page.
    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = performance, catch)]
    fn now() -> Result<f64, wasm_bindgen::JsValue>;
}

#[cfg(target_arch = "wasm32")]
impl Clock for PlatformClock {
    fn now_us(&self) -> u64 {
        match now() {
            Ok(ms) if ms.is_finite() && ms >= 0.0 => (ms * 1000.0) as u64,
            _ => 0,
        }
    }
}

/// Sur l'hôte, la même horloge que le binaire natif.
#[cfg(not(target_arch = "wasm32"))]
pub type PlatformClock = astre_app::StdClock;

/// Construit l'horloge de la cible courante.
#[must_use]
pub fn platform_clock() -> PlatformClock {
    #[cfg(target_arch = "wasm32")]
    {
        PlatformClock
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        PlatformClock::new()
    }
}
