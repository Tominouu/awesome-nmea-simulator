//! Bloc NMEA d'un tick : vue pure de l'état (`docs/07`). L'ordre d'émission
//! est une donnée ([`LEGACY_ORDER`], [`MODERN_ORDER`]), pas un effet de bord.

use std::collections::BTreeMap;

use nmeasim_core::random::Rng;
use nmeasim_core::units::{mps_to_kmh, mps_to_knots, normalize_deg, rad_to_deg};
use nmeasim_sim::compat::CompatProfile;
use nmeasim_sim::state::SimState;
use serde::{Deserialize, Serialize};

use crate::ais::{self, AisIdentity, PositionReport};
use crate::sentence::{Sentence, coord, date_field, fixed, prefix, time_field, utc};

/// Ordre legacy : 24 phrases (plus `WPL` si un waypoint est marqué).
pub const LEGACY_ORDER: &[&str] = &[
    "RMC", "VHW", "VTG", "HDT", "GLL", "GGA", "GSA", "ZDA", "VBW", "AIS1", "MWD", "MWV", "MTW",
    "DPT", "DBT", "WPL", "AIS5", "RPM", "APB", "RMB",
];

/// Ordre moderne : legacy + `HDM`, `ROT`, `GSV`.
pub const MODERN_ORDER: &[&str] = &[
    "RMC", "VHW", "VTG", "HDT", "HDM", "ROT", "GLL", "GGA", "GSA", "GSV", "ZDA", "VBW", "AIS1",
    "MWD", "MWV", "MTW", "DPT", "DBT", "WPL", "AIS5", "RPM", "APB", "RMB",
];

/// Toutes les phrases connues (pour l'UI et la validation).
pub const ALL_SENTENCES: &[&str] = MODERN_ORDER;

/// Talker par défaut (`docs/07` §1.2).
pub fn default_talker(id: &str) -> &'static str {
    match id {
        "VHW" | "HDT" | "HDM" | "VBW" | "MTW" | "RPM" | "APB" => "II",
        "ROT" => "TI",
        "MWD" | "MWV" => "WI",
        "DPT" | "DBT" => "SD",
        "WPL" => "IN",
        "AIS1" | "AIS5" => "AI",
        _ => "GP",
    }
}

/// Options d'encodage NMEA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NmeaOptions {
    /// Talker par formateur.
    pub talkers: BTreeMap<String, String>,
    /// Phrases émises, dans l'ordre ; `None` = ordre du profil.
    pub sentences: Option<Vec<String>>,
    /// Préfixe 61162-450.
    pub prefix: bool,
    /// Source du préfixe.
    pub prefix_source: String,
    /// Fuseau pour `ZDA`, minutes (`None` = fuseau système, fourni par l'app).
    pub zda_offset_minutes: Option<i32>,
    /// Identité AIS.
    pub ais: AisIdentity,
}

impl Default for NmeaOptions {
    fn default() -> Self {
        Self {
            talkers: BTreeMap::new(),
            sentences: None,
            prefix: false,
            prefix_source: "nmeasim".into(),
            zda_offset_minutes: None,
            ais: AisIdentity::default(),
        }
    }
}

impl NmeaOptions {
    fn talker(&self, id: &str) -> String {
        self.talkers
            .get(id)
            .cloned()
            .unwrap_or_else(|| default_talker(id).to_string())
    }
}

/// Contexte d'un encodage : état, profil, décalage horaire effectif.
pub struct Ctx<'a> {
    /// Instantané.
    pub state: &'a SimState,
    /// Profil de compatibilité.
    pub compat: CompatProfile,
    /// Options.
    pub opts: &'a NmeaOptions,
    /// Décalage du fuseau `ZDA` en minutes (système si non configuré).
    pub tz_offset_minutes: i32,
    /// Longueur du navire, m (VBW arrière).
    pub vessel_length_m: f64,
}

fn deg(r: f64) -> f64 {
    normalize_deg(rad_to_deg(r))
}

/// Champs de zone `ZDA` : `+` implicite, `-` explicite, minutes non signées.
pub fn zda_zone(offset_min: i32) -> (String, String) {
    let h = offset_min.abs() / 60;
    let m = offset_min.abs() % 60;
    let hs = if offset_min < 0 {
        format!("-{h:02}")
    } else {
        format!("{h:02}")
    };
    (hs, format!("{m:02}"))
}

