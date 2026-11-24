//! Substitut de [`crate::sys`] pour les plateformes non POSIX (Windows).
//!
//! Le portage Win32 de `terminal.c` (console en `ENABLE_VIRTUAL_TERMINAL_PROCESSING`,
//! `ReadConsoleInputW`, conversion UTF-16 par caractère complet) **reste à faire** : il
//! figure au risque P3 de §I.1 et à la checklist manuelle Windows avant release.
//!
//! Plutôt qu'un `compile_error!` (qui empêcherait tout `cargo build` sur Windows, y
//! compris pour `--snapshot`), ce module garde l'API de [`crate::sys`] et échoue
//! **explicitement** à l'ouverture d'une session interactive. Le mode `--snapshot`, lui,
//! ne touche jamais ce module et fonctionne sur toutes les plateformes.

use std::io;

/// `termios` factice (aucun appel système disponible).
#[derive(Clone, Copy, Debug, Default)]
pub struct Termios;

/// Numéro de signal factice.
pub type Signal = i32;

/// Gestionnaire de signal factice.
pub type SignalHandler = usize;

/// Séquence d'entrée sur l'écran alterné (non utilisée hors POSIX).
pub const ENTER_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[2J";
/// Séquence de sortie de l'écran alterné.
pub const LEAVE_SCREEN: &[u8] = b"\x1b[0m\x1b[?25h\x1b[?1049l";

/// Aucun signal d'arrêt n'est intercepté hors POSIX.
pub fn running() -> bool {
    false
}

/// Aucune suspension gérée hors POSIX.
pub fn take_suspend() -> bool {
    false
}

/// Sans effet.
pub fn reset_flags() {}

/// Faux par construction : la détection de TTY n'est pas portée.
pub fn is_tty() -> bool {
    false
}

/// Taille inconnue.
pub fn size() -> Option<(i32, i32)> {
    None
}

/// Toujours en échec : voir la note de module.
pub fn enable_raw() -> io::Result<Termios> {
    Err(io::Error::other(
        "astre : backend terminal Windows non implemente (Phase 2). Utilisez --snapshot.",
    ))
}

/// Sans effet.
pub fn restore_termios(_original: &Termios) {}

/// Sans effet.
pub fn remember(_original: &Termios) {}

/// Sans effet.
pub fn forget() {}

/// Aucune écriture brute disponible.
pub fn write_stdout(_bytes: &[u8]) -> bool {
    false
}

/// Aucune lecture brute disponible.
pub fn read_stdin(_buf: &mut [u8]) -> usize {
    0
}

/// Aucun gestionnaire installé.
pub fn install_signals() -> Vec<(Signal, SignalHandler)> {
    Vec::new()
}

/// Rien à restaurer.
pub fn restore_signals(_saved: &[(Signal, SignalHandler)]) {}

/// Sans effet.
pub fn stop_process() {}

/// Sans effet.
pub fn install_panic_restore() {}

/// Sans effet.
pub fn emergency_restore() {}
