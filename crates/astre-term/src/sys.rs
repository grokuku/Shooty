//! FFI POSIX minimale — **seul module du workspace autorisé à contenir de l'`unsafe`**.
//!
//! ## Pourquoi `libc` et pas `crossterm` (question ouverte 6, tranchée)
//!
//! | Critère | `crossterm` | `libc` seule (retenu) |
//! | --- | --- | --- |
//! | Dépendances ajoutées | ~8 à 15 crates (dont `libc`, `mio`/`signal-hook`) | **1** |
//! | Encodeur ANSI | ne le fournit pas : la parité octet à octet de `t3_present` reste à écrire | idem, écrit ici |
//! | `SIGTSTP` (Ctrl-Z) | non géré : il faudrait `libc` en plus (§G.1) | géré |
//! | Taille de terminal | fournie | `ioctl(TIOCGWINSZ)`, 10 lignes |
//! | `unsafe` dans *notre* code | aucun | confiné à ce fichier |
//!
//! Le seul avantage de `crossterm` serait de garder `forbid(unsafe_code)` dans ce crate ;
//! il est acheté au prix de dépendances qui, de toute façon, ne couvrent pas `SIGTSTP`
//! ni l'encodeur différentiel (qui est tout l'objet de `astre-term`). Arbitrage retenu :
//! une dépendance `libc` (std-adjacente, `1.0`-stable depuis 2016), `unsafe` **confiné**
//! à ce module, et `deny(unsafe_code)` partout ailleurs.
//!
//! ## Ce que fait ce module
//!
//! - `termios` : passage en mode brut (`~ICANON`, `~ECHO`, `~IXON`, `~ICRNL`, `VMIN=0`,
//!   `VTIME=0`) et restauration ;
//! - `ioctl(TIOCGWINSZ)` : taille du terminal ;
//! - `read(2)` non bloquant sur `stdin` (le C faisait exactement ça) ;
//! - signaux `SIGINT`/`SIGTERM`/`SIGHUP`/`SIGQUIT`/`SIGTSTP` : drapeaux atomiques, comme
//!   les `volatile sig_atomic_t` du C, mais sans `static mut` ;
//! - restauration du terminal en cas de **panic** : `panic = "abort"` en release, donc
//!   les `Drop` ne s'exécutent pas — un *panic hook* rétablit l'écran puis relaie.
//!
//! Aucun verrou n'est pris depuis un gestionnaire de signal : les handlers ne font que
//! `store` un `AtomicBool` (async-signal-safe).

#![allow(unsafe_code)]

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once};

/// Séquence d'entrée sur l'écran alterné (`enter_screen` du C).
pub const ENTER_SCREEN: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[2J";
/// Séquence de sortie de l'écran alterné (`leave_screen` du C).
pub const LEAVE_SCREEN: &[u8] = b"\x1b[0m\x1b[?25h\x1b[?1049l";

/// `struct termios` opaque pour le reste du crate.
pub type Termios = libc::termios;

/// Numéro de signal (`libc::c_int` sous Unix).
pub type Signal = libc::c_int;

/// Gestionnaire de signal précédent (`signal(2)` le rend sous cette forme).
pub type SignalHandler = libc::sighandler_t;

/// Signaux interceptés, dans l'ordre du C (`SIGTSTP` traité à part).
const SIGNALS: [libc::c_int; 5] = [
    libc::SIGINT,
    libc::SIGTERM,
    libc::SIGHUP,
    libc::SIGQUIT,
    libc::SIGTSTP,
];

/// `quitting` du C, en atomique.
static QUITTING: AtomicBool = AtomicBool::new(false);
/// `suspending` du C, en atomique.
static SUSPENDING: AtomicBool = AtomicBool::new(false);
/// Termios d'origine, publié pour le *panic hook* et la reprise après `SIGTSTP`.
static ORIGINAL: Mutex<Option<Termios>> = Mutex::new(None);

/// Gestionnaire de signal : `store` sur deux drapeaux, rien d'autre.
extern "C" fn on_signal(sig: libc::c_int) {
    if sig == libc::SIGTSTP {
        SUSPENDING.store(true, Ordering::SeqCst);
    } else {
        QUITTING.store(true, Ordering::SeqCst);
    }
}

/// `true` tant qu'aucun signal d'arrêt n'a été reçu (`t3_terminal_running` du C).
pub fn running() -> bool {
    !QUITTING.load(Ordering::SeqCst)
}

/// Consomme une demande de suspension (`Ctrl-Z`).
pub fn take_suspend() -> bool {
    SUSPENDING.swap(false, Ordering::SeqCst)
}

/// Réarme les drapeaux (début de session).
pub fn reset_flags() {
    QUITTING.store(false, Ordering::SeqCst);
    SUSPENDING.store(false, Ordering::SeqCst);
}

/// `isatty(STDIN)` **et** `isatty(STDOUT)`, comme `t3_terminal_open`.
pub fn is_tty() -> bool {
    unsafe { libc::isatty(libc::STDIN_FILENO) == 1 && libc::isatty(libc::STDOUT_FILENO) == 1 }
}

/// Taille du terminal via `ioctl(TIOCGWINSZ)` (`t3_terminal_size` du C).
pub fn size() -> Option<(i32, i32)> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) };
    if rc < 0 || ws.ws_col == 0 || ws.ws_row == 0 {
        return None;
    }
    Some((i32::from(ws.ws_col), i32::from(ws.ws_row)))
}

