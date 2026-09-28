//! Migration de la configuration legacy `nmeasim_config` (`docs/02` §2) vers
//! le schéma 2. Aucune clé n'est perdue : les clés non migrées sont
//! conservées sous `extra.legacy`.

use nmeasim_core::geo::LatLon;
use nmeasim_sim::config::DriftSpec;
use nmeasim_transport::{Parity, TransportSpec};
use serde_json::{Map, Value};

use crate::config::{AppConfig, ConfigError, OutputFormat, TransportConfig};

/// Types numériques legacy (`mods_b/8138.js`).
pub const LEGACY_TYPES: [&str; 7] = [
    "websocket-server",
    "tcp-server",
    "tcp-client",
    "serial",
    "udp-broadcast",
    "udp-client",
    "udp-multicast",
];

fn num(v: &Value, path: &[&str]) -> Option<f64> {
    path.iter().try_fold(v, |acc, k| acc.get(k))?.as_f64()
}

fn st<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter().try_fold(v, |acc, k| acc.get(k))?.as_str()
}

fn seed_spec(v: &Value, obj: &str, field: &str, into: &mut DriftSpec) -> Option<f64> {
    let s = v.get("seed")?.get(obj)?.get(field)?;
    if let Some(x) = s.get("min").and_then(Value::as_f64) {
        into.min = x;
    }
    if let Some(x) = s.get("max").and_then(Value::as_f64) {
        into.max = x;
    }
    if let Some(x) = s.get("plus").and_then(Value::as_f64) {
        into.plus = x;
    }
    if let Some(x) = s.get("minus").and_then(Value::as_f64) {
        into.minus = x;
    }
    s.get("value").and_then(Value::as_f64)
}

/// Résout `server.type` : nombre 0..6 ou nom canonique. Une chaîne legacy
/// (`"udp"`) est une erreur explicite, là où le legacy ne démarrait rien.
pub fn resolve_type(v: &Value) -> Result<&'static str, ConfigError> {
    match v {
        Value::Number(n) => n
            .as_u64()
            .and_then(|i| LEGACY_TYPES.get(i as usize).copied())
            .ok_or_else(|| {
                ConfigError::new(
                    "server.type",
                    format!("valeur {n} hors plage 0..6 ({})", LEGACY_TYPES.join(", ")),
                )
            }),
        Value::String(s) => LEGACY_TYPES
            .iter()
            .find(|t| **t == s.as_str())
            .copied()
            .ok_or_else(|| {
                ConfigError::new(
                    "server.type",
                    format!(
                        "type `{s}` inconnu ; acceptés : 0..6 ou {}",
                        LEGACY_TYPES.join(", ")
                    ),
                )
            }),
        _ => Err(ConfigError::new("server.type", "nombre ou chaîne attendu")),
    }
}

