//! HUD — port fidèle de `demo.c:overlay` (C-REF, contrat visuel §G.4).
//!
//! Les **chaînes sont un contrat** : mêmes libellés (sans accents, le C n'autorisait que
//! l'ASCII imprimable), mêmes paliers de largeur (`w >= 32`, `46`, `63`, `86`,
//! `h >= 20`, `h >= 3`), mêmes positions d'ancrage, même inversion du badge ` ASTRE `
//! (`fg = panel`, `bg = mint` dans le C — ce n'est pas une coquille).
//!
//! Les formats `printf` sont reproduits à l'identique : `%03d`, `%04.1f`, `%6.1f`,
//! `%6.2f`, `%-8s` et `%.*s` ont chacun leur équivalent Rust exact, sur des valeurs
//! `f64` (le C passait des `double` à `printf`, pas des `float`).

use astre_core::hud::{fill, text, Anchor};
use astre_core::{CellBuffer, Mode};

use crate::orbit::Orbit;

/// Fond des panneaux (`panel` = `C(20,32,46)`).
const PANEL: u32 = 0x0014_202E;
/// Fond de la barre d'état (`bar` = `C(28,43,56)`).
const BAR: u32 = 0x001C_2B38;
/// Accent menthe (`mint` = `C(125,228,206)`).
const MINT: u32 = 0x007D_E4CE;
/// Texte principal (`white` = `C(236,234,217)`).
const WHITE: u32 = 0x00EC_EAD9;
/// Texte secondaire (`muted` = `C(177,192,196)`).
const MUTED: u32 = 0x00B1_C0C4;
/// Accent ambre (`amber` = `C(255,184,106)`).
const AMBER: u32 = 0x00FF_B86A;

/// Lignes de l'écran d'aide (`lines` du C), dans l'ordre.
const HELP_LINES: [&str; 10] = [
    "PILOTAGE",
    "Fleches   Tourner autour de la navette",
    "+ / -     Rapprocher / eloigner",
    "Espace    Pause / reprise",
    "M         ASCII / Braille",
    "Tab       Afficher / masquer le HUD",
    "P         Moniteur de performances",
    "R         Reprendre l'orbite auto",
    "Q         Quitter",
    "H         Fermer l'aide",
];

/// Libellés du moniteur (`labels` du C).
const MONITOR_LABELS: [&str; 2] = ["RENDU", "TERMINAL"];