/// Installe le mode brut et retourne le termios d'origine.
///
/// # Errors
///
/// Erreur `io` si `tcgetattr`/`tcsetattr` échoue (entrée non-TTY, terminal exotique).
pub fn enable_raw() -> io::Result<Termios> {
    let mut original: Termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut original) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut active = original;
    active.c_lflag &= !(libc::ICANON | libc::ECHO);
    active.c_iflag &= !(libc::IXON | libc::ICRNL);
    active.c_cc[libc::VMIN] = 0;
    active.c_cc[libc::VTIME] = 0;
    if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &active) } < 0 {
        return Err(io::Error::last_os_error());
    }
    if let Ok(mut slot) = ORIGINAL.lock() {
        *slot = Some(original);
    }
    Ok(original)
}

/// Restaure un termios précédemment sauvé (`tcsetattr(TCSANOW)` du C).
pub fn restore_termios(original: &Termios) {
    unsafe {
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, original);
    }
}

/// Publie le termios courant (utilisé après une reprise de `SIGTSTP`).
pub fn remember(original: &Termios) {
    if let Ok(mut slot) = ORIGINAL.lock() {
        *slot = Some(*original);
    }
}

/// Oublie le termios publié (fin de session).
pub fn forget() {
    if let Ok(mut slot) = ORIGINAL.lock() {
        *slot = None;
    }
}

/// Écrit des octets bruts sur `stdout` **sans verrou** (utilisable en `Drop`/hook).
///
/// Retourne `true` si tout a été écrit (`write(2)` peut être partiel sur un TTY).
pub fn write_stdout(bytes: &[u8]) -> bool {
    let mut rest = bytes;
    while !rest.is_empty() {
        let n = unsafe { libc::write(libc::STDOUT_FILENO, rest.as_ptr().cast(), rest.len()) };
        if n <= 0 {
            return false;
        }
        // `n <= rest.len()` par construction de `write(2)`.
        rest = &rest[n as usize..];
    }
    true
}

/// Lit ce qui est disponible sur `stdin`. Retourne 0 si la file est vide.
pub fn read_stdin(buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    let n = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr().cast(), buf.len()) };
    if n <= 0 {
        0
    } else {
        n as usize
    }
}

/// Installe les gestionnaires de signaux et retourne les précédents pour restauration.
pub fn install_signals() -> Vec<(libc::c_int, libc::sighandler_t)> {
    let mut saved = Vec::with_capacity(SIGNALS.len());
    for &sig in &SIGNALS {
        let prev = unsafe { libc::signal(sig, on_signal as *const () as usize) };
        saved.push((sig, prev));
    }
    saved
}

/// Rétablit les gestionnaires précédents (`t3_terminal_close` du C).
pub fn restore_signals(saved: &[(libc::c_int, libc::sighandler_t)]) {
    for &(sig, prev) in saved {
        unsafe {
            libc::signal(sig, prev);
        }
    }
}

/// Stoppe le processus (`raise(SIGSTOP)`) après avoir quitté l'écran alterné.
pub fn stop_process() {
    unsafe {
        libc::raise(libc::SIGSTOP);
    }
}

/// Rétablit le terminal si un panic survient avant que les `Drop` puissent courir.
///
/// `profile.release.panic = "abort"` : sans ce hook, un panic laisserait le terminal en
/// mode brut et sur l'écran alterné. Idempotent (installé une seule fois).
pub fn install_panic_restore() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            emergency_restore();
            previous(info);
        }));
    });
}

/// Sortie de secours : quitte l'écran alterné et rend le termios d'origine.
///
/// Sans verrou bloquant : si le panic a eu lieu en tenant `ORIGINAL`, on se contente
/// d'écrire la séquence ANSI (le shell refera un `stty sane` si nécessaire).
pub fn emergency_restore() {
    write_stdout(LEAVE_SCREEN);
    if let Ok(slot) = ORIGINAL.try_lock() {
        if let Some(original) = slot.as_ref() {
            restore_termios(original);
        }
    }
}
