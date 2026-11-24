//! Configuration de démarrage de l'application web — parseur JSON **minimal**.
//!
//! `App::new(config_json)` reçoit une chaîne JSON plate, écrite par `web/src/main.ts` :
//!
//! ```json
//! { "cellPx": 16, "tickHz": 60, "mode": "braille", "hud": true }
//! ```
//!
//! Pourquoi pas `serde_json` : §F.1 impose `wasm-bindgen` comme **seule** dépendance
//! d'`astre-web` (taille du `.wasm`, surface d'audit). Le sous-ensemble accepté est donc
//! volontairement minuscule : un objet plat, valeurs `number` / `string` / `true` /
//! `false`, clés entre guillemets, pas d'imbrication, pas d'échappement (sauf `\"`).
//!
//! **Aucune entrée ne fait échouer le parseur** : une clé inconnue, une valeur d'un
//! autre type ou un nombre illisible sont ignorés. Seul un document qui n'est pas un
//! objet (`[`, `"`, texte libre…) est refusé — c'est un bug d'appelant, pas un cas
//! utilisateur (cf. [`crate::App::new`]).

use astre_core::Mode;

/// Configuration effective d'un démarrage web.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// Largeur de cellule demandée en pixels device (`None` = palier automatique).
    pub cell_px: Option<u32>,
    /// Fréquence du pas fixe de simulation (bornée `10..=240`, §D.1).
    pub tick_hz: Option<u32>,
    /// Mode de résolution des glyphes.
    pub mode: Option<Mode>,
    /// HUD dans les cellules visible (`Tab`).
    pub hud: Option<bool>,
}

/// Parseur : rend la configuration décrite par `json`, ou `None` si le document n'est
/// pas un objet JSON plat.
#[must_use]
pub fn parse(json: &str) -> Option<Config> {
    let text = json.trim();
    if text.is_empty() {
        // `new("")` = configuration par défaut : pratique en test.
        return Some(Config::default());
    }
    if !text.starts_with('{') || !text.ends_with('}') {
        return None;
    }
    let body = &text[1..text.len() - 1];
    let mut cfg = Config::default();
    for member in split_top_level(body) {
        let Some((key, value)) = split_pair(member) else {
            continue;
        };
        let key = unquote(key.trim());
        let value = value.trim();
        match key {
            "cellPx" | "cell_px" => cfg.cell_px = parse_u32(value),
            "tickHz" | "tick_hz" => {
                cfg.tick_hz = parse_u32(value).map(|hz| hz.clamp(10, 240));
            }
            "mode" => {
                cfg.mode = match unquote(value) {
                    "ascii" => Some(Mode::Ascii),
                    "braille" => Some(Mode::Braille),
                    _ => None,
                };
            }
            "hud" => {
                cfg.hud = match value {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                };
            }
            // Clé inconnue : ignorée (extension future sans rupture).
            _ => {}
        }
    }
    Some(cfg)
}

/// Découpe un objet plat sur les virgules de premier niveau (une chaîne entre
/// guillemets n'est jamais coupée).
fn split_top_level(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            ',' if !in_string => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < body.len() {
        parts.push(&body[start..]);
    }
    parts
}

/// Coupe `"clé": valeur` sur le premier `:` hors chaîne.
fn split_pair(member: &str) -> Option<(&str, &str)> {
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in member.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            ':' if !in_string => return Some((&member[..i], &member[i + 1..])),
            _ => {}
        }
    }
    None
}

/// Retire les guillemets d'une valeur chaîne (au plus une paire).
fn unquote(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// Nombre entier non signé ; `None` si la valeur n'est pas un entier décimal simple.
fn parse_u32(value: &str) -> Option<u32> {
    // Un `16.0` (JSON valide) est accepté comme `16`.
    let value = value.trim();
    if let Some((int_part, frac)) = value.split_once('.') {
        if !frac.chars().all(|c| c == '0') || int_part.is_empty() {
            return None;
        }
        return int_part.parse::<u32>().ok();
    }
    value.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    // Exception locale, comme dans les autres crates : le code de test a le droit
    // d'échouer bruyamment (lints `unwrap_used`/`expect_used` du workspace).
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn empty_document_is_the_default() {
        assert_eq!(parse(""), Some(Config::default()));
        assert_eq!(parse("   \n"), Some(Config::default()));
    }

    #[test]
    fn full_document_is_parsed() {
        let cfg = parse(r#"{"cellPx":16,"tickHz":90,"mode":"ascii","hud":false}"#)
            .expect("objet JSON valide");
        assert_eq!(cfg.cell_px, Some(16));
        assert_eq!(cfg.tick_hz, Some(90));
        assert_eq!(cfg.mode, Some(Mode::Ascii));
        assert_eq!(cfg.hud, Some(false));
    }

    #[test]
    fn unknown_keys_and_bad_values_are_ignored() {
        let cfg = parse(r#"{"zoom":3,"cellPx":"8","tickHz":abc,"mode":"crayon"}"#)
            .expect("objet JSON valide");
        assert_eq!(cfg.cell_px, None);
        assert_eq!(cfg.tick_hz, None);
        assert_eq!(cfg.mode, None);
    }

    #[test]
    fn tick_hz_is_clamped() {
        assert_eq!(parse(r#"{"tickHz":1}"#).unwrap().tick_hz, Some(10));
        assert_eq!(parse(r#"{"tickHz":10000}"#).unwrap().tick_hz, Some(240));
    }

    #[test]
    fn non_object_documents_are_rejected() {
        assert!(parse("[1,2]").is_none());
        assert!(parse("16").is_none());
        assert!(parse("\"cellPx\"").is_none());
    }

    #[test]
    fn string_values_containing_colons_survive() {
        let cfg = parse(r#"{"mode":"braille","note":"a:b,c"}"#).expect("valide");
        assert_eq!(cfg.mode, Some(Mode::Braille));
    }
}