/// Migre un objet de configuration legacy.
pub fn migrate(v: &Value) -> Result<(AppConfig, Vec<String>), ConfigError> {
    let mut c = AppConfig::default();
    let mut warnings = vec!["configuration legacy 1.6.1 migrée vers le schéma 2".to_string()];
    let s = &mut c.simulation;

    if let Some(x) = num(v, &["interval"]) {
        c.output.interval_ms = x as u32;
    }
    if let Some(b) = v.get("autoStart").and_then(Value::as_bool) {
        c.auto_start = b;
    }
    if let Some(b) = v.get("sendNmeaPrefix").and_then(Value::as_bool) {
        c.output.nmea.prefix = b;
    }
    if let Some(id) = st(v, &["vesselId"]) {
        c.output.signalk.vessel_id = id.into();
    }
    if let Some(ctx) = st(v, &["context"]) {
        c.output.signalk.context = ctx.into();
    }
    if let Some(vs) = v.get("viewsync") {
        c.viewsync.enabled = true;
        if let Some(h) = vs.get("host").or(vs.get("address")).and_then(Value::as_str) {
            c.viewsync.host = h.into();
        }
        if let Some(p) = vs.get("port").and_then(Value::as_u64) {
            c.viewsync.port = p as u16;
        }
        if let Some(a) = vs.get("altitude").and_then(Value::as_f64) {
            c.viewsync.altitude = a;
        }
        if let Some(t) = vs.get("tilt").and_then(Value::as_f64) {
            c.viewsync.tilt = t;
        }
    }
    // Talkers : tableau d'entrées [[formateur, talker], ...].
    if let Some(list) = v.get("talkers").and_then(Value::as_array) {
        for e in list {
            if let (Some(k), Some(t)) = (
                e.get(0).and_then(Value::as_str),
                e.get(1).and_then(Value::as_str),
            ) {
                let key = if k == "AIVDM" || k == "AIVDO" {
                    continue;
                } else {
                    k.to_string()
                };
                c.output
                    .nmea
                    .talkers
                    .insert(key, t.chars().take(2).collect::<String>().to_uppercase());
            }
        }
    }
    // Seeds.
    let d = &mut s.drift;
    if let Some(x) = seed_spec(v, "gps", "speed", &mut d.speed) {
        s.initial.speed_kn = x;
    }
    if let Some(x) = seed_spec(v, "gps", "heading", &mut d.heading) {
        s.initial.heading_deg = x;
    }
    if let Some(x) = seed_spec(v, "gps", "hdop", &mut d.hdop) {
        s.gnss.hdop = x;
    }
    if let Some(x) = seed_spec(v, "gps", "vdop", &mut d.vdop) {
        s.gnss.vdop = x;
    }
    if let Some(x) = seed_spec(v, "gps", "pdop", &mut d.pdop) {
        s.gnss.pdop = x;
    }
    if let (Some(lat), Some(lon)) = (
        num(v, &["seed", "gps", "position", "value", "latitude"]),
        num(v, &["seed", "gps", "position", "value", "longitude"]),
    ) {
        s.initial.position = LatLon::new(lat, lon);
    }
    if let Some(a) = num(v, &["seed", "gps", "altitude", "value"]) {
        s.initial.altitude_m = a;
    }
    if let Some(x) = seed_spec(v, "wind", "direction", &mut d.wind_direction) {
        s.environment.wind_direction_deg = x;
    }
    if let Some(x) = seed_spec(v, "wind", "speed", &mut d.wind_speed) {
        s.environment.wind_speed_kn = x;
    }
    if let Some(x) = seed_spec(v, "water", "depth", &mut d.depth) {
        s.environment.depth_m = x;
    }
    if let Some(x) = seed_spec(v, "water", "temperature", &mut d.water_temperature) {
        s.environment.water_temperature_c = x;
    }
    let _ = seed_spec(v, "p0", "rpm", &mut d.rpm);
    let _ = seed_spec(v, "p0", "temperature", &mut d.engine_temperature);

    // Serveur → un transport.
    if let Some(server) = v.get("server") {
        let kind = resolve_type(server.get("type").unwrap_or(&Value::Null))?;
        let ip = server.get("ip").cloned().unwrap_or(Value::Null);
        let address = st(&ip, &["address"]).unwrap_or("127.0.0.1").to_string();
        let port = num(&ip, &["port"]).unwrap_or(3100.0) as u16;
        let spec = match kind {
            "websocket-server" => TransportSpec::WebsocketServer {
                bind: "0.0.0.0".into(),
                port,
            },
            "tcp-server" => TransportSpec::TcpServer {
                bind: "0.0.0.0".into(),
                port,
            },
            "tcp-client" => TransportSpec::TcpClient {
                host: address,
                port,
            },
            "udp-broadcast" => TransportSpec::UdpBroadcast {
                interface: st(&ip, &["interfaceId"]).unwrap_or("").into(),
                port,
            },
            "udp-client" => TransportSpec::UdpClient {
                host: address,
                port,
            },
            "udp-multicast" => TransportSpec::UdpMulticast {
                group: st(&ip, &["multicastAddress"])
                    .unwrap_or("239.255.0.0")
                    .into(),
                port,
                interface: String::new(),
                ttl: 128,
                loopback: true,
            },
            _ => {
                let ser = server.get("serial").cloned().unwrap_or(Value::Null);
                TransportSpec::Serial {
                    port: st(&ser, &["port"]).unwrap_or("").into(),
                    baud_rate: num(&ser, &["baudRate"]).unwrap_or(4800.0) as u32,
                    data_bits: num(&ser, &["dataBits"]).unwrap_or(8.0) as u8,
                    stop_bits: num(&ser, &["stopBits"]).unwrap_or(1.0) as u8,
                    parity: match st(&ser, &["parity"]) {
                        Some("even") => Parity::Even,
                        Some("odd") => Parity::Odd,
                        _ => Parity::None,
                    },
                    flow_control: Default::default(),
                }
            }
        };
        let format = if st(v, &["outputFormat"]) == Some("SIGNALK") {
            OutputFormat::Signalk
        } else {
            OutputFormat::Nmea
        };
        c.transports = vec![TransportConfig {
            id: "legacy".into(),
            enabled: true,
            format,
            spec,
        }];
    }

    // Conservation des clés non migrées.
    let known = [
        "version",
        "interval",
        "outputFormat",
        "autoStart",
        "sendNmeaPrefix",
        "viewsync",
        "vesselId",
        "context",
        "server",
        "seed",
        "talkers",
        "panelOrder",
        "panel",
    ];
    if let Some(obj) = v.as_object() {
        let rest: Map<String, Value> = obj
            .iter()
            .filter(|(k, _)| !known.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if !rest.is_empty() {
            warnings.push(format!(
                "clés legacy non reconnues conservées : {}",
                rest.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
            c.extra.insert("legacy".into(), Value::Object(rest));
        }
    }
    Ok((c, warnings))
}
