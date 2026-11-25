//! Analyse de la ligne de commande — port de `demo.c:{Options,number,dimensions,usage,
//! main}` (C-REF).
//!
//! **Pas de `clap`** (§G.6) : 0 dépendance, messages et **codes de sortie identiques au
//! C** (2 pour une option invalide, 0 pour `--help`), et 12 options seulement. Les
//! bornes sont celles du C, à l'identique :
//!
//! | Option | Bornes C |
//! | --- | --- |
//! | `--fps` | `1..=120`, valeur entière obligatoire |
//! | `--cell-aspect` | `0.1..=3` |
//! | `--time` | `0..=86400` |
//! | `--size` | `WxH`, chaque dimension `1..=4096` |
//! | `--mode` | `ascii` \| `braille` |
//! | `--palette` | `rgb` \| `256` |
//!
//! Ajouts propres au portage (absents du C, documentés comme tels) : `--tick-hz`
//! (§D.1, pas fixe) et `--plain` reste réservé à `--snapshot` comme dans le C.

use core::fmt;

use astre_core::{Mode, Palette};

use crate::orbit::Orbit;
use astre_scene::camera::DEFAULT_AZIMUTH;

/// Largeur par défaut de la grille (`Options.cols` du C).
pub const DEFAULT_COLS: i32 = 120;
/// Hauteur par défaut de la grille (`Options.rows` du C).
pub const DEFAULT_ROWS: i32 = 40;
/// Plafond de rafraîchissement par défaut (`Options.fps` du C).
pub const DEFAULT_FPS: u32 = 60;
/// Rapport largeur/hauteur d'une cellule par défaut (`Options.cell_aspect` du C).
pub const DEFAULT_CELL_ASPECT: f32 = 0.5;
/// Fréquence de simulation par défaut (§D.1, même valeur que le C pour la parité de
/// trajectoire).
pub const DEFAULT_TICK_HZ: u32 = 60;

/// Borne basse de `--fps`.
pub const FPS_MIN: u32 = 1;
/// Borne haute de `--fps`.
pub const FPS_MAX: u32 = 120;
/// Borne basse de `--tick-hz` (§D.1).
pub const TICK_HZ_MIN: u32 = 10;
/// Borne haute de `--tick-hz` (§D.1).
pub const TICK_HZ_MAX: u32 = 240;
/// Borne basse de `--cell-aspect`.
pub const CELL_ASPECT_MIN: f32 = 0.1;
/// Borne haute de `--cell-aspect`.
pub const CELL_ASPECT_MAX: f32 = 3.0;
/// Borne haute de `--time`, en secondes.
pub const TIME_MAX: f64 = 86400.0;
/// Borne par dimension de `--size` (`t3_resize` du C).
pub const MAX_DIM: i32 = 4096;

/// Commande à exécuter, telle que la ligne de commande la décrit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Lance la démo (mode interactif ou `--snapshot`).
    Run,
    /// Affiche l'aide et sort avec le code 0.
    Help,
}

/// Options de la démonstration (`Options` + `Orbit` du C, réunies).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// Largeur de la grille en cellules (`--size`, `120` par défaut).
    pub cols: i32,
    /// Hauteur de la grille en cellules (`--size`, `40` par défaut).
    pub rows: i32,
    /// Plafond de rafraîchissement (`--fps`) ; **ce n'est pas** la cadence de simulation.
    pub fps: u32,
    /// Rapport largeur/hauteur d'une cellule (`--cell-aspect`).
    pub cell_aspect: f32,
    /// Rendu unique sur `stdout` puis sortie (`--snapshot`).
    pub snapshot: bool,
    /// Temps de scène initial (`--time`), en secondes.
    pub start: f64,
    /// Palette de sortie (`--palette`, `--plain`).
    pub palette: Palette,
    /// Mode de résolution **initial** (`--mode`).
    ///
    /// Ce n'est qu'une graine : [`Options::initial_orbit`] recopie la valeur dans
    /// [`crate::orbit::Orbit::mode`], que la touche `M` fait ensuite varier. Le rendu lit
    /// l'état runtime, jamais ces options (sinon `M` n'aurait aucun effet, cf. §D.4).
    pub mode: Mode,
    /// HUD affiché (`--no-hud` le masque).
    pub hud: bool,
    /// Fréquence du pas fixe de simulation (`--tick-hz`), propre au portage.
    pub tick_hz: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            fps: DEFAULT_FPS,
            cell_aspect: DEFAULT_CELL_ASPECT,
            snapshot: false,
            start: 0.0,
            palette: Palette::Rgb,
            mode: Mode::Braille,
            hud: true,
            tick_hz: DEFAULT_TICK_HZ,
        }
    }
}