/// Dessine le HUD et les panneaux (`overlay` du C).
///
/// `eye_y` est l'ordonnée de l'œil de la caméra rendue (l'ALT du C).
pub fn overlay(buf: &mut CellBuffer, orbit: &Orbit, eye_y: f32) {
    let w = buf.cols();
    let h = buf.rows();
    let mode = match orbit.mode {
        Mode::Ascii => "ASCII",
        Mode::Braille => "BRAILLE",
    };

    if orbit.hud {
        let state = if orbit.paused {
            "PAUSE"
        } else if orbit.automatic {
            "ORBITE AUTO"
        } else {
            "ORBITE MANUELLE"
        };

        // Barre de titre.
        fill(buf, 0, 0, w, 1, PANEL);
        text(buf, Anchor::TopLeft, 0, 0, " ASTRE ", PANEL, MINT);
        if w >= 32 {
            let title = if w >= 63 {
                "RELAIS 07  /  AIRE DE VOL"
            } else {
                "RELAIS 07"
            };
            text(buf, Anchor::TopLeft, 8, 0, title, WHITE, PANEL);
        }
        if w >= 46 {
            let status = format!("{state}  /  {mode}");
            text(buf, Anchor::TopRight, 1, 0, &status, MINT, PANEL);
        } else if w >= 15 {
            text(buf, Anchor::TopRight, 1, 0, mode, MINT, PANEL);
        }

        // Barre de touches.
        if h >= 3 {
            fill(buf, 0, h - 1, w, 1, PANEL);
            let keys = if w >= 86 {
                "Fleches  Orbite   +/-  Zoom   Espace  Pause   M  Rendu   P  Perf"
            } else if w >= 63 {
                "Fleches  Orbite   +/-  Zoom   M  Rendu"
            } else {
                "M Rendu"
            };
            if w >= 21 {
                text(buf, Anchor::BottomLeft, 1, 0, keys, MUTED, PANEL);
            }
            let right = if w >= 41 {
                "H  Aide   Q  Quitter"
            } else {
                "H ?  Q Fin"
            };
            text(buf, Anchor::BottomRight, 1, 0, right, WHITE, PANEL);
        }

        // Barre d'état de la navette.
        if w >= 48 && h >= 20 {
            fill(buf, 0, h - 2, w, 1, BAR);
            text(buf, Anchor::BottomLeft, 1, 1, "NX-04", AMBER, BAR);
            if w >= 72 {
                text(buf, Anchor::BottomLeft, 8, 1, "NAVETTE D'ESSAI", WHITE, BAR);
            }
            let status = format!(
                "AZ {:03}   ALT {:04.1} m   DIST {:04.1} m",
                orbit.bearing_degrees(),
                f64::from(eye_y),
                orbit.radius
            );
            text(buf, Anchor::BottomRight, 1, 1, &status, WHITE, BAR);
        }
    }

    if orbit.monitor && !orbit.help && h >= 3 {
        let width = w.min(23);
        let rows = if h >= 7 && w >= 19 { 3 } else { 1 };
        let values = [orbit.render_ms, orbit.present_ms];
        fill(buf, w - width, 1, width, rows, PANEL);
        let fps = if orbit.fps > 0.0 {
            format!("FPS {:6.1}", orbit.fps)
        } else {
            "FPS    ...".to_string()
        };
        text(buf, Anchor::TopLeft, w - width, 1, &fps, MINT, PANEL);
        for i in 0..(rows.saturating_sub(1) as usize) {
            let line = if orbit.fps > 0.0 {
                format!("{:<8} {:6.2} ms", MONITOR_LABELS[i], values[i])
            } else {
                format!("{:<8}    ... ms", MONITOR_LABELS[i])
            };
            text(
                buf,
                Anchor::TopLeft,
                w - width,
                2 + i as i32,
                &line,
                WHITE,
                PANEL,
            );
        }
    }

    if orbit.help {
        let width = w.min(44);
        let height = h.min(12);
        let x = (w - width) / 2;
        let y = (h - height) / 2;
        fill(buf, x, y, width, height, PANEL);
        let inset = if width > 5 { 2 } else { 0 };
        let limit = (width - inset).max(0) as usize;
        for (i, line) in HELP_LINES.iter().enumerate() {
            if i >= height as usize {
                break;
            }
            let truncated: String = line.chars().take(limit).collect();
            let fg = if i == 0 { MINT } else { WHITE };
            let row = y + i as i32 + i32::from(height == 12);
            text(buf, Anchor::TopLeft, x + inset, row, &truncated, fg, PANEL);
        }
    }
}

#[cfg(test)]
mod tests {
    // Voir astre-scene : exception locale et justifiée pour le code de test.
    #![allow(clippy::expect_used)]

    use super::*;

