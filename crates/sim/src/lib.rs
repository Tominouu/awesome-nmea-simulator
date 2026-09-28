//! Modèle de domaine et simulation déterministe de nmeasim-rs.
//!
//! - [`state`] : état du monde, en SI ;
//! - [`command`] : commandes de l'API de contrôle, seule voie d'écriture ;
//! - [`sim`] : le [`Simulator`], seul propriétaire de l'état ;
//! - [`compat`] : profils `modern` / `legacy` (D1) ;
//! - [`drift`] : bruit borné (D3) ;
//! - [`config`] : configuration validée.

pub mod command;
pub mod compat;
pub mod config;
pub mod drift;
pub mod sim;
pub mod state;

pub use command::{Command, EngineSel, RejectReason, Source};
pub use compat::{CompatConfig, CompatProfile, Profile};
pub use config::SimConfig;
pub use sim::{SimEvent, Simulator};
pub use state::SimState;
