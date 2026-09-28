//! Clavier (`docs/09` §2). Les touches produisent des commandes, jamais des
//! écritures d'état. Profil legacy : `↑`/`↓` vitesse ±1 kn, `←`/`→` barre ±1°.

use nmeasim_sim::command::{Command, EngineSel};
use serde::{Deserialize, Serialize};

/// Touche reconnue, indépendante de la bibliothèque d'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    /// Flèche haut.
    Up,
    /// Flèche bas.
    Down,
    /// Flèche gauche.
    Left,
    /// Flèche droite.
    Right,
    /// Page précédente.
    PageUp,
    /// Page suivante.
    PageDown,
    /// Début.
    Home,
    /// Espace.
    Space,
    /// Lettre (majuscule).
    Char(char),
}

/// Contexte d'interprétation.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyContext {
    /// Profil legacy (vitesse commandée directement).
    pub legacy: bool,
    /// Majuscule enfoncée : pas ×5.
    pub shift: bool,
}

/// Commande associée à une touche, ou `None`.
pub fn command_for(key: Key, ctx: KeyContext) -> Option<Command> {
    let k = if ctx.shift { 5.0 } else { 1.0 };
    Some(match key {
        Key::Up if ctx.legacy => Command::NudgeSpeed { kn: k },
        Key::Down if ctx.legacy => Command::NudgeSpeed { kn: -k },
        Key::Up => Command::NudgeThrottle {
            engine: EngineSel::All,
            delta: 0.05 * k,
        },
        Key::Down => Command::NudgeThrottle {
            engine: EngineSel::All,
            delta: -0.05 * k,
        },
        Key::Left => Command::NudgeRudder { deg: -k },
        Key::Right => Command::NudgeRudder { deg: k },
        Key::PageUp => Command::NudgeHeading { deg: 10.0 },
        Key::PageDown => Command::NudgeHeading { deg: -10.0 },
        Key::Home => Command::CenterRudder,
        Key::Space => Command::TogglePause,
        Key::Char('A') => Command::ToggleAutopilot,
        Key::Char('E') => Command::ToggleEngines,
        Key::Char('N') => Command::ToggleAnchor,
        Key::Char('M') => Command::MarkPosition,
        Key::Char('W') => Command::NextWaypoint,
        Key::Char('0') => Command::SetThrottle {
            engine: EngineSel::All,
            value: 0.0,
        },
        _ => return None,
    })
}

/// Aide des raccourcis, pour l'interface.
pub const HELP: &[(&str, &str)] = &[
    ("↑ / ↓", "propulsion ±5 % (legacy : vitesse ±1 kn)"),
    ("← / →", "barre ±1° (Maj : ±5°)"),
    ("Pg↑ / Pg↓", "cap du pilote ±10°"),
    ("Début", "barre au centre"),
    ("0", "point mort"),
    ("Espace", "pause / reprise"),
    ("A", "pilote automatique"),
    ("E", "moteurs marche / arrêt"),
    ("N", "mouillage"),
    ("M", "marquer la position"),
    ("W", "point de route suivant"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_and_legacy_arrows() {
        let m = KeyContext::default();
        let l = KeyContext {
            legacy: true,
            shift: false,
        };
        assert_eq!(
            command_for(Key::Up, l),
            Some(Command::NudgeSpeed { kn: 1.0 })
        );
        assert_eq!(
            command_for(Key::Right, l),
            Some(Command::NudgeRudder { deg: 1.0 })
        );
        assert!(matches!(
            command_for(Key::Up, m),
            Some(Command::NudgeThrottle { .. })
        ));
        assert_eq!(
            command_for(Key::Left, KeyContext { shift: true, ..m }),
            Some(Command::NudgeRudder { deg: -5.0 })
        );
        assert_eq!(command_for(Key::Char('Z'), m), None);
        assert_eq!(command_for(Key::Space, m), Some(Command::TogglePause));
    }
}