impl Options {
    /// Options d'une exécution `--snapshot` des dimensions et du temps donnés.
    #[must_use]
    pub fn snapshot(cols: i32, rows: i32, start: f64) -> Self {
        Self {
            cols,
            rows,
            snapshot: true,
            start,
            ..Self::default()
        }
    }

    /// État orbital initial correspondant aux options (`demo.c:main`) :
    /// `seconds = start` et `azimuth += fmod(start*0.095, 2*PI)`.
    ///
    /// Le `fmod` est calculé en `f64` avec un diviseur `f64` converti depuis le `2*PI`
    /// **`f32`** du C, puis tronqué en `f32` — c'est exactement ce que fait
    /// `orbit.azimuth += fmod(opts.start*0.095, 2*PI)` dans l'original.
    #[must_use]
    pub fn initial_orbit(&self) -> Orbit {
        let mut orbit = Orbit::new(self.mode, self.hud);
        orbit.seconds = self.start;
        orbit.azimuth += orbit_phase(self.start);
        orbit
    }
}

/// `fmod(start * 0.095, 2*PI)` du C, en `f32` (voir [`Options::initial_orbit`]).
#[must_use]
pub fn orbit_phase(start: f64) -> f32 {
    let two_pi = f64::from(astre_scene::TWO_PI);
    (start * 0.095).rem_euclid(two_pi) as f32
}

/// Erreur de ligne de commande : message destiné à `stderr`, code de sortie 2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliError {
    /// Message affiché sur `stderr` (identique au C quand l'option existe là-bas).
    pub message: String,
}

impl CliError {
    fn invalid(arg: &str) -> Self {
        Self {
            message: format!("Option invalide : {arg}"),
        }
    }

