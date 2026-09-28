//! Encodeurs de nmeasim-rs : vues pures de l'état, sans aucun accès réseau.
//!
//! - [`sentence`] : phrase NMEA, checksum, formats (`toFixed` JavaScript) ;
//! - [`nmea`] : bloc d'un tick, ordre legacy ou moderne ;
//! - [`ais`] : messages 1 et 5 ;
//! - [`signalk`] : hello et delta ;
//! - [`viewsync`] : paquet CSV.

pub mod ais;
pub mod nmea;
pub mod parse;
pub mod sentence;
pub mod signalk;
pub mod viewsync;
