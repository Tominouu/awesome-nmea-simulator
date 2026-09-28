//! Noyau headless de nmeasim-rs.
//!
//! [`units`] et [`random`] (J0), [`geo`] : géodésie sphérique partagée par le
//! modèle et les encodeurs.
//!
//! Ce crate ne dépend que de `std` : aucune UI, aucun réseau, aucune source
//! globale d'aléa (`docs/decisions/008-runtime-rust.md`).

pub mod geo;
pub mod random;
pub mod units;