    fn plain_without_snapshot() -> Self {
        Self {
            message: "--plain s'utilise avec --snapshot.".to_string(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Nom de la commande affiché dans l'aide (parité `ASTRE_COMMAND` du C).
#[cfg(windows)]
pub const COMMAND: &str = ".\\astre.exe";
/// Nom de la commande affiché dans l'aide.
#[cfg(not(windows))]
pub const COMMAND: &str = "./astre";

/// Texte d'aide (`demo.c:usage`), complété de `--tick-hz` propre au portage.
pub const USAGE: &str = concat!(
    "ASTRE - demonstration 3D pour terminal\n",
    "Usage : ./astre [--mode ascii|braille] [--palette rgb|256] [--fps 1..120]\n",
    "                [--cell-aspect largeur/hauteur] [--no-hud] [--tick-hz 10..240]\n",
    "         ./astre --snapshot [--size 120x40] [--time secondes] [--plain]\n",
    "Touches : fleches, +/-, Espace, M, P (performances), Tab, R, H, Q.\n",
);

/// Résultat de l'analyse : options à exécuter ou aide à afficher.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed {
    /// Options retenues.
    pub options: Options,
    /// Commande à exécuter.
    pub command: Command,
}

/// Analyse `args` (**sans** `argv[0]`) selon les règles du C.
///
/// # Errors
///
/// Renvoie un [`CliError`] dont le message est celui du C (`Option invalide : …`) ou
/// le message spécifique de `--plain` sans `--snapshot`.
pub fn parse<I>(args: I) -> Result<Parsed, CliError>
where
    I: IntoIterator<Item = String>,
{
    let args: Vec<String> = args.into_iter().collect();
    let mut options = Options::default();
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index].as_str();
        let value = args.get(index + 1).map(String::as_str).unwrap_or("");
        match arg {
            "--help" | "-h" => {
                return Ok(Parsed {
                    options,
                    command: Command::Help,
                })
            }
            "--snapshot" => options.snapshot = true,
            "--plain" => options.palette = Palette::Plain,
            "--no-hud" => options.hud = false,
            "--mode" => {
                options.mode = match value {
                    "ascii" => Mode::Ascii,
                    "braille" => Mode::Braille,
                    _ => return Err(CliError::invalid(arg)),
                };
                index += 1;
            }
            "--palette" => {
                options.palette = match value {
                    "rgb" => Palette::Rgb,
                    "256" => Palette::Ansi256,
                    _ => return Err(CliError::invalid(arg)),
                };
                index += 1;
            }
            "--fps" => {
                match number(value, f64::from(FPS_MIN), f64::from(FPS_MAX)) {
                    Some(n) if n.fract() == 0.0 => options.fps = n as u32,
                    _ => return Err(CliError::invalid(arg)),
                }
                index += 1;
            }
            "--tick-hz" => {
                match number(value, f64::from(TICK_HZ_MIN), f64::from(TICK_HZ_MAX)) {
                    Some(n) if n.fract() == 0.0 => options.tick_hz = n as u32,
                    _ => return Err(CliError::invalid(arg)),
                }
                index += 1;
            }
            "--cell-aspect" => {
                match number(
                    value,
                    f64::from(CELL_ASPECT_MIN),
                    f64::from(CELL_ASPECT_MAX),
                ) {
                    Some(n) => options.cell_aspect = n as f32,
                    None => return Err(CliError::invalid(arg)),
                }
                index += 1;
            }
            "--time" => {
                match number(value, 0.0, TIME_MAX) {
                    Some(n) => options.start = n,
                    None => return Err(CliError::invalid(arg)),
                }
                index += 1;
            }
            "--size" => {
                let Some((cols, rows)) = dimensions(value) else {
                    return Err(CliError::invalid(arg));
                };
                options.cols = cols;
                options.rows = rows;
                index += 1;
            }
            _ => return Err(CliError::invalid(arg)),
        }
        index += 1;
    }
    if options.palette == Palette::Plain && !options.snapshot {
        return Err(CliError::plain_without_snapshot());
    }
    Ok(Parsed {
        options,
        command: Command::Run,
    })
}

/// `number` du C : analyse complète, finie, dans `[low, high]`.
#[must_use]
pub fn number(s: &str, low: f64, high: f64) -> Option<f64> {
    let v: f64 = s.parse().ok()?;
    (v.is_finite() && v >= low && v <= high).then_some(v)
}

/// `dimensions` du C : `"WxH"`, chaque dimension dans `1..=4096`.
#[must_use]
pub fn dimensions(s: &str) -> Option<(i32, i32)> {
    let (w, rest) = s.split_once('x')?;
    let h = rest;
    let width: i64 = w.parse().ok()?;
    let height: i64 = h.parse().ok()?;
    if !(1..=i64::from(MAX_DIM)).contains(&width) || !(1..=i64::from(MAX_DIM)).contains(&height) {
        return None;
    }
    Some((width as i32, height as i32))
}

/// Azimut initial du C, exposé pour les tests (`DEFAULT_AZIMUTH + fmod(t*0.095, 2π)`).
#[must_use]
pub fn initial_azimuth(start: f64) -> f32 {
    DEFAULT_AZIMUTH + orbit_phase(start)
}

#[cfg(test)]
mod tests {
    // Voir astre-scene : exception locale et justifiée pour le code de test.
    #![allow(clippy::expect_used)]

    use super::*;

    fn parse_args(args: &[&str]) -> Result<Parsed, CliError> {
        parse(args.iter().map(|s| (*s).to_string()))
    }

    #[test]
    fn defaults_match_the_c() {
        let p = parse_args(&[]).expect("options par défaut");
        assert_eq!(p.command, Command::Run);
        assert_eq!(p.options.cols, 120);
        assert_eq!(p.options.rows, 40);
        assert_eq!(p.options.fps, 60);
        assert_eq!(p.options.cell_aspect, 0.5);
        assert_eq!(p.options.palette, Palette::Rgb);
        assert_eq!(p.options.mode, Mode::Braille);
        assert!(p.options.hud);
        assert!(!p.options.snapshot);
        assert_eq!(p.options.tick_hz, 60);
    }

