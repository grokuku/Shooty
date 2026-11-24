//! Décodage des entrées — port de `demo.c:keypress` (C-REF) au-dessus d'une table
//! d'actions générique.
//!
//! Le C comparait directement des `int` renvoyés par `t3_terminal_key` (caractères
//! ASCII et codes `T3_KEY_UP..=T3_KEY_LEFT`). Le portage sépare proprement :
//!
//! `touche brute (backend)` -> [`KeyCode`] -> [`InputMap`] -> [`Action`] -> [`apply`].
//!
//! L'intérêt n'est pas cosmétique : c'est le même chemin pour le terminal natif
//! (`astre-term` en mode *pull*) et pour le web (`astre-web` alimenté par
//! `push_key`, §C.4), et les remappages deviennent une simple table.
//!
//! Correspondance des touches, à l'identique du C : `q`/`Q` quitte, `h`/`H`/`?` aide,
//! `p`/`P` moniteur, `Tab` HUD, `m`/`M` ASCII <-> Braille, `Espace` pause,
//! `r`/`R` recadrage, flèches orbite/élévation, `+`/`=` rapproche, `-` éloigne.

use astre_core::Mode;

use crate::orbit::Orbit;
use astre_scene::camera::{AZIMUTH_STEP, DOLLY_STEP, ELEVATION_STEP};

/// Touche décodée, indépendante du backend (terminal ou navigateur).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// Caractère imprimable (ou espace).
    Char(char),
    /// Flèche haut.
    Up,
    /// Flèche bas.
    Down,
    /// Flèche gauche.
    Left,
    /// Flèche droite.
    Right,
    /// Tabulation.
    Tab,
    /// Échap (sans action par défaut, comme dans le C).
    Esc,
}

/// Action de haut niveau déclenchée par une touche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// Quitter proprement (restauration du terminal).
    Quit,
    /// Afficher/masquer l'aide.
    ToggleHelp,
    /// Afficher/masquer le moniteur de performances.
    ToggleMonitor,
    /// Afficher/masquer le HUD.
    ToggleHud,
    /// Basculer ASCII <-> Braille.
    ToggleMode,
    /// Figer/reprendre l'animation.
    TogglePause,
    /// Revenir au cadrage initial et à l'orbite automatique.
    ResetCamera,
    /// Tourner l'azimut vers la gauche.
    OrbitLeft,
    /// Tourner l'azimut vers la droite.
    OrbitRight,
    /// Monter l'élévation.
    ElevationUp,
    /// Descendre l'élévation.
    ElevationDown,
    /// Rapprocher la caméra (`+`).
    ZoomIn,
    /// Éloigner la caméra (`-`).
    ZoomOut,
}

/// Table touche -> action, remappable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputMap {
    bindings: Vec<(KeyCode, Action)>,
}

impl InputMap {
    /// Table par défaut : exactement les touches du C.
    #[must_use]
    pub fn new() -> Self {
        use Action::{
            ElevationDown, ElevationUp, OrbitLeft, OrbitRight, Quit, ResetCamera, ToggleHelp,
            ToggleHud, ToggleMode, ToggleMonitor, TogglePause, ZoomIn, ZoomOut,
        };
        use KeyCode::{Char, Down, Left, Right, Tab, Up};
        Self {
            bindings: vec![
                (Char('q'), Quit),
                (Char('Q'), Quit),
                (Char('h'), ToggleHelp),
                (Char('H'), ToggleHelp),
                (Char('?'), ToggleHelp),
                (Char('p'), ToggleMonitor),
                (Char('P'), ToggleMonitor),
                (Tab, ToggleHud),
                (Char('m'), ToggleMode),
                (Char('M'), ToggleMode),
                (Char(' '), TogglePause),
                (Char('r'), ResetCamera),
                (Char('R'), ResetCamera),
                (Left, OrbitLeft),
                (Right, OrbitRight),
                (Up, ElevationUp),
                (Down, ElevationDown),
                (Char('+'), ZoomIn),
                (Char('='), ZoomIn),
                (Char('-'), ZoomOut),
            ],
        }
    }

    /// Action associée à une touche, s'il y en a une.
    #[must_use]
    pub fn action(&self, key: KeyCode) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, a)| *a)
    }

    /// Rebinde une touche (remplace la liaison existante ou l'ajoute).
    pub fn rebind(&mut self, key: KeyCode, action: Action) {
        match self.bindings.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = action,
            None => self.bindings.push((key, action)),
        }
    }

    /// Toutes les liaisons, dans l'ordre de la table.
    #[must_use]
    pub fn bindings(&self) -> &[(KeyCode, Action)] {
        &self.bindings
    }
}

