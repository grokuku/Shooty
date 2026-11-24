//! # astre-term
//!
//! Backend terminal natif — port de `test3Dterminal/terminal.c` (C-REF), conformément à
//! `docs/ARCHITECTURE.md` §B.2 et §G.1.
//!
//! ## Contenu
//!
//! | Module | Origine C | Rôle |
//! | --- | --- | --- |
//! | [`sys`] | `terminal.c` (Unix) | FFI POSIX **confinée** : `termios`, `ioctl`, `read`, signaux |
//! | [`session`] | `t3_terminal_{open,close,running,size,key}` | [`session::AlternateScreen`] : garde mode brut + écran alterné + touches |
//! | [`ansi`] | `{utf8,nearest_cube,indexed,ink,emit,equal_cell,t3_present,t3_snapshot}` | encodeur **différentiel** (parité octet à octet) |
//! | [`presenter`] | `t3_present` | [`presenter::TerminalPresenter`] : implémentation de `astre_app::Presenter` |
//!
//! ## Dépendance ajoutée : `libc` (et pas `crossterm`)
//!
//! Question ouverte 6 tranchée : **`libc` seule**. `crossterm` n'évite pas `libc` (il en
//! dépend), n'apporte pas l'encodeur différentiel — qui est ici tout l'objet du crate et
//! doit rester identique octet à octet au C — et ne gère pas `SIGTSTP`, qu'il faudrait de
//! toute façon écrire à la main (§G.1). Voir la justification détaillée en tête de
//! [`sys`].
//!
//! Conséquence assumée : ce crate est la **seule** exception du workspace à
//! `unsafe_code = "forbid"`, ramené à `deny` avec un `allow` de module **unique**
//! ([`sys`]) qui ne contient que les appels FFI nécessaires.
//!
//! ## Garanties de restauration
//!
//! Le terminal est rendu dans tous les cas : `Drop` de [`session::AlternateScreen`],
//! sortie normale sur `q`, sortie sur signal (`Ctrl-C`, `SIGTERM`), reprise après
//! `Ctrl-Z`, et **panic** (via le *panic hook* de [`sys`], indispensable puisque
//! `profile.release.panic = "abort"` court-circuite les `Drop`).

#![deny(missing_docs)]

pub mod ansi;
pub mod presenter;
pub mod session;

#[cfg(unix)]
pub mod sys;

/// API identique à [`sys`], en échec explicite hors POSIX (voir le module).
#[cfg(not(unix))]
#[path = "sys_stub.rs"]
pub mod sys;

pub use ansi::{cell_equals, indexed, snapshot, AnsiEncoder};
pub use presenter::TerminalPresenter;
pub use session::{AlternateScreen, SessionError};
