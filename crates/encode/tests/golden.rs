//! Compatibilité « NMEASimulator legacy VS nouveau simulateur » sur les golden
//! files de `tests/fixtures/legacy/`.
//!
//! Pour chaque bloc capturé : l'état est reconstruit à partir des phrases,
//! réencodé en profil `legacy`, puis comparé champ par champ. Chaque écart est
//! classé :
//!
//! 1. `FixVolontaire` — bogue corrigé volontairement (`docs/04`) ;
//! 2. `ModerneVolontaire` — amélioration volontaire ou aléa legacy ;
//! 3. `Involontaire` — tout le reste : **le test échoue**.
//!
//! Les écarts numériques dans la précision de reconstruction (valeurs
//! déduites de champs arrondis) ne sont pas des divergences.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use nmeasim_core::random::Xoshiro256StarStar;
use nmeasim_core::units::{celsius_to_kelvin, deg_to_rad, knots_to_mps};
use nmeasim_encode::ais;
use nmeasim_encode::nmea::{self, Ctx, NmeaOptions};
use nmeasim_encode::parse::{self, Parsed};
use nmeasim_encode::sentence::legacy_coord;
use nmeasim_encode::signalk::{self, SignalKOptions};
use nmeasim_encode::viewsync::{self, ViewSyncOptions};
use nmeasim_sim::compat::{CompatConfig, CompatProfile, Profile};
use nmeasim_sim::state::{SimState, Waypoint};
use nmeasim_sim::{SimConfig, Simulator};
use serde_json::Value;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/legacy")
}

/// Chaînes JSON d'une capture (une par ligne), en ne gardant que le
/// récepteur `R1` pour le multicast.
fn payloads(file: &str) -> Vec<String> {
    let text = std::fs::read_to_string(fixtures().join(file)).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.contains(" R2 "))
        .filter_map(|l| l.find('"').map(|i| &l[i..]))
        .filter_map(|j| serde_json::from_str::<String>(j).ok())
        .collect()
}

/// Blocs de phrases : chaque bloc commence par `RMC`.
fn blocks(file: &str) -> Vec<Vec<String>> {
    let all: String = payloads(file).concat();
    let mut out: Vec<Vec<String>> = Vec::new();
    for line in all.split("\r\n").filter(|l| !l.is_empty()) {
        let is_rmc = parse::parse(line).is_some_and(|p| p.formatter == "RMC");
        if is_rmc {
            out.push(Vec::new());
        }
        if let Some(b) = out.last_mut() {
            b.push(line.to_string());
        }
    }
    // Garder les blocs complets (RMB en dernier).
    out.into_iter()
        .filter(|b| {
            b.last()
                .and_then(|l| parse::parse(l))
                .is_some_and(|p| p.formatter == "RMB")
        })
        .collect()
}

fn legacy_sim() -> SimState {
    let cfg = SimConfig {
        seed: Some(1),
        start_time_ms: Some(0),
        compatibility: CompatConfig {
            profile: Profile::Legacy,
            ..Default::default()
        },
        ..Default::default()
    };
    Simulator::new(cfg).unwrap().snapshot()
}