impl Default for InputMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Action d'une touche avec la table par défaut (raccourci de `InputMap::action`).
#[must_use]
pub fn action_for(key: KeyCode) -> Option<Action> {
    InputMap::new().action(key)
}

/// Applique une action à l'état orbital.
///
/// Retourne `false` quand l'application doit s'arrêter (`q`/`Q`), comme le `keypress`
/// du C qui rendait `false` pour ces deux touches.
pub fn apply(orbit: &mut Orbit, action: Action) -> bool {
    match action {
        Action::Quit => return false,
        Action::ToggleHelp => orbit.help = !orbit.help,
        Action::ToggleMonitor => orbit.monitor = !orbit.monitor,
        Action::ToggleHud => orbit.hud = !orbit.hud,
        Action::ToggleMode => {
            orbit.mode = match orbit.mode {
                Mode::Ascii => Mode::Braille,
                Mode::Braille => Mode::Ascii,
            };
        }
        Action::TogglePause => orbit.paused = !orbit.paused,
        Action::ResetCamera => {
            orbit.reset_camera();
            orbit.paused = false;
        }
        Action::OrbitLeft => orbit.orbit(-AZIMUTH_STEP),
        Action::OrbitRight => orbit.orbit(AZIMUTH_STEP),
        Action::ElevationUp => orbit.elevate(ELEVATION_STEP),
        Action::ElevationDown => orbit.elevate(-ELEVATION_STEP),
        // §E.7 : sur le natif `+`/`-` restent le dolly caméra, comme dans le C.
        Action::ZoomIn => orbit.dolly(-DOLLY_STEP),
        Action::ZoomOut => orbit.dolly(DOLLY_STEP),
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_map_matches_the_c_keys() {
        assert_eq!(action_for(KeyCode::Char('q')), Some(Action::Quit));
        assert_eq!(action_for(KeyCode::Char('Q')), Some(Action::Quit));
        assert_eq!(action_for(KeyCode::Char('?')), Some(Action::ToggleHelp));
        assert_eq!(action_for(KeyCode::Tab), Some(Action::ToggleHud));
        assert_eq!(action_for(KeyCode::Char(' ')), Some(Action::TogglePause));
        assert_eq!(action_for(KeyCode::Esc), None);
        assert_eq!(action_for(KeyCode::Char('z')), None);
    }

    #[test]
    fn arrows_stop_the_automatic_orbit() {
        let mut o = Orbit::default();
        assert!(o.automatic);
        assert!(apply(&mut o, Action::OrbitLeft));
        assert!(!o.automatic);
        assert!(o.azimuth < astre_scene::camera::DEFAULT_AZIMUTH);
    }

    #[test]
    fn plus_and_minus_move_the_radius_like_the_c() {
        let mut o = Orbit::default();
        let r = o.radius;
        apply(&mut o, Action::ZoomIn);
        assert_eq!(o.radius, r - DOLLY_STEP);
        apply(&mut o, Action::ZoomOut);
        assert_eq!(o.radius, r);
    }

    #[test]
    fn quit_is_the_only_action_that_stops() {
        let mut o = Orbit::default();
        for action in [
            Action::ToggleHelp,
            Action::ToggleMonitor,
            Action::ToggleHud,
            Action::ToggleMode,
            Action::TogglePause,
            Action::ResetCamera,
            Action::OrbitLeft,
            Action::OrbitRight,
            Action::ElevationUp,
            Action::ElevationDown,
            Action::ZoomIn,
            Action::ZoomOut,
        ] {
            assert!(apply(&mut o, action), "{action:?} ne doit pas quitter");
        }
        assert!(!apply(&mut o, Action::Quit));
    }

    #[test]
    fn rebinding_replaces_or_adds_a_binding() {
        let mut map = InputMap::new();
        map.rebind(KeyCode::Char('x'), Action::ToggleMode);
        assert_eq!(map.action(KeyCode::Char('x')), Some(Action::ToggleMode));
        map.rebind(KeyCode::Char('x'), Action::Quit);
        assert_eq!(map.action(KeyCode::Char('x')), Some(Action::Quit));
        assert!(map.bindings().len() >= 20);
    }
}
