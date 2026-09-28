//! ViewSync (Google Earth) : CSV sans terminateur, un datagramme par tick
//! (`docs/07` §5). Correction : `timeEnd = timeStart + intervalle`, y compris
//! en cadence élevée.

use nmeasim_core::units::{normalize_deg, rad_to_deg};
use nmeasim_sim::state::SimState;
use serde::{Deserialize, Serialize};

/// Secondes entre l'an 0 (calendrier grégorien proleptique) et l'epoch Unix.
pub const EPOCH_OFFSET_S: i64 = 62_167_219_200;

/// Options ViewSync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ViewSyncOptions {
    /// Émission active.
    pub enabled: bool,
    /// Hôte destinataire.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Hauteur de caméra, m.
    pub altitude: f64,
    /// Inclinaison de caméra, degrés.
    pub tilt: f64,
}

impl Default for ViewSyncOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".into(),
            port: 7001,
            altitude: 30.0,
            tilt: 85.0,
        }
    }
}

/// Nombre au format JavaScript (`String(x)`) pour les valeurs usuelles.
pub fn js_number(x: f64) -> String {
    if x == x.trunc() && x.abs() < 1e15 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// Paquet ViewSync.
pub fn packet(counter: u64, s: &SimState, opts: &ViewSyncOptions, interval_ms: u32) -> String {
    let altitude = opts.altitude.max(s.vessel.altitude);
    let start = EPOCH_OFFSET_S + s.sentence_time_ms().div_euclid(1000);
    let end = start as f64 + f64::from(interval_ms) / 1000.0;
    format!(
        "{},{},{},{},{},{},{},{},{},",
        counter,
        js_number(s.vessel.position.lat),
        js_number(s.vessel.position.lon),
        js_number(altitude),
        js_number(normalize_deg(rad_to_deg(s.vessel.heading))),
        js_number(opts.tilt),
        0,
        start,
        js_number(end),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_numbers() {
        assert_eq!(js_number(30.0), "30");
        assert_eq!(js_number(-34.99966098512356), "-34.99966098512356");
        assert_eq!(js_number(19.144223437500003), "19.144223437500003");
        assert_eq!(js_number(63957803212.5), "63957803212.5");
    }
}