    #[test]
    fn help_exits_as_a_command() {
        let p = parse_args(&["--help"]).expect("aide");
        assert_eq!(p.command, Command::Help);
        let p = parse_args(&["-h"]).expect("aide courte");
        assert_eq!(p.command, Command::Help);
    }

    #[test]
    fn snapshot_set_is_parsed() {
        let p = parse_args(&[
            "--snapshot",
            "--size",
            "23x9",
            "--time",
            "8",
            "--mode",
            "ascii",
            "--palette",
            "256",
            "--plain",
            "--no-hud",
        ])
        .expect("snapshot complet");
        assert!(p.options.snapshot);
        assert_eq!((p.options.cols, p.options.rows), (23, 9));
        assert_eq!(p.options.start, 8.0);
        assert_eq!(p.options.mode, Mode::Ascii);
        assert_eq!(p.options.palette, Palette::Plain);
        assert!(!p.options.hud);
    }

    #[test]
    fn invalid_options_are_rejected_with_the_c_message() {
        for args in [
            vec!["--unknown"],
            vec!["--fps", "0"],
            vec!["--fps", "121"],
            vec!["--fps", "1.5"],
            vec!["--size", "0x9"],
            vec!["--size", "4097x1"],
            vec!["--size", "120"],
            vec!["--cell-aspect", "0.05"],
            vec!["--time", "-1"],
            vec!["--mode", "blocks"],
            vec!["--palette", "16"],
        ] {
            let err = parse_args(&args).expect_err("option invalide attendue");
            assert_eq!(err.message, format!("Option invalide : {}", args[0]));
        }
    }

    #[test]
    fn valued_options_consume_their_argument() {
        // Un `index += 1` oublié ferait lire la valeur comme une option : régression
        // silencieuse que ce test couvre pour chaque option à valeur.
        let p = parse_args(&[
            "--fps",
            "30",
            "--tick-hz",
            "120",
            "--cell-aspect",
            "0.45",
            "--time",
            "3.5",
            "--size",
            "80x24",
            "--mode",
            "ascii",
            "--palette",
            "256",
        ])
        .expect("options à valeur");
        assert_eq!(p.options.fps, 30);
        assert_eq!(p.options.tick_hz, 120);
        assert_eq!(p.options.cell_aspect, 0.45_f32);
        assert_eq!(p.options.start, 3.5);
        assert_eq!((p.options.cols, p.options.rows), (80, 24));
        assert_eq!(p.options.mode, Mode::Ascii);
        assert_eq!(p.options.palette, Palette::Ansi256);
    }

    #[test]
    fn plain_requires_snapshot() {
        let err = parse_args(&["--plain"]).expect_err("--plain seul refusé");
        assert_eq!(err.message, "--plain s'utilise avec --snapshot.");
        assert!(parse_args(&["--plain", "--snapshot"]).is_ok());
    }

    #[test]
    fn numbers_and_dimensions_follow_the_c_bounds() {
        assert_eq!(number("0.5", 0.1, 3.0), Some(0.5));
        assert_eq!(number("inf", 0.0, 1.0), None);
        assert_eq!(number("nan", 0.0, 1.0), None);
        assert_eq!(number("", 0.0, 1.0), None);
        assert_eq!(dimensions("120x40"), Some((120, 40)));
        assert_eq!(dimensions("4096x4096"), Some((4096, 4096)));
        assert_eq!(dimensions("4097x1"), None);
        assert_eq!(dimensions("1x4097"), None);
    }

    #[test]
    fn initial_azimuth_matches_the_c_formula() {
        // `1.01 + fmod(8*0.095, 2π)` = 1.77 en f32 (valeur relevée dans le C).
        let a = initial_azimuth(8.0);
        assert!((a - 1.77).abs() < 1e-6, "azimut initial = {a}");
        assert_eq!(initial_azimuth(0.0), DEFAULT_AZIMUTH);
    }
}
