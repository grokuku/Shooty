//! Test de fumée du binaire `shooty` — mode headless (`--snapshot`).
//!
//! Il ne vérifie **pas** le contenu du jeu (le décor est un placeholder, il changera) :
//! seulement que la chaîne scène → moteur → terminal fonctionne de bout en bout, sans
//! TTY, et que la sortie est déterministe. Les libellés de HUD, eux, appartiennent au
//! moteur HET et sont testés là-bas.

// Les lints du workspace interdisent `expect`/`unwrap` hors tests ; dans un test,
// paniquer sur un échec est précisément le comportement attendu.
#![allow(clippy::expect_used)]

use std::process::{Command, Stdio};

/// Lance le binaire `shooty` avec les arguments donnés, stdin fermé (jamais de TTY).
fn shooty(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_shooty"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("exécution du binaire shooty")
}

#[test]
fn snapshot_renders_a_non_empty_grid() {
    let out = shooty(&["--snapshot", "--plain", "--size", "40x12", "--time", "1"]);
    assert!(out.status.success(), "statut : {:?}", out.status);
    assert!(
        out.stderr.is_empty(),
        "aucun message attendu : {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("sortie UTF-8");
    assert_eq!(text.lines().count(), 12, "une ligne par rangée");
    let glyphs = text.chars().filter(|c| !c.is_whitespace()).count();
    assert!(glyphs > 20, "image trop pauvre : {glyphs} glyphes");
}

#[test]
fn snapshot_is_deterministic() {
    let args = ["--snapshot", "--plain", "--size", "80x24", "--time", "2"];
    let first = shooty(&args);
    let second = shooty(&args);
    assert!(first.status.success(), "statut : {:?}", first.status);
    assert_eq!(
        first.stdout, second.stdout,
        "deux exécutions de --snapshot doivent produire des octets identiques"
    );
}

#[test]
fn help_exits_with_zero_and_lists_the_motor_options() {
    let out = shooty(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).expect("sortie UTF-8");
    // Le texte d'usage appartient au moteur (`het_app::cli::usage`) ; on vérifie
    // seulement que le nom du programme et les options clés y figurent.
    assert!(text.contains("Usage : shooty"), "{text:?}");
    assert!(text.contains("--snapshot"));
}

#[test]
fn invalid_option_exits_with_two() {
    let out = shooty(&["--unknown"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8(out.stderr).expect("stderr UTF-8");
    assert!(err.contains("Option invalide"), "{err:?}");
    assert!(err.contains("Usage :"), "{err:?}");
}

#[test]
fn live_mode_without_a_terminal_fails_cleanly() {
    let out = shooty(&[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "aucune image ne doit être écrite");
}
