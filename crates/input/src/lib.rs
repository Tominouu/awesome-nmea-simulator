//! Entrées de nmeasim-rs : manette et clavier traduits en commandes de l'API
//! de contrôle, par des fonctions pures testables sans matériel.

pub mod gamepad;
pub mod keyboard;

pub use gamepad::{Axis, Button, InputMapper, Profile, RawEvent};