/// Reconstruction de l'état à partir d'un bloc legacy.
fn reconstruct(block: &[String]) -> (SimState, i32) {
    let mut s = legacy_sim();
    let mut tz = 0;
    let parsed: Vec<Parsed> = block.iter().filter_map(|l| parse::parse(l)).collect();
    let by = |f: &str| parsed.iter().find(|p| p.formatter == f);
    let rmc = by("RMC").unwrap();
    let t = NaiveTime::parse_from_str(rmc.f(0), "%H%M%S%.3f").unwrap();
    let d = NaiveDate::parse_from_str(rmc.f(8), "%d%m%y").unwrap();
    s.time_ms = NaiveDateTime::new(d, t).and_utc().timestamp_millis();
    s.vessel.position.lat = parse::coord(rmc.f(2), rmc.f(3)).unwrap();
    s.vessel.position.lon = parse::coord(rmc.f(4), rmc.f(5)).unwrap();
    let sog = knots_to_mps(rmc.num(6).unwrap());
    s.vessel.sog = sog;
    s.vessel.stw = sog;
    let hdg = deg_to_rad(by("VHW").and_then(|p| p.num(0)).unwrap());
    s.vessel.heading = hdg;
    s.vessel.cog = hdg;
    if let Some(g) = by("GGA") {
        s.gnss.quality = g.num(5).unwrap() as u8;
        s.gnss.hdop = g.num(7).unwrap();
        s.vessel.altitude = g.num(8).unwrap();
    }
    if let Some(g) = by("GSA") {
        s.gnss.used = (2..14).filter_map(|i| g.f(i).parse().ok()).collect();
        s.gnss.pdop = g.num(14).unwrap();
        s.gnss.vdop = g.num(16).unwrap();
    }
    if let Some(z) = by("ZDA") {
        let h: i32 = z.f(4).parse().unwrap();
        let m: i32 = z.f(5).parse().unwrap();
        tz = if z.f(4).starts_with('-') {
            h * 60 - m
        } else {
            h * 60 + m
        };
    }
    if let Some(w) = by("MWD") {
        s.wind.true_direction = deg_to_rad(w.num(0).unwrap());
        s.wind.true_speed = knots_to_mps(w.num(4).unwrap());
    }
    match by("MWV") {
        Some(w) => {
            let a = w.num(0).unwrap();
            s.wind.apparent_angle = deg_to_rad(if a > 180.0 { a - 360.0 } else { a });
            s.wind.apparent_speed = knots_to_mps(w.num(2).unwrap());
        }
        None => s.wind.apparent_speed = 0.0,
    }
    s.water.temperature = celsius_to_kelvin(by("MTW").and_then(|p| p.num(0)).unwrap());
    s.water.depth = by("DPT").and_then(|p| p.num(0)).unwrap();
    for (i, r) in parsed
        .iter()
        .filter(|p| p.formatter == "RPM")
        .enumerate()
        .take(2)
    {
        s.engines[i].running = r.f(2) != "0";
        s.engines[i].rpm = r.num(2).unwrap_or(0.0);
    }
    s.marked = by("WPL").map(|w| Waypoint {
        name: w.f(4).to_string(),
        position: nmeasim_core::geo::LatLon::new(
            parse::coord(w.f(0), w.f(1)).unwrap(),
            parse::coord(w.f(2), w.f(3)).unwrap(),
        ),
    });
    (s, tz)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Reconstruction,
    FixVolontaire(&'static str),
    ModerneVolontaire(&'static str),
}

fn decimals(s: &str) -> i32 {
    s.split_once('.').map_or(0, |(_, d)| d.len() as i32)
}

fn close(a: &str, b: &str, units: f64) -> bool {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() <= units * 10f64.powi(-decimals(a)) + 1e-9,
        _ => false,
    }
}

/// Compare une phrase legacy et une phrase produite ; rend les classes des
/// champs différents, ou `Err` pour une divergence involontaire.
fn compare(legacy: &Parsed, ours: &Parsed) -> Result<Vec<Class>, String> {
    let f = legacy.formatter.as_str();
    if legacy.formatter != ours.formatter || legacy.talker != ours.talker {
        return Err(format!(
            "adresse {}{} ≠ {}{}",
            legacy.talker, legacy.formatter, ours.talker, ours.formatter
        ));
    }
    if !ours.checksum_ok {
        return Err(format!("checksum invalide dans {f}"));
    }
    if f == "VBW" {
        return Ok(vec![Class::ModerneVolontaire("VBW aléatoire legacy (D5)")]);
    }
    if f == "VDO" || f == "VDM" {
        return compare_ais(legacy, ours);
    }
    let mut classes = Vec::new();
    let n = legacy.fields.len().max(ours.fields.len());
    for i in 0..n {
        let (a, b) = (legacy.f(i), ours.f(i));
        if a == b {
            continue;
        }
        let coord_field = matches!(
            (f, i),
            ("RMC", 2 | 4) | ("GLL", 0 | 2) | ("GGA", 1 | 3) | ("WPL", 0 | 2)
        );
        let class = if coord_field && close(a, b, 1.0) {
            let v = parse::coord(b, "N").unwrap();
            if legacy_coord(v, a.find('.').unwrap_or(0) == 4) == a {
                Class::FixVolontaire("coordonnées : fraction tronquée et de longueur variable")
            } else {
                Class::Reconstruction
            }
        } else if f == "RMC" && i == 11 && a.is_empty() && b == "A" {
            Class::FixVolontaire("RMC : mode de navigation ajouté")
        } else if close(a, b, 3.0) {
            Class::Reconstruction
        } else {
            return Err(format!(
                "{f} champ {} : legacy `{a}` ≠ nouveau `{b}`",
                i + 1
            ));
        };
        classes.push(class);
    }
    Ok(classes)
}

fn compare_ais(legacy: &Parsed, ours: &Parsed) -> Result<Vec<Class>, String> {
    let head = |p: &Parsed| p.fields[..4].join(",");
    if head(legacy) != head(ours) || legacy.f(5) != ours.f(5) {
        return Err(format!(
            "AIS en-tête {} ≠ {}",
            legacy.fields.join(","),
            ours.fields.join(",")
        ));
    }
    if legacy.f(4) == ours.f(4) {
        return Ok(vec![]);
    }
    let a = ais::dearmor(legacy.f(4));
    let b = ais::dearmor(ours.f(4));
    let t = ais::read_uint(&a, 0, 6);
    if t == 1 && a.len() == b.len() {
        // Écarts de ±1 sur lat/lon (1/10 000 min) dus à la reconstruction
        // depuis des minutes à 6 décimales.
        for (start, n, tol) in [
            (8, 30, 0),
            (50, 10, 1),
            (61, 28, 2),
            (89, 27, 2),
            (116, 12, 1),
            (128, 9, 1),
            (137, 6, 0),
        ] {
            let (x, y) = (
                ais::read_uint(&a, start, n) as i64,
                ais::read_uint(&b, start, n) as i64,
            );
            if (x - y).abs() > tol {
                return Err(format!("AIS 1 bits {start}+{n} : {x} ≠ {y}"));
            }
        }
        return Ok(vec![Class::Reconstruction]);
    }
    Err(format!("AIS charge utile {} ≠ {}", legacy.f(4), ours.f(4)))
}

#[derive(Default)]
struct Report {
    sentences: usize,
    identical: usize,
    classes: BTreeMap<String, usize>,
    unintended: Vec<String>,
}

fn run_file(file: &str, report: &mut Report) {
    let blocks = blocks(file);
    assert!(!blocks.is_empty(), "{file} : aucun bloc complet");
    for block in &blocks {
        let (state, tz) = reconstruct(block);
        let opts = NmeaOptions::default();
        let ctx = Ctx {
            state: &state,
            compat: CompatProfile::legacy(),
            opts: &opts,
            tz_offset_minutes: tz,
            vessel_length_m: 20.0,
        };
        let mut rng = Xoshiro256StarStar::seed_from_u64(0);
        let ours: Vec<String> = nmea::block(&ctx, &mut rng);
        let legacy: Vec<Parsed> = block.iter().filter_map(|l| parse::parse(l)).collect();
        let ours: Vec<Parsed> = ours.iter().filter_map(|l| parse::parse(l)).collect();
        let ids = |v: &[Parsed]| {
            v.iter()
                .map(|p| format!("{}{}", p.talker, p.formatter))
                .collect::<Vec<_>>()
        };
        if ids(&legacy) != ids(&ours) {
            report.unintended.push(format!(
                "{file} : séquence {:?} ≠ {:?}",
                ids(&legacy),
                ids(&ours)
            ));
            continue;
        }
        for (l, o) in legacy.iter().zip(&ours) {
            report.sentences += 1;
            match compare(l, o) {
                Ok(c) if c.is_empty() => report.identical += 1,
                Ok(c) => {
                    for k in c {
                        *report.classes.entry(format!("{k:?}")).or_default() += 1;
                    }
                }
                Err(e) => report.unintended.push(format!("{file} : {e}")),
            }
        }
    }
}

#[test]
fn nmea_golden_files_legacy_vs_new() {
    let files = [
        "cap-tcp.log",
        "cap-fixed.log",
        "cap-prefix.log",
        "cap-ws.log",
        "cap-udp2.log",
        "cap-turn.log",
        "cap-steer.log",
        "cap-ui2.log",
        "cap-udpclient.log",
        "cap-tcpclient.log",
        "cap-mcast.log",
    ];
    let mut report = Report::default();
    for f in files {
        run_file(f, &mut report);
    }
    println!("phrases comparées : {}", report.sentences);
    println!("identiques        : {}", report.identical);
    for (k, v) in &report.classes {
        println!("{v:6} × {k}");
    }
    assert!(report.sentences > 3000, "{}", report.sentences);
    assert!(
        report.unintended.is_empty(),
        "{} divergences involontaires, dont : {:#?}",
        report.unintended.len(),
        &report.unintended[..report.unintended.len().min(15)]
    );
}

#[test]
fn route_golden_hemisphere_fix() {
    // legacy/cap-dest.log et cap-geo.log : RMB avec route. La destination est
    // reconstruite avec l'hémisphère qui rend la distance émise cohérente ;
    // la distance et le relèvement recalculés doivent coïncider.
    for file in ["cap-dest.log", "cap-geo.log"] {
        let mut checked = 0;
        for block in blocks(file) {
            let rmb = block
                .iter()
                .filter_map(|l| parse::parse(l))
                .find(|p| p.formatter == "RMB")
                .unwrap();
            if rmb.f(0) != "A" {
                continue;
            }
            assert_eq!(rmb.f(6), "N", "le legacy émet toujours N");
            let (s, _) = reconstruct(&block);
            let dtg_nm = rmb.num(9).unwrap();
            let lat = parse::coord(rmb.f(5), "N").unwrap();
            let lon = parse::coord(rmb.f(7), "E").unwrap();
            let best = [(lat, lon), (-lat, lon), (lat, -lon), (-lat, -lon)]
                .into_iter()
                .map(|(a, b)| nmeasim_core::geo::LatLon::new(a, b))
                .min_by(|a, b| {
                    let da = (s.vessel.position.distance_to(*a) / 1852.0 - dtg_nm).abs();
                    let db = (s.vessel.position.distance_to(*b) / 1852.0 - dtg_nm).abs();
                    da.total_cmp(&db)
                })
                .unwrap();
            let dist = s.vessel.position.distance_to(best) / 1852.0;
            let brg = s.vessel.position.bearing_to(best).to_degrees();
            assert!((dist - dtg_nm).abs() < 0.01, "{file} : {dist} vs {dtg_nm}");
            assert!(
                (brg - rmb.num(10).unwrap()).abs() < 0.1,
                "{file} : relèvement"
            );
            assert!(
                best.lat < 0.0,
                "{file} : destination réelle dans l'hémisphère sud"
            );
            checked += 1;
        }
        assert!(checked > 0, "{file}");
    }
}

#[test]
fn signalk_golden_legacy_vs_new() {
    let msgs: Vec<Value> = payloads("cap-sk.log")
        .iter()
        .filter_map(|p| serde_json::from_str(p).ok())
        .collect();
    let hello = &msgs[0];
    assert_eq!(hello["version"], "17.12.05", "défaut legacy documenté");
    let ours: Value =
        serde_json::from_str(&signalk::hello(&SignalKOptions::default(), "1.0.0", 0)).unwrap();
    for k in ["name", "self", "roles"] {
        assert_eq!(hello[k], ours[k], "hello.{k}");
    }
    let mut compared = 0;
    for delta in &msgs[1..] {
        let values = delta["updates"][0]["values"].as_array().unwrap();
        let get = |p: &str| {
            values
                .iter()
                .find(|v| v["path"] == p)
                .map(|v| v["value"].clone())
                .unwrap()
        };
        let mut s = legacy_sim();
        let ts = delta["updates"][0]["timestamp"].as_str().unwrap();
        s.time_ms = chrono::DateTime::parse_from_rfc3339(ts)
            .unwrap()
            .timestamp_millis();
        s.vessel.cog = get("navigation.courseOverGroundTrue").as_f64().unwrap();
        s.vessel.heading = get("navigation.headingTrue").as_f64().unwrap();
        s.vessel.sog = get("navigation.speedOverGround").as_f64().unwrap();
        s.vessel.stw = get("navigation.speedThroughWater").as_f64().unwrap();
        s.gnss.hdop = get("navigation.gnss.horizontalDilution").as_f64().unwrap();
        s.gnss.pdop = get("navigation.gnss.positionDilution").as_f64().unwrap();
        let pos = get("navigation.position");
        s.vessel.position.lat = pos["latitude"].as_f64().unwrap();
        s.vessel.position.lon = pos["longitude"].as_f64().unwrap();
        s.vessel.altitude = pos["altitude"].as_f64().unwrap();
        s.wind.true_direction = get("environment.wind.directionTrue").as_f64().unwrap();
        s.wind.true_speed = get("environment.wind.speedTrue").as_f64().unwrap();
        s.wind.apparent_angle = get("environment.wind.angleApparent").as_f64().unwrap();
        s.wind.apparent_speed = get("environment.wind.speedApparent").as_f64().unwrap();
        s.water.temperature = get("environment.water.temperature").as_f64().unwrap();
        s.water.depth = get("environment.depth.belowTransducer").as_f64().unwrap();
        for (i, id) in ["p0", "p1"].iter().enumerate() {
            s.engines[i].running = get(&format!("propulsion.{id}.state")) == "started";
            s.engines[i].temperature = get(&format!("propulsion.{id}.temperature"))
                .as_f64()
                .unwrap();
        }
        let ours = signalk::delta_value(&s, CompatProfile::legacy(), &SignalKOptions::default());
        assert_eq!(ours["context"], delta["context"]);
        assert_eq!(
            ours["updates"][0]["$source"],
            delta["updates"][0]["$source"]
        );
        assert_eq!(
            ours["updates"][0]["timestamp"],
            delta["updates"][0]["timestamp"]
        );
        assert_eq!(
            &ours["updates"][0]["values"], &delta["updates"][0]["values"],
            "delta identique"
        );
        compared += 1;
    }
    assert!(compared >= 5);
}

#[test]
fn viewsync_golden_legacy_vs_new() {
    let mut compared = 0;
    for p in payloads("cap-udp.log") {
        let f: Vec<&str> = p.split(',').collect();
        let mut s = legacy_sim();
        s.vessel.position.lat = f[1].parse().unwrap();
        s.vessel.position.lon = f[2].parse().unwrap();
        s.vessel.heading = f[4].parse::<f64>().unwrap().to_radians();
        s.time_ms = (f[7].parse::<i64>().unwrap() - viewsync::EPOCH_OFFSET_S) * 1000 + 500;
        let ours = viewsync::packet(f[0].parse().unwrap(), &s, &ViewSyncOptions::default(), 1000);
        let (a, b): (Vec<&str>, Vec<&str>) = (p.split(',').collect(), ours.split(',').collect());
        assert_eq!(a.len(), b.len());
        for i in 0..a.len() {
            if i == 4 {
                // Cap : aller-retour degrés → radians → degrés, à 1e-9 près.
                assert!((a[4].parse::<f64>().unwrap() - b[4].parse::<f64>().unwrap()).abs() < 1e-9);
            } else {
                assert_eq!(a[i], b[i], "champ {i} de {p}");
            }
        }
        compared += 1;
    }
    assert!(compared >= 5);
}
