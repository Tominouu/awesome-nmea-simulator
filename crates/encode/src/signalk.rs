//! Signal K : hello et delta (`docs/07` §4). Forme legacy conservée :
//! `updates[0].values` est un tableau `{path, value}`. Corrections : version du
//! hello issue de la version applicative, chemins de destination valides.

use nmeasim_core::units::normalize_rad;
use nmeasim_sim::compat::CompatProfile;
use nmeasim_sim::state::SimState;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::sentence::iso;

/// Options Signal K.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SignalKOptions {
    /// Identifiant du navire.
    pub vessel_id: String,
    /// Contexte.
    pub context: String,
    /// `$source` des deltas.
    pub source: String,
}

impl Default for SignalKOptions {
    fn default() -> Self {
        Self {
            vessel_id: "urn:mrn:signalk:uuid:b7590868-1d62-47d9-989c-32321b349fb9".into(),
            context: "vessels".into(),
            source: "nmea-simulator".into(),
        }
    }
}

/// Hello, envoyé à l'ouverture d'une connexion. `version` = version de
/// l'application (correction du `17.12.05` legacy).
pub fn hello(opts: &SignalKOptions, version: &str, now_ms: i64) -> String {
    json!({
        "name": "nmea-simulator",
        "version": version,
        "self": format!("{}.{}", opts.context, opts.vessel_id),
        "roles": ["master", "main"],
        "timestamp": iso(now_ms),
    })
    .to_string()
}

fn pv(path: impl Into<String>, value: Value) -> Value {
    json!({ "path": path.into(), "value": js(value) })
}

/// Nombres au format JavaScript : un flottant entier est émis sans décimale
/// (`2`, pas `2.0`), comme `JSON.stringify` du legacy.
pub fn js(v: Value) -> Value {
    match v {
        Value::Number(n) => match n.as_f64() {
            Some(x) if n.is_f64() && x.fract() == 0.0 && x.abs() < 9.007_199_254_740_992e15 => {
                json!(x as i64)
            }
            _ => Value::Number(n),
        },
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (k, js(v))).collect()),
        Value::Array(a) => Value::Array(a.into_iter().map(js).collect()),
        other => other,
    }
}

/// Delta d'un tick.
pub fn delta(state: &SimState, compat: CompatProfile, opts: &SignalKOptions) -> String {
    delta_value(state, compat, opts).to_string()
}

/// Delta d'un tick, en valeur JSON.
pub fn delta_value(s: &SimState, compat: CompatProfile, opts: &SignalKOptions) -> Value {
    let v = &s.vessel;
    let mut values = vec![
        pv("navigation.courseOverGroundTrue", json!(v.cog)),
        pv("navigation.headingTrue", json!(v.heading)),
        pv("navigation.speedOverGround", json!(v.sog)),
        pv("navigation.speedThroughWater", json!(v.stw)),
        pv("navigation.gnss.horizontalDilution", json!(s.gnss.hdop)),
        pv("navigation.gnss.positionDilution", json!(s.gnss.pdop)),
        pv("navigation.gnss.satellites", json!(s.gnss.used.len())),
        pv(
            "navigation.gnss.methodQuality",
            json!(if s.gnss.quality == 0 {
                "no GPS"
            } else {
                "GNSS Fix"
            }),
        ),
        pv("navigation.gnss.type", json!("GPS")),
        pv(
            "navigation.position",
            json!({ "longitude": v.position.lon, "latitude": v.position.lat, "altitude": v.altitude }),
        ),
        pv(
            "environment.wind.directionTrue",
            json!(s.wind.true_direction),
        ),
        pv("environment.wind.speedTrue", json!(s.wind.true_speed)),
    ];
    if s.wind.apparent_speed != 0.0 {
        values.push(pv(
            "environment.wind.angleApparent",
            json!(s.wind.apparent_angle),
        ));
        values.push(pv(
            "environment.wind.speedApparent",
            json!(s.wind.apparent_speed),
        ));
    }
    values.push(pv(
        "environment.water.temperature",
        json!(s.water.temperature),
    ));
    values.push(pv(
        "environment.depth.belowTransducer",
        json!(s.water.depth),
    ));
    for e in &s.engines {
        let p = format!("propulsion.{}", e.id);
        values.push(pv(
            format!("{p}.state"),
            json!(if e.running { "started" } else { "stopped" }),
        ));
        values.push(pv(format!("{p}.label"), json!(e.label)));
        values.push(pv(format!("{p}.temperature"), json!(e.temperature)));
        values.push(pv(
            format!("{p}.revolutions"),
            json!(if e.running { e.rpm / 60.0 } else { 0.0 }),
        ));
        if !compat.legacy_sentences {
            values.push(pv(format!("{p}.engineLoad"), json!(e.load)));
        }
    }
    if !compat.legacy_sentences {
        values.push(pv(
            "navigation.headingMagnetic",
            json!(s.heading_magnetic()),
        ));
        values.push(pv(
            "navigation.magneticVariation",
            json!(s.magnetic_variation),
        ));
        values.push(pv("navigation.rateOfTurn", json!(v.rate_of_turn)));
        values.push(pv("navigation.gnss.verticalDilution", json!(s.gnss.vdop)));
        values.push(pv(
            "steering.rudderAngle",
            json!(v.rudder_angle.to_radians()),
        ));
        values.push(pv(
            "environment.current",
            json!({ "setTrue": normalize_rad(s.water.current_set), "drift": s.water.current_drift }),
        ));
        values.push(pv(
            "navigation.anchor.state",
            json!(if v.anchored { "on" } else { "off" }),
        ));
    }
    if let (Some(dest), Some(g)) = (s.route.destination(), s.route.geometry.as_ref()) {
        let p = json!({ "latitude": dest.position.lat, "longitude": dest.position.lon });
        values.push(pv("navigation.destination.commonName", json!(dest.name)));
        values.push(pv("navigation.destination.waypoint", json!(dest.name)));
        values.push(pv("navigation.courseGreatCircle.nextPoint.position", p));
        values.push(pv(
            "navigation.courseGreatCircle.nextPoint.distance",
            json!(g.dtg),
        ));
        values.push(pv(
            "navigation.courseGreatCircle.nextPoint.bearingTrue",
            json!(g.bearing_to_dest),
        ));
        values.push(pv(
            "navigation.courseGreatCircle.nextPoint.velocityMadeGood",
            json!(g.closing_speed),
        ));
        values.push(pv(
            "navigation.courseGreatCircle.crossTrackError",
            json!(g.xte),
        ));
    }
    json!({
        "context": format!("{}.{}", opts.context, opts.vessel_id),
        "updates": [{
            "timestamp": iso(s.sentence_time_ms()),
            "$source": opts.source,
            "values": values,
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_uses_app_version() {
        let h: Value =
            serde_json::from_str(&hello(&SignalKOptions::default(), "2.0.0", 0)).unwrap();
        assert_eq!(h["version"], "2.0.0");
        assert_eq!(
            h["self"],
            "vessels.urn:mrn:signalk:uuid:b7590868-1d62-47d9-989c-32321b349fb9"
        );
        assert_eq!(h["name"], "nmea-simulator");
        assert_eq!(h["timestamp"], "1970-01-01T00:00:00.000Z");
    }
}
