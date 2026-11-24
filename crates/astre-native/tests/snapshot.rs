//! Tests d'intégration du binaire `astre` — mode headless (`--snapshot`) et CLI.
//!
//! Ces tests vérifient les exigences de §H.5 sans terminal ni TTY :
//!
//! 1. `--snapshot` est **déterministe** (deux exécutions produisent des octets
//!    identiques) et **non trivial** (glyphes imprimables + HUD) ;
//! 2. la CLI renvoie les mêmes codes de sortie que le C (0 pour `--help`, 2 pour une
//!    option invalide, 1 sans terminal interactif) ;
//! 3. `--plain` reste réservé à `--snapshot`.
//!
//! Le mode interactif n'est pas testé ici (pas de TTY en CI, §H.6) : il est couvert par
//! les tests unitaires de `astre-app` (Sim, overlay) et de `astre-term` (encodeur ANSI).

// Les lints du workspace interdisent `expect`/`unwrap` hors tests ; dans un test,
// paniquer sur un échec est précisément le comportement attendu.
#![allow(clippy::expect_used)]

use std::io::Write as _;
use std::process::{Command, Stdio};

/// Lance le binaire `astre` avec les arguments donnés, stdin fermé (jamais de TTY).
fn astre(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_astre"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("exécution du binaire astre")
}

