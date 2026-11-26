//! # shooty — binaire natif
//!
//! `shooty [options]` lance le jeu dans le terminal courant, `shooty --snapshot` écrit
//! une image ANSI sur `stdout` et sort (mode headless, sans `termios`).
//!
//! Toute la mécanique (analyse de la CLI, boucle à pas fixe, présentation, HUD chrome)
//! vient du moteur HET : ce binaire ne fait que choisir la scène
//! ([`ShootyScene`]) et la passer à [`het_app::App`].
//!
//! Codes de sortie, ceux du moteur :
//!
//! | Cas | Code |
//! | --- | --- |
//! | succès, `--help` | 0 |
//! | échec de rendu/présentation, pas de terminal interactif | 1 |
//! | option invalide (message + usage sur `stderr`) | 2 |
//!
//! Options CLI : celles de `het_app::cli` (`--snapshot`, `--size`, `--time`,
//! `--plain`, `--mode`, `--palette`, `--fps`, `--tick-hz`, `--cell-aspect`,
//! `--no-hud`). Aucune option propre au jeu n'existe encore.

use std::io::Write;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use het_app::cli::{self, Command, Options};
use het_app::{App, Clock, PresentOutcome, Presenter, StdClock};
use het_term::{AlternateScreen, TerminalPresenter};
use shooty_game::ShootyScene;

/// Code de sortie d'une erreur d'exécution (contrat du moteur).
const EXIT_FAILURE: u8 = 1;
/// Code de sortie d'une option invalide (contrat du moteur).
const EXIT_USAGE: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = cli::usage("shooty");
    let parsed = match cli::parse(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{}", error.message);
            eprint!("{usage}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    if parsed.command == Command::Help {
        print!("{usage}");
        return ExitCode::SUCCESS;
    }

    let options = parsed.options;
    let result = if options.snapshot {
        run_snapshot(&options)
    } else {
        run_live(&options)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}

/// Mode headless : rend une image et l'écrit sur `stdout` (`--snapshot`).
///
/// Ne touche **ni** `isatty` **ni** `termios` (§H.5) : utilisable en CI, sans TTY.
fn run_snapshot(options: &Options) -> Result<(), u8> {
    let mut app = App::new(*options, ShootyScene::default()).map_err(|e| {
        eprintln!(
            "Impossible de creer une grille {}x{} : {e}",
            options.cols, options.rows
        );
        EXIT_FAILURE
    })?;
    app.render(&het_app::NullClock);

    let stdout = std::io::stdout();
    let mut presenter = TerminalPresenter::new(stdout.lock());
    presenter
        .snapshot(app.cells(), options.palette)
        .map_err(|e| {
            eprintln!("Impossible d'ecrire l'image : {e}");
            EXIT_FAILURE
        })?;
    Ok(())
}

/// Mode interactif : session terminal brute + boucle à pas fixe (§D.4).
fn run_live(options: &Options) -> Result<(), u8> {
    let mut session = AlternateScreen::open().map_err(|e| {
        eprintln!("{e}");
        EXIT_FAILURE
    })?;

    let mut app = App::new(*options, ShootyScene::default()).map_err(|e| {
        eprintln!(
            "Impossible de creer une grille {}x{} : {e}",
            options.cols, options.rows
        );
        EXIT_FAILURE
    })?;

    let mut presenter = TerminalPresenter::new(std::io::stdout());
    presenter.configure(app.present_config()).map_err(|e| {
        eprintln!("Impossible de presenter l'image dans ce terminal : {e}");
        EXIT_FAILURE
    })?;

    // Première taille : celle du terminal, avant toute image (§F.6 appliqué au natif).
    if let Some((cols, rows)) = session.size() {
        if let Err(e) = app.resize(cols, rows) {
            eprintln!("Taille de terminal invalide ({cols}x{rows}) : {e}");
            return Err(EXIT_FAILURE);
        }
    }
    presenter.invalidate();

    let clock = StdClock::new();
    let mut last = Instant::now();
    let mut ok = true;
    loop {
        // Reprise après Ctrl-Z : l'écran a été effacé, il faut tout redessiner.
        session.handle_suspend();
        if session.take_resumed() {
            presenter.invalidate();
        }
        if !session.running() {
            break;
        }

        let frame_start = Instant::now();
        let frame_dt = frame_start.duration_since(last).as_secs_f32();
        last = frame_start;

        // 2-3. Événements -> caméra, mode, arrêt.
        let mut quit = false;
        while let Some(key) = session.poll_key() {
            if !app.apply_key(key) {
                quit = true;
                break;
            }
        }
        if quit || !session.running() {
            break;
        }

        // 4. Simulation à pas fixe.
        app.advance(frame_dt, &clock);

        // Changement de taille éventuel (aucune allocation si la taille est identique).
        if let Some((cols, rows)) = session.size() {
            match app.resize(cols, rows) {
                Ok(true) => presenter.invalidate(),
                Ok(false) => {}
                Err(e) => {
                    eprintln!("Taille de terminal invalide ({cols}x{rows}) : {e}");
                    ok = false;
                    break;
                }
            }
        }

        // 5-9. Rendu (begin, scène, resolve, HUD).
        app.render(&clock);

        // 10. Présentation — seul point de sortie.
        let t_present = Instant::now();
        let outcome = match app.present(&mut presenter) {
            Ok(outcome) => outcome,
            Err(e) => {
                eprintln!("Impossible de presenter l'image dans ce terminal : {e}");
                ok = false;
                break;
            }
        };
        let present_us = u32::try_from(t_present.elapsed().as_micros()).unwrap_or(u32::MAX);
        let presented = matches!(outcome, PresentOutcome::Presented { .. });

        // 11. Compteurs (fenêtre 0,5 s, comme le C).
        app.observe(clock.now_us(), present_us, presented);

        // Pas de verrou 60 Hz : on dort jusqu'au **prochain tick** (§D.2), en retirant le
        // travail déjà effectué, et en respectant le plafond `--fps` s'il est demandé.
        let work = frame_start.elapsed().as_secs_f32();
        let mut wait = app.sim().time_to_next_tick() - work;
        if options.fps > 0 {
            let cap = 1.0 / options.fps as f32;
            wait = wait.max(cap - work);
        }
        if wait > 0.0 {
            std::thread::sleep(Duration::from_secs_f32(wait));
        }
    }

    // Sortie explicite du garde : `Drop` le ferait, mais le code de retour doit être
    // décidé après restauration.
    session.close();
    let _ = std::io::stdout().flush();
    if ok {
        Ok(())
    } else {
        Err(EXIT_FAILURE)
    }
}