/// Produit le bloc de phrases d'un tick, sans terminateurs.
pub fn block(ctx: &Ctx<'_>, rng: &mut dyn Rng) -> Vec<String> {
    let default_order = if ctx.compat.legacy_sentences {
        LEGACY_ORDER
    } else {
        MODERN_ORDER
    };
    let order: Vec<String> = match &ctx.opts.sentences {
        Some(list) => list.clone(),
        None => default_order.iter().map(|s| (*s).to_string()).collect(),
    };
    let mut out = Vec::new();
    for id in &order {
        out.extend(sentence(id, ctx, rng));
    }
    if ctx.opts.prefix {
        let p = prefix(ctx.state.sentence_time_ms(), &ctx.opts.prefix_source);
        out = out.into_iter().map(|s| format!("{p}{s}")).collect();
    }
    out
}

/// Une phrase (ou plusieurs pour AIS et RPM) par identifiant.
pub fn sentence(id: &str, ctx: &Ctx<'_>, rng: &mut dyn Rng) -> Vec<String> {
    let s = ctx.state;
    let v = &s.vessel;
    let t = s.sentence_time_ms();
    let tk = ctx.opts.talker(id);
    let fix = s.gnss.fix;
    let (lat, ns) = coord(v.position.lat, true);
    let (lon, ew) = coord(v.position.lon, false);
    let heading = deg(v.heading);
    let cog = deg(v.cog);
    let hdg_m = deg(s.heading_magnetic());
    let variation = rad_to_deg(s.magnetic_variation);
    let sog_kn = mps_to_knots(v.sog);
    let stw_kn = mps_to_knots(v.stw);
    let pos = |b: Sentence| {
        if fix {
            b.field(&lat).field(ns).field(&lon).field(ew)
        } else {
            b.empty(4)
        }
    };
    let one = |x: Sentence| vec![x.finish()];
    match id {
        "RMC" => {
            let b = Sentence::new('$', &tk, "RMC")
                .field(time_field(t))
                .field(if fix { "A" } else { "V" });
            let b = pos(b)
                .field(if fix { fixed(sog_kn, 1) } else { String::new() })
                .field(if fix { fixed(cog, 1) } else { String::new() })
                .field(date_field(t))
                .empty(2)
                .field(if fix { "A" } else { "N" });
            one(b)
        }
        "VHW" => one(Sentence::new('$', &tk, "VHW")
            .field(fixed(heading, 1))
            .field("T")
            .field(fixed(hdg_m, 1))
            .field("M")
            .field(fixed(stw_kn, 1))
            .field("N")
            .field(fixed(mps_to_kmh(v.stw), 1))
            .field("K")),
        "VTG" => one(Sentence::new('$', &tk, "VTG")
            .field(fixed(cog, 1))
            .field("T")
            .field(fixed(normalize_deg(cog - variation), 1))
            .field("M")
            .field(fixed(sog_kn, 1))
            .field("N")
            .field(fixed(mps_to_kmh(v.sog), 1))
            .field("K")),
        "HDT" => one(Sentence::new('$', &tk, "HDT")
            .field(fixed(heading, 1))
            .field("T")),
        "HDM" => one(Sentence::new('$', &tk, "HDM")
            .field(fixed(hdg_m, 1))
            .field("M")),
        "ROT" => one(Sentence::new('$', &tk, "ROT")
            .field(fixed(rad_to_deg(v.rate_of_turn) * 60.0, 1))
            .field("A")),
        "GLL" => {
            let b = pos(Sentence::new('$', &tk, "GLL"))
                .field(time_field(t))
                .field(if fix { "A" } else { "V" });
            one(b)
        }
        "GGA" => {
            let b = Sentence::new('$', &tk, "GGA").field(time_field(t));
            let b = pos(b)
                .field(s.gnss.quality.to_string())
                .field(s.gnss.used.len().to_string())
                .field(fixed(s.gnss.hdop, 1))
                .field(fixed(v.altitude, 1))
                .field("M")
                .empty(4);
            one(b)
        }
        "GSA" => {
            let mut b = Sentence::new('$', &tk, "GSA").field("A").field(if fix {
                s.gnss.mode.to_string()
            } else {
                "1".into()
            });
            for k in 0..12 {
                b = b.field(s.gnss.used.get(k).map(u8::to_string).unwrap_or_default());
            }
            one(b
                .field(fixed(s.gnss.pdop, 1))
                .field(fixed(s.gnss.hdop, 1))
                .field(fixed(s.gnss.vdop, 1)))
        }
        "GSV" => {
            let sats = &s.gnss.satellites;
            let total = sats.len().div_ceil(4).max(1);
            (0..total)
                .map(|m| {
                    let mut b = Sentence::new('$', &tk, "GSV")
                        .field(total.to_string())
                        .field((m + 1).to_string())
                        .field(format!("{:02}", sats.len()));
                    for sat in sats.iter().skip(4 * m).take(4) {
                        b = b
                            .field(format!("{:02}", sat.prn))
                            .field(format!("{:02}", sat.elevation.round() as i64))
                            .field(format!("{:03}", sat.azimuth.round() as i64 % 360))
                            .field(format!("{:02}", sat.snr.round() as i64));
                    }
                    b.finish()
                })
                .collect()
        }
        "ZDA" => {
            let d = utc(t);
            let (zh, zm) = zda_zone(ctx.tz_offset_minutes);
            use chrono::Datelike;
            one(Sentence::new('$', &tk, "ZDA")
                .field(time_field(t))
                .field(format!("{:02}", d.day()))
                .field(format!("{:02}", d.month()))
                .field(d.year().to_string())
                .field(zh)
                .field(zm))
        }
        "VBW" => {
            let vals: [f64; 6] = if ctx.compat.legacy_vbw {
                let sp = sog_kn;
                std::array::from_fn(|_| sp - rng.next_f64())
            } else {
                // D5 : vitesses longitudinales et transversales (tribord +),
                // eau puis fond ; l'arrière ajoute r · L/2.
                let rel = v.cog - v.heading;
                let (gl, gt) = (sog_kn * rel.cos(), sog_kn * rel.sin());
                let (wl, wt) = (stw_kn, mps_to_knots(v.sway));
                let stern_add = mps_to_knots(v.rate_of_turn * ctx.vessel_length_m / 2.0);
                [wl, wt, gl, gt, wt + stern_add, gt + stern_add]
            };
            one(Sentence::new('$', &tk, "VBW")
                .field(fixed(vals[0], 1))
                .field(fixed(vals[1], 1))
                .field("A")
                .field(fixed(vals[2], 1))
                .field(fixed(vals[3], 1))
                .field("A")
                .field(fixed(vals[4], 1))
                .field("A")
                .field(fixed(vals[5], 1))
                .field("A"))
        }
        "AIS1" => {
            let legacy = ctx.compat.legacy_sentences;
            let p = PositionReport {
                sog_kn,
                cog_deg: if legacy { heading } else { cog },
                heading_deg: heading,
                lat: v.position.lat,
                lon: v.position.lon,
                rot_deg_min: Some(rad_to_deg(v.rate_of_turn) * 60.0),
                utc_second: chrono::Timelike::second(&utc(t)),
                nav_status: if v.anchored && !legacy { 1 } else { 0 },
            };
            let b = ais::message1(ctx.opts.ais.mmsi, &p, legacy);
            let seq = ctx.opts.ais.sequential_id;
            let mut o = ais::sentences(&b, &tk, "VDO", seq, legacy);
            o.extend(ais::sentences(&b, &tk, "VDM", seq, legacy));
            o
        }
        "AIS5" => {
            let legacy = ctx.compat.legacy_sentences;
            use chrono::{Datelike, Timelike};
            let eta_t = utc(ctx.opts.ais.eta_ms.unwrap_or(t));
            let eta = (eta_t.month(), eta_t.day(), eta_t.hour(), eta_t.minute());
            let b = ais::message5(&ctx.opts.ais, eta);
            let seq = ctx.opts.ais.sequential_id;
            let mut o = ais::sentences(&b, &tk, "VDO", seq, legacy);
            o.extend(ais::sentences(&b, &tk, "VDM", seq, legacy));
            o
        }
        "MWD" => one(Sentence::new('$', &tk, "MWD")
            .field(fixed(deg(s.wind.true_direction), 1))
            .field("T")
            .field(fixed(
                normalize_deg(deg(s.wind.true_direction) - variation),
                1,
            ))
            .field("M")
            .field(fixed(mps_to_knots(s.wind.true_speed), 1))
            .field("N")
            .field(fixed(s.wind.true_speed, 1))
            .field("M")),
        "MWV" => {
            if s.wind.apparent_speed == 0.0 {
                return vec![];
            }
            one(Sentence::new('$', &tk, "MWV")
                .field(fixed(deg(s.wind.apparent_angle), 1))
                .field("R")
                .field(fixed(mps_to_knots(s.wind.apparent_speed), 1))
                .field("N")
                .field("A"))
        }
        "MTW" => one(Sentence::new('$', &tk, "MTW")
            .field(fixed(s.water.temperature - 273.15, 1))
            .field("C")),
        "DPT" => one(Sentence::new('$', &tk, "DPT")
            .field(fixed(s.water.depth, 1))
            .field("0.3")),
        "DBT" => {
            // D4 : ordre legacy conforme, facteurs legacy pour l'égalité octet.
            let d = s.water.depth;
            one(Sentence::new('$', &tk, "DBT")
                .field(fixed(3.28084 * d, 1))
                .field("f")
                .field(fixed(d, 1))
                .field("M")
                .field(fixed(0.546807 * d, 1))
                .field("F"))
        }
        "WPL" => match &s.marked {
            None => vec![],
            Some(w) => {
                let (la, n) = coord(w.position.lat, true);
                let (lo, e) = coord(w.position.lon, false);
                one(Sentence::new('$', &tk, "WPL")
                    .field(la)
                    .field(n)
                    .field(lo)
                    .field(e)
                    .field(&w.name))
            }
        },
        "RPM" => s
            .engines
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let b = Sentence::new('$', &tk, "RPM")
                    .field("E")
                    .field((i + 1).to_string());
                if ctx.compat.legacy_rpm {
                    b.field(if e.running {
                        fixed(e.rpm, 1)
                    } else {
                        "0".into()
                    })
                    .field("10.5")
                    .field("A")
                    .finish()
                } else {
                    b.field(if e.running {
                        fixed(e.rpm, 1)
                    } else {
                        "0.0".into()
                    })
                    .field(fixed(e.pitch_percent, 1))
                    .field("A")
                    .finish()
                }
            })
            .collect(),
        "APB" | "RMB" => route_sentence(id, &tk, ctx),
        _ => vec![],
    }
}