/// Retire les séquences ANSI pour ne garder que les glyphes.
fn strip_ansi(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut chars = bytes.iter().copied().peekable();
    while let Some(b) = chars.next() {
        if b == 0x1b {
            // `ESC [ … lettre` : on saute jusqu'à la lettre finale.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(char::from(b));
    }
    out
}

#[test]
fn snapshot_is_deterministic() {
    let args = ["--snapshot", "--size", "120x40", "--time", "8"];
    let first = astre(&args);
    let second = astre(&args);
    assert!(first.status.success(), "statut : {:?}", first.status);
    assert_eq!(
        first.stdout, second.stdout,
        "deux exécutions de --snapshot doivent produire des octets identiques"
    );
    assert!(!first.stdout.is_empty());
}

#[test]
fn snapshot_plain_has_printable_glyphs_and_the_hud() {
    let out = astre(&["--snapshot", "--plain", "--size", "120x40", "--time", "0"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("sortie UTF-8");
    assert_eq!(text.lines().count(), 40, "une ligne par rangée");
    // Le mode par défaut est Braille : la sortie mêle ASCII et Unicode.
    assert!(!text.is_ascii());

    let glyphs = text.chars().filter(|c| !c.is_whitespace()).count();
    assert!(glyphs > 500, "image trop pauvre : {glyphs} glyphes");

    // HUD : libellés contractuels de `demo.c:overlay` (§G.4).
    assert!(text.contains(" ASTRE "), "badge ASTRE absent");
    assert!(text.contains("RELAIS 07  /  AIRE DE VOL"), "titre absent");
    assert!(text.contains("ORBITE AUTO  /  BRAILLE"), "état absent");
    assert!(text.contains("NX-04"), "barre navette absente");
    assert!(
        text.contains("H  Aide   Q  Quitter"),
        "barre de touches absente"
    );
    assert!(
        text.starts_with(" ASTRE "),
        "le badge doit ouvrir la première ligne"
    );
}

#[test]
fn snapshot_in_braille_emits_unicode_glyphs() {
    let out = astre(&[
        "--snapshot",
        "--plain",
        "--size",
        "60x18",
        "--mode",
        "braille",
    ]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("sortie UTF-8");
    let braille = text
        .chars()
        .filter(|c| ('\u{2800}'..='\u{28FF}').contains(c))
        .count();
    let blocks = text
        .chars()
        .filter(|c| ('\u{2580}'..='\u{259F}').contains(c))
        .count();
    assert!(braille > 50, "glyphes braille insuffisants : {braille}");
    assert!(blocks > 0, "aucun bloc aligné dans le rendu braille");
}

#[test]
fn ascii_mode_stays_printable_ascii() {
    let out = astre(&[
        "--snapshot",
        "--plain",
        "--size",
        "80x24",
        "--mode",
        "ascii",
    ]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("sortie UTF-8");
    assert!(
        text.is_ascii(),
        "le mode ASCII ne doit émettre que de l'ASCII imprimable"
    );
    assert!(
        text.chars().all(|c| !c.is_control() || c == '\n'),
        "aucun caractère de contrôle hors saut de ligne"
    );
}

#[test]
fn truecolor_snapshot_emits_ansi_colour_sequences() {
    let out = astre(&["--snapshot", "--size", "40x12", "--palette", "rgb"]);
    assert!(out.status.success());
    let stripped = strip_ansi(&out.stdout);
    let text = String::from_utf8(out.stdout).expect("sortie UTF-8");
    assert!(text.contains("\x1b[48;2;"), "pas de fond vrai couleur");
    assert!(text.contains("\x1b[38;2;"), "pas d'encre vrai couleur");
    assert!(
        text.contains("\x1b[0m\n"),
        "pas de remise à zéro en fin de ligne"
    );
    assert!(!stripped.trim().is_empty());
}

#[test]
fn palette_256_uses_indexed_sequences() {
    let out = astre(&["--snapshot", "--size", "40x12", "--palette", "256"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("sortie UTF-8");
    assert!(text.contains("\x1b[48;5;"), "pas de fond indexé");
    assert!(!text.contains("\x1b[48;2;"), "mélange des deux palettes");
}

#[test]
fn help_exits_with_zero() {
    let out = astre(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).expect("sortie UTF-8");
    assert!(text.starts_with("ASTRE - demonstration 3D pour terminal"));
    assert!(text.contains("--snapshot"));
    assert!(text.contains("Touches : fleches"));
    assert!(out.stderr.is_empty());
}

#[test]
fn invalid_option_exits_with_two_and_prints_the_c_message() {
    for args in [
        vec!["--unknown"],
        vec!["--fps", "0"],
        vec!["--size", "4097x1"],
        vec!["--cell-aspect", "0.05"],
        vec!["--mode", "blocks"],
        vec!["--time", "-1"],
    ] {
        let out = astre(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let err = String::from_utf8(out.stderr).expect("stderr UTF-8");
        assert!(
            err.starts_with(&format!("Option invalide : {}", args[0])),
            "{args:?} -> {err:?}"
        );
        assert!(err.contains("Usage :"), "usage absent de {err:?}");
    }
}

#[test]
fn plain_without_snapshot_exits_with_two() {
    let out = Command::new(env!("CARGO_BIN_EXE_astre"))
        .arg("--plain")
        .stdin(Stdio::null())
        .output()
        .expect("exécution");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8(out.stderr)
        .expect("stderr")
        .starts_with("--plain s'utilise avec --snapshot."));
}

#[test]
fn live_mode_without_a_terminal_fails_cleanly() {
    let out = Command::new(env!("CARGO_BIN_EXE_astre"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("exécution");
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).expect("stderr UTF-8");
    assert!(
        err.contains("terminal interactif"),
        "message d'absence de TTY attendu, obtenu {err:?}"
    );
    assert!(out.stdout.is_empty(), "aucune image ne doit être écrite");
}

#[test]
fn snapshot_output_is_stable_at_the_maximum_ansi_budget() {
    // 240x72 : taille de référence de la spec (§A.1.2), ~1,1 Mio d'échantillons.
    let args = [
        "--snapshot",
        "--size",
        "240x72",
        "--time",
        "12.25",
        "--mode",
        "braille",
    ];
    let first = astre(&args);
    let second = astre(&args);
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    let mut sink = Vec::new();
    sink.write_all(&first.stdout).expect("écriture mémoire");
    assert!(sink.len() > 20_000, "sortie trop petite : {} o", sink.len());
}