    fn row(buf: &CellBuffer, y: i32) -> String {
        buf.row(y)
            .map(|cells| {
                cells
                    .iter()
                    .map(|c| char::from_u32(c.glyph).unwrap_or('?'))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn buffer(cols: i32, rows: i32) -> CellBuffer {
        CellBuffer::new(cols, rows).expect("grille de test")
    }

    #[test]
    fn title_bar_matches_the_c_at_120x40() {
        let mut buf = buffer(120, 40);
        let o = Orbit::default();
        overlay(&mut buf, &o, o.camera(0.5).eye.y);
        let top = row(&buf, 0);
        assert!(
            top.starts_with(" ASTRE  RELAIS 07  /  AIRE DE VOL"),
            "{top:?}"
        );
        assert!(top.ends_with("ORBITE AUTO  /  BRAILLE "), "{top:?}");
        let bottom = row(&buf, 39);
        assert!(
            bottom.trim_end().ends_with("H  Aide   Q  Quitter"),
            "{bottom:?}"
        );
        let status = row(&buf, 38);
        assert!(status.contains("NX-04"), "{status:?}");
        assert!(
            status.contains("AZ 058"),
            "cap du C à l'azimut 1.01 : {status:?}"
        );
        assert!(status.contains("ALT 05.7 m"), "{status:?}");
        assert!(status.contains("DIST 14.2 m"), "{status:?}");
    }

    #[test]
    fn narrow_terminals_fall_back_to_the_short_labels() {
        // Palier `w >= 32` sans `w >= 46` : titre court, pas de barre de navette.
        let mut buf = buffer(40, 12);
        let o = Orbit::default();
        overlay(&mut buf, &o, 5.75);
        let top = row(&buf, 0);
        assert!(top.contains("RELAIS 07"), "{top:?}");
        assert!(!top.contains("AIRE DE VOL"), "{top:?}");
        assert!(top.trim_end().ends_with("BRAILLE"), "{top:?}");

        // Palier minimal : ni titre, ni barre d'état, libellés courts.
        let mut buf = buffer(23, 9);
        overlay(&mut buf, &o, 5.75);
        let top = row(&buf, 0);
        assert!(top.starts_with(" ASTRE "), "{top:?}");
        assert!(!top.contains("RELAIS"), "{top:?}");
        assert!(top.ends_with("BRAILLE "), "{top:?}");
        let bottom = row(&buf, 8);
        assert!(bottom.contains("M Rendu"), "{bottom:?}");
        assert!(bottom.trim_end().ends_with("H ?  Q Fin"), "{bottom:?}");
        // `h < 20` : pas de barre d'état navette sur l'avant-dernière ligne.
        assert!(!row(&buf, 7).contains("NX-04"), "{:?}", row(&buf, 7));
    }

    #[test]
    fn monitor_and_help_panels_are_drawn() {
        let mut buf = buffer(80, 24);
        let mut o = Orbit {
            monitor: true,
            fps: 61.25,
            render_ms: 4.1,
            present_ms: 0.09,
            ..Orbit::default()
        };
        overlay(&mut buf, &o, 5.75);
        let line = row(&buf, 1);
        assert!(line.contains("FPS   61.2"), "{line:?}");
        assert!(
            row(&buf, 2).contains("RENDU      4.10 ms"),
            "{:?}",
            row(&buf, 2)
        );
        assert!(
            row(&buf, 3).contains("TERMINAL   0.09 ms"),
            "{:?}",
            row(&buf, 3)
        );

        o.help = true;
        // Nouvelle grille : le C repart d'une image fraîchement résolue à chaque frame.
        let mut buf = buffer(80, 24);
        overlay(&mut buf, &o, 5.75);
        let all: String = (0..24).map(|y| row(&buf, y)).collect::<Vec<_>>().join("\n");
        assert!(all.contains("PILOTAGE"), "{all}");
        assert!(all.contains("Fermer l'aide"), "{all}");
        // L'aide masque le moniteur (condition `monitor && !help` du C).
        assert!(!all.contains("FPS"), "{all}");
    }

    #[test]
    fn hud_can_be_hidden() {
        let mut buf = buffer(60, 18);
        let o = Orbit {
            hud: false,
            ..Orbit::default()
        };
        overlay(&mut buf, &o, 5.75);
        let all: String = (0..18).map(|y| row(&buf, y)).collect::<Vec<_>>().join("");
        assert!(all.trim().is_empty(), "{all:?}");
    }
}
