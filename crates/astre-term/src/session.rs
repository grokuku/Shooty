//! Session terminal — port de `terminal.c:t3_terminal_{open,close,running,size,key}`
//! (Unix), avec la sûreté en plus : la restauration est portée par un `Drop`.
//!
//! [`AlternateScreen`] est le **garde** demandé : tant qu'il vit, le terminal est en mode
//! brut et sur l'écran alterné. Il est restauré :
//!
//! 1. normalement, via `Drop` ;
//! 2. sur `Ctrl-C`/`SIGTERM`/`SIGHUP`/`SIGQUIT` (drapeau atomique lu par la boucle, qui
//!    sort par le chemin normal) ;
//! 3. sur `Ctrl-Z`, par la séquence « quitter l'écran -> `SIGSTOP` -> ré-entrer ->
//!    redraw complet » du C ;
//! 4. sur **panic**, par le *panic hook* de [`crate::sys`] (nécessaire parce que
//!    `profile.release.panic = "abort"` n'exécute aucun `Drop`).
//!
//! Décodage des touches : machine à états `escape` identique au C
//! (`ESC` -> `[`/`O` -> lettre finale), une touche par appel, `None` si la file est vide.

use std::io;

use astre_app::KeyCode;

use crate::sys;

/// Erreur d'ouverture de session.
#[derive(Debug)]
pub enum SessionError {
    /// L'entrée ou la sortie n'est pas un terminal (mode `--snapshot` requis).
    NotATerminal,
    /// Appel système en échec (`tcgetattr`, `tcsetattr`, écriture de l'écran alterné).
    Io(io::Error),
}

impl core::fmt::Display for SessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotATerminal => {
                f.write_str("Utilisez un terminal interactif ou ajoutez --snapshot.")
            }
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<io::Error> for SessionError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Garde d'écran alterné + entrée brute (`T3Terminal` du C, partie session).
#[derive(Debug)]
pub struct AlternateScreen {
    original: sys::Termios,
    signals: Vec<(sys::Signal, sys::SignalHandler)>,
    escape: u8,
    resumed: bool,
    active: bool,
}

impl AlternateScreen {
    /// Ouvre la session : vérifie le TTY, passe en mode brut, installe les signaux et
    /// entre sur l'écran alterné.
    ///
    /// # Errors
    ///
    /// [`SessionError::NotATerminal`] hors TTY, [`SessionError::Io`] si un appel système
    /// échoue (le terminal est alors laissé tel qu'il était).
    pub fn open() -> Result<Self, SessionError> {
        if !sys::is_tty() {
            return Err(SessionError::NotATerminal);
        }
        sys::install_panic_restore();
        sys::reset_flags();
        let original = sys::enable_raw()?;
        let signals = sys::install_signals();
        if !sys::write_stdout(sys::ENTER_SCREEN) {
            sys::restore_signals(&signals);
            sys::restore_termios(&original);
            sys::forget();
            return Err(SessionError::Io(io::Error::other(
                "impossible d'entrer sur l'écran alterné",
            )));
        }
        Ok(Self {
            original,
            signals,
            escape: 0,
            resumed: false,
            active: true,
        })
    }

    /// Taille du terminal en cellules, `None` si indisponible (`t3_terminal_size`).
    #[must_use]
    pub fn size(&self) -> Option<(i32, i32)> {
        sys::size()
    }

    /// `true` tant qu'aucun signal d'arrêt n'a été reçu (`t3_terminal_running`).
    #[must_use]
    pub fn running(&self) -> bool {
        sys::running()
    }

    /// `true` (une fois) si le terminal vient d'être repris après `Ctrl-Z`.
    ///
    /// L'appelant doit alors invalider le présentateur : le contenu de l'écran a été
    /// effacé par la sortie de l'écran alterné.
    pub fn take_resumed(&mut self) -> bool {
        core::mem::take(&mut self.resumed)
    }

    /// Traite une demande de suspension (`Ctrl-Z`) comme le C.
    pub fn handle_suspend(&mut self) {
        if !sys::take_suspend() || !sys::running() {
            return;
        }
        sys::write_stdout(sys::LEAVE_SCREEN);
        sys::restore_termios(&self.original);
        sys::stop_process();
        // Reprise : `SIGCONT` a rendu la main, on réinstalle exactement l'état d'entrée.
        let _ = sys::enable_raw();
        sys::write_stdout(sys::ENTER_SCREEN);
        self.escape = 0;
        self.resumed = true;
    }

    /// Lit une touche, `None` si la file est vide (`t3_terminal_key`, rendait `-1`).
    ///
    /// Comme le C, on **boucle** sur les octets disponibles jusqu'à trouver une touche
    /// mappée : un octet de contrôle isolé (ou une séquence `ESC` inconnue) est ignoré
    /// plutôt que rendu, le `keypress` de la démo les ignorait de toute façon.
    pub fn poll_key(&mut self) -> Option<KeyCode> {
        let mut byte = [0u8; 1];
        while sys::read_stdin(&mut byte) == 1 {
            let c = byte[0];
            if c == 27 {
                self.escape = 1;
                continue;
            }
            if self.escape == 1 && (c == b'[' || c == b'O') {
                self.escape = 2;
                continue;
            }
            if self.escape == 2 {
                if c < 0x40 {
                    continue;
                }
                self.escape = 0;
                match c {
                    b'A' => return Some(KeyCode::Up),
                    b'B' => return Some(KeyCode::Down),
                    b'C' => return Some(KeyCode::Right),
                    b'D' => return Some(KeyCode::Left),
                    _ => continue,
                }
            }
            self.escape = 0;
            if let Some(key) = decode_byte(c) {
                return Some(key);
            }
        }
        None
    }

    /// Restaure explicitement le terminal (idempotent ; fait aussi par `Drop`).
    pub fn close(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        sys::write_stdout(sys::LEAVE_SCREEN);
        sys::restore_termios(&self.original);
        sys::restore_signals(&self.signals);
        sys::forget();
    }
}

/// Traduit un octet brut en touche (parité de la table du C).
fn decode_byte(c: u8) -> Option<KeyCode> {
    match c {
        b'\t' => Some(KeyCode::Tab),
        27 => None,
        32..=126 => Some(KeyCode::Char(char::from(c))),
        _ => None,
    }
}

impl Drop for AlternateScreen {
    fn drop(&mut self) {
        self.close();
    }
}