fn route_sentence(id: &str, tk: &str, ctx: &Ctx<'_>) -> Vec<String> {
    let s = ctx.state;
    let (Some(dest), Some(g)) = (s.route.destination(), s.route.geometry.as_ref()) else {
        // Inerte sans route : forme legacy (docs/04 §3.5).
        return vec![if id == "APB" {
            Sentence::new('$', tk, "APB")
                .field("V")
                .field("V")
                .empty(1)
                .field("R")
                .field("N")
                .empty(3)
                .field("T")
                .empty(2)
                .field("T")
                .empty(1)
                .field("T")
                .field("N")
                .finish()
        } else {
            Sentence::new('$', tk, "RMB")
                .field("V")
                .empty(1)
                .field("R")
                .empty(10)
                .field("N")
                .finish()
        }];
    };
    let xte_nm = (g.xte / 1852.0).abs();
    // Correction : à gauche de la route (xte < 0), il faut venir à droite.
    let steer = if g.xte < 0.0 { "R" } else { "L" };
    let origin = s
        .route
        .origin
        .as_ref()
        .map_or("origin", |o| o.name.as_str());
    let brg_o = fixed(deg(g.bearing_origin_to_dest), 1);
    let brg_c = fixed(deg(g.bearing_to_dest), 1);
    let arrived = if g.arrived { "A" } else { "V" };
    if id == "APB" {
        let hts = match s.autopilot.mode {
            nmeasim_sim::state::AutopilotMode::Heading => deg(s.autopilot.heading_target),
            _ => deg(g.bearing_to_dest),
        };
        vec![
            Sentence::new('$', tk, "APB")
                .field("A")
                .field("A")
                .field(fixed(xte_nm, 6))
                .field(steer)
                .field("N")
                .field(arrived)
                .field(if g.perpendicular_passed { "A" } else { "V" })
                .field(brg_o)
                .field("T")
                .field(&dest.name)
                .field(&brg_c)
                .field("T")
                .field(fixed(hts, 1))
                .field("T")
                .field("A")
                .finish(),
        ]
    } else {
        let (la, n) = coord(dest.position.lat, true);
        let (lo, e) = coord(dest.position.lon, false);
        vec![
            Sentence::new('$', tk, "RMB")
                .field("A")
                .field(fixed(xte_nm, 6))
                .field(steer)
                .field(origin)
                .field(&dest.name)
                .field(la)
                .field(n)
                .field(lo)
                .field(e)
                .field(fixed(g.dtg / 1852.0, 3))
                .field(brg_c)
                .field(fixed(mps_to_knots(g.closing_speed), 1))
                .field(arrived)
                .field("A")
                .finish(),
        ]
    }
}
