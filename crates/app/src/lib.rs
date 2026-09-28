//! Assemblage headless de nmeasim-rs : configuration persistante et migration
//! legacy, moteur d'exécution (API de contrôle), API HTTP, scénarios, manette.
//!
//! Le cœur fonctionne sans interface : le binaire `nmeasim --headless` et les
//! tests n'utilisent que ce crate.

pub mod config;
pub mod gamepad;
pub mod http;
pub mod legacy;
pub mod runtime;
pub mod scenario;

pub use config::AppConfig;
pub use runtime::{APP_VERSION, AppEvent, Controller, Published, Runtime};
