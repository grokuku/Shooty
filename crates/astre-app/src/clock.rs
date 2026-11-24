//! Source de temps **injectable** — indispensable à la cible WASM.
//!
//! `std::time::Instant::now()` **panique** (trap `unreachable`) sur
//! `wasm32-unknown-unknown` : la PAL `std` de cette cible n'a pas d'horloge. Un
//! `astre-app` qui appellerait `Instant::now()` en dur serait donc inutilisable depuis
//! `astre-web`. Tout le chronométrage des phases (§D.6) passe donc par ce trait, avec
//! un point d'injection unique par frame.
//!
//! Trois implémentations, jamais plus :
//!
//! | Implémentation | Cible | Mécanisme |
//! | --- | --- | --- |
//! | [`StdClock`] | native, tests hôte | `std::time::Instant`, monotone |
//! | [`NullClock`] | rendus déterministes (`--snapshot`, tests) | constante `0` |
//! | `astre_web::PerfClock` | WASM | `performance.now()`, importé par `wasm-bindgen` |
//!
//! Le contrat est minimal : [`Clock::now_us`] rend des **microsecondes monotones**. Un
//! appelant qui remonte le temps (`now_us` plus petit qu'un relevé précédent) ne casse
//! rien : toutes les soustractions sont saturantes (`saturating_sub`).

/// Horloge monotone en microsecondes.
pub trait Clock {
    /// Microsecondes écoulées depuis une origine arbitraire mais **monotone**.
    fn now_us(&self) -> u64;
}

/// Horloge native : `std::time::Instant`.
///
/// Ne jamais l'instancier sur `wasm32` (l'appel à `Instant::now()` y panique) :
/// `astre-web` utilise `performance.now()`.
#[derive(Clone, Copy, Debug)]
pub struct StdClock {
    base: std::time::Instant,
}

impl StdClock {
    /// Origine = maintenant.
    #[must_use]
    pub fn new() -> Self {
        Self {
            base: std::time::Instant::now(),
        }
    }
}

impl Default for StdClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for StdClock {
    #[inline]
    fn now_us(&self) -> u64 {
        u64::try_from(self.base.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

/// Horloge figée : toujours `0`.
///
/// Sert aux chemins **déterministes** (`--snapshot`, goldens, tests unitaires) où les
/// durées de phase n'ont aucun sens : elles valent alors `0`, sans effet sur l'image.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullClock;

impl Clock for NullClock {
    #[inline]
    fn now_us(&self) -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_clock_is_monotone() {
        let clock = StdClock::new();
        let a = clock.now_us();
        let b = clock.now_us();
        assert!(b >= a);
    }

    #[test]
    fn null_clock_is_frozen() {
        let clock = NullClock;
        assert_eq!(clock.now_us(), 0);
        assert_eq!(clock.now_us(), 0);
    }
}
