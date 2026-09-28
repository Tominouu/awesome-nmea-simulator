//! Le simulateur : seul propriétaire de l'état, seul écrivain.
//!
//! Déterminisme : à graine, configuration, séquence de commandes et durée
//! égales, l'état est identique bit à bit. Aucune source globale d'aléa ni
//! d'horloge n'est consultée pendant l'intégration.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use nmeasim_core::geo::LatLon;
use nmeasim_core::random::{Rng, Xoshiro256StarStar, entropy_seed};
use nmeasim_core::units::{
    angle_diff_deg, celsius_to_kelvin, deg_to_rad, kelvin_to_celsius, knots_to_mps, mps_to_knots,
    normalize_deg, normalize_rad, rad_to_deg,
};
use serde::{Deserialize, Serialize};

use crate::command::{Command, EngineSel, RejectReason, Source};
use crate::compat::CompatProfile;
use crate::config::{ConfigError, SimConfig};
use crate::drift;
use crate::state::*;

/// Événement émis par le simulateur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum SimEvent {
    /// Simulation démarrée.
    Started,
    /// Simulation arrêtée.
    Stopped,
    /// Pause.
    Paused,
    /// Reprise.
    Resumed,
    /// Remise à l'état initial.
    Reset,
    /// Mode du pilote changé.
    AutopilotChanged {
        /// Nouveau mode.
        mode: AutopilotMode,
    },
    /// Route modifiée.
    RouteChanged,
    /// Point de route atteint.
    WaypointReached {
        /// Index atteint.
        index: usize,
    },
    /// Moteur démarré ou arrêté.
    EngineChanged {
        /// Index.
        index: usize,
        /// En marche.
        running: bool,
    },
    /// Mouillage changé.
    AnchorChanged {
        /// Mouillé.
        on: bool,
    },
    /// Override posé ou levé.
    OverrideChanged {
        /// Chemin.
        path: String,
        /// Actif.
        active: bool,
    },
}

/// Valeurs nominales des grandeurs bruitées (modern) : le bruit y revient.
#[derive(Debug, Clone, PartialEq)]
struct Nominals {
    wind_speed_kn: f64,
    wind_dir_deg: f64,
    depth_m: f64,
    water_temp_c: f64,
    hdop: f64,
    vdop: f64,
}

/// Simulateur déterministe.
#[derive(Debug, Clone)]
pub struct Simulator {
    cfg: SimConfig,
    compat: CompatProfile,
    state: SimState,
    rng: Xoshiro256StarStar,
    nominals: Nominals,
    /// Vitesse commandée en propulsion directe (legacy), m/s.
    commanded_speed: f64,
    events: Vec<SimEvent>,
}

const CURVATURE: [(f64, f64); 6] = [
    (0.0, 0.0),
    (2.0, 1.0 / 600.0),
    (5.0, 1.0 / 400.0),
    (10.0, 1.0 / 200.0),
    (20.0, 1.0 / 150.0),
    (40.0, 1.0 / 100.0),
];

/// Courbure de giration (1/m) pour un angle de barre (degrés, valeur absolue).
/// Interpolation linéaire passant par les paliers legacy (décision 010).
pub fn curvature(rudder_deg: f64) -> f64 {
    let a = rudder_deg.abs();
    for w in CURVATURE.windows(2) {
        let ((a0, k0), (a1, k1)) = (w[0], w[1]);
        if a <= a1 {
            return k0 + (k1 - k0) * (a - a0) / (a1 - a0);
        }
    }
    CURVATURE[CURVATURE.len() - 1].1
}

/// Rayon de giration legacy par paliers (`updateHeading`), m. Le legacy
/// n'applique les paliers qu'aux angles positifs ; la cible utilise la valeur
/// absolue (correction, `docs/04`).
pub fn legacy_turn_radius(rudder_deg: f64) -> f64 {
    let a = rudder_deg.abs();
    if a >= 20.0 {
        150.0
    } else if a >= 10.0 {
        200.0
    } else if a >= 5.0 {
        400.0
    } else {
        600.0
    }
}

fn wall_clock_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn range(v: f64, min: f64, max: f64) -> Result<f64, RejectReason> {
    if v.is_finite() && v >= min && v <= max {
        Ok(v)
    } else {
        Err(RejectReason::OutOfRange { min, max })
    }
}

/// Chemins numériques écrivables : (chemin, min, max) en unités d'usage.
pub const PATHS: &[(&str, f64, f64)] = &[
    ("vessel.headingTrue", 0.0, 360.0),
    ("vessel.speedThroughWater", -30.0, 70.0),
    ("vessel.speedOverGround", 0.0, 70.0),
    ("vessel.altitude", -100.0, 10_000.0),
    ("vessel.rudder.command", -90.0, 90.0),
    ("env.wind.directionTrue", 0.0, 360.0),
    ("env.wind.speedTrue", 0.0, 100.0),
    ("env.water.depth", 0.1, 11_000.0),
    ("env.water.temperature", -2.0, 40.0),
    ("env.current.set", 0.0, 360.0),
    ("env.current.drift", 0.0, 10.0),
    ("env.magneticVariation", -180.0, 180.0),
    ("gnss.hdop", 0.1, 50.0),
    ("gnss.vdop", 0.1, 50.0),
    ("gnss.pdop", 0.1, 50.0),
    ("gnss.fix", 0.0, 1.0),
    ("propulsion.p0.rpm", 0.0, 10_000.0),
    ("propulsion.p1.rpm", 0.0, 10_000.0),
    ("propulsion.p0.temperature", -40.0, 200.0),
    ("propulsion.p1.temperature", -40.0, 200.0),
];

const READ_ONLY: &[&str] = &[
    "vessel.rudder.angle",
    "vessel.rateOfTurn",
    "vessel.courseOverGround",
];

impl Simulator {
    /// Construit un simulateur ; la configuration est validée.
    pub fn new(cfg: SimConfig) -> Result<Self, ConfigError> {
        cfg.validate()?;
        let seed = cfg.seed.unwrap_or_else(entropy_seed);
        let compat = cfg.compatibility.resolve();
        let mut rng = Xoshiro256StarStar::seed_from_u64(seed);
        let start = cfg.start_time_ms.unwrap_or_else(wall_clock_ms);
        let state = initial_state(&cfg, &compat, seed, start, &mut rng);
        let nominals = nominals_from(&cfg);
        let commanded_speed = knots_to_mps(cfg.initial.speed_kn);
        Ok(Self {
            cfg,
            compat,
            state,
            rng,
            nominals,
            commanded_speed,
            events: Vec::new(),
        })
    }

    /// État courant (lecture seule).
    pub fn state(&self) -> &SimState {
        &self.state
    }

    /// Instantané immuable.
    pub fn snapshot(&self) -> SimState {
        self.state.clone()
    }

    /// Configuration.
    pub fn config(&self) -> &SimConfig {
        &self.cfg
    }

    /// Profil de compatibilité résolu.
    pub fn compat(&self) -> CompatProfile {
        self.compat
    }

    /// Événements produits depuis le dernier appel.
    pub fn take_events(&mut self) -> Vec<SimEvent> {
        std::mem::take(&mut self.events)
    }

    /// Valide et applique une commande. Refus typé, jamais de troncature.
    pub fn submit(&mut self, cmd: Command, source: &Source) -> Result<(), RejectReason> {
        let from_source = matches!(source, Source::Track | Source::Replay);
        if self.state.source_active && cmd.is_navigation() && !from_source {
            return Err(RejectReason::ProducerLocked);
        }
        self.apply(cmd)
    }

    fn engine_indices(sel: EngineSel) -> &'static [usize] {
        sel.indices()
    }

    fn apply(&mut self, cmd: Command) -> Result<(), RejectReason> {
        let max_rudder = self.cfg.vessel.max_rudder_deg;
        let legacy = self.compat.direct_speed;
        match cmd {
            Command::Start => {
                if !self.state.running {
                    self.state.running = true;
                    self.state.paused = false;
                    self.events.push(SimEvent::Started);
                }
            }
            Command::Stop => {
                if self.state.running {
                    self.state.running = false;
                    self.state.paused = false;
                    self.events.push(SimEvent::Stopped);
                }
            }
            Command::Pause | Command::Resume | Command::TogglePause => {
                if !self.state.running {
                    return Err(RejectReason::InvalidState {
                        detail: "simulation arrêtée".into(),
                    });
                }
                let pause = match cmd {
                    Command::Pause => true,
                    Command::Resume => false,
                    _ => !self.state.paused,
                };
                if pause != self.state.paused {
                    self.state.paused = pause;
                    self.events.push(if pause {
                        SimEvent::Paused
                    } else {
                        SimEvent::Resumed
                    });
                }
            }
            Command::Reset => {
                let running = self.state.running;
                self.rng = Xoshiro256StarStar::seed_from_u64(self.state.seed);
                let start = self.cfg.start_time_ms.unwrap_or(self.state.time_ms);
                self.state = initial_state(
                    &self.cfg,
                    &self.compat,
                    self.state.seed,
                    start,
                    &mut self.rng,
                );
                self.state.running = running;
                self.nominals = nominals_from(&self.cfg);
                self.commanded_speed = knots_to_mps(self.cfg.initial.speed_kn);
                self.events.push(SimEvent::Reset);
            }
            Command::SetRudder { deg } => {
                let d = range(deg, -max_rudder, max_rudder)?;
                self.set_rudder(d);
            }
            Command::NudgeRudder { deg } => {
                range(deg, -2.0 * max_rudder, 2.0 * max_rudder)?;
                let d = (self.state.vessel.rudder_command + deg).clamp(-max_rudder, max_rudder);
                self.set_rudder(d);
            }
            Command::CenterRudder => self.set_rudder(0.0),
            Command::SetThrottle { engine, value } => {
                let v = range(value, -1.0, 1.0)?;
                for &i in Self::engine_indices(engine) {
                    self.state.engines[i].throttle = v;
                }
                if legacy {
                    self.commanded_speed = self.throttle_to_legacy_speed(v);
                }
            }
            Command::NudgeThrottle { engine, delta } => {
                range(delta, -2.0, 2.0)?;
                let mut last = 0.0;
                for &i in Self::engine_indices(engine) {
                    let e = &mut self.state.engines[i];
                    e.throttle = (e.throttle + delta).clamp(-1.0, 1.0);
                    last = e.throttle;
                }
                if legacy {
                    self.commanded_speed = self.throttle_to_legacy_speed(last);
                }
            }
            Command::SetEngineState { engine, running } => {
                for &i in Self::engine_indices(engine) {
                    self.set_engine(i, running);
                }
            }
            Command::ToggleEngines => {
                let on = !(self.state.engines[0].running || self.state.engines[1].running);
                self.set_engine(0, on);
                self.set_engine(1, on);
            }
            Command::SetHeading { deg } => {
                let h = deg_to_rad(normalize_deg(range(deg, -360.0, 720.0)?));
                self.set_heading(h);
            }
            Command::NudgeHeading { deg } => {
                range(deg, -360.0, 360.0)?;
                let base = if self.state.autopilot.mode == AutopilotMode::Heading && !legacy {
                    self.state.autopilot.heading_target
                } else {
                    self.state.vessel.heading
                };
                self.set_heading(normalize_rad(base + deg_to_rad(deg)));
            }
            Command::SetSpeed { kn } => {
                let v = range(kn, -self.cfg.vessel.max_speed_astern_kn, 70.0)?;
                self.set_speed(knots_to_mps(v));
            }
            Command::NudgeSpeed { kn } => {
                range(kn, -70.0, 70.0)?;
                let current = if legacy {
                    self.commanded_speed
                } else {
                    self.state.engines[0]
                        .throttle
                        .max(self.state.engines[1].throttle)
                        * knots_to_mps(self.cfg.vessel.max_speed_ahead_kn)
                };
                let target = (current + knots_to_mps(kn)).clamp(
                    -knots_to_mps(self.cfg.vessel.max_speed_astern_kn),
                    knots_to_mps(70.0),
                );
                self.set_speed(target);
            }
            Command::SetPosition { position } => {
                if !position.is_valid() {
                    return Err(RejectReason::Invalid {
                        detail: "position hors plage".into(),
                    });
                }
                self.state.vessel.position = position;
            }
            Command::SetDestination { position, name } => {
                if !position.is_valid() {
                    return Err(RejectReason::Invalid {
                        detail: "position hors plage".into(),
                    });
                }
                self.state.route = Route {
                    origin: Some(Waypoint {
                        name: "origin".into(),
                        position: self.state.vessel.position,
                    }),
                    waypoints: vec![Waypoint {
                        name: short_name(name.as_deref().unwrap_or("dest")),
                        position,
                    }],
                    active: 0,
                    geometry: None,
                };
                self.update_route();
                self.events.push(SimEvent::RouteChanged);
            }
            Command::ClearDestination => {
                self.state.route = Route::default();
                if self.state.autopilot.mode == AutopilotMode::Route {
                    self.set_autopilot(AutopilotMode::Heading);
                }
                self.events.push(SimEvent::RouteChanged);
            }
            Command::SetRoute { points } => {
                if points.is_empty() || points.iter().any(|p| !p.is_valid()) {
                    return Err(RejectReason::Invalid {
                        detail: "route vide ou point hors plage".into(),
                    });
                }
                self.state.route = Route {
                    origin: Some(Waypoint {
                        name: "origin".into(),
                        position: self.state.vessel.position,
                    }),
                    waypoints: points
                        .into_iter()
                        .enumerate()
                        .map(|(i, p)| Waypoint {
                            name: format!("WP{:03}", i + 1),
                            position: p,
                        })
                        .collect(),
                    active: 0,
                    geometry: None,
                };
                self.update_route();
                self.events.push(SimEvent::RouteChanged);
            }
            Command::AddWaypoint { position, name } => {
                if !position.is_valid() {
                    return Err(RejectReason::Invalid {
                        detail: "position hors plage".into(),
                    });
                }
                self.add_waypoint(position, name);
            }
            Command::AddWaypointAtVessel => {
                let p = self.state.vessel.position;
                self.add_waypoint(p, None);
            }
            Command::NextWaypoint => {
                let r = &self.state.route;
                if r.active + 1 >= r.waypoints.len() {
                    return Err(RejectReason::InvalidState {
                        detail: "aucun point suivant".into(),
                    });
                }
                self.advance_waypoint();
            }
            Command::MarkPosition => {
                self.state.marked = Some(Waypoint {
                    name: "wpt".into(),
                    position: self.state.vessel.position,
                });
            }
            Command::SetAutopilot { mode } => {
                if mode == AutopilotMode::Route && self.state.route.destination().is_none() {
                    return Err(RejectReason::InvalidState {
                        detail: "aucune destination".into(),
                    });
                }
                if mode == AutopilotMode::Heading
                    && self.state.autopilot.mode != AutopilotMode::Heading
                {
                    self.state.autopilot.heading_target = self.state.vessel.heading;
                }
                self.set_autopilot(mode);
            }
            Command::ToggleAutopilot => {
                if self.state.autopilot.mode == AutopilotMode::Off {
                    self.state.autopilot.heading_target = self.state.vessel.heading;
                    self.set_autopilot(AutopilotMode::Heading);
                } else {
                    self.set_autopilot(AutopilotMode::Off);
                }
            }
            Command::SetAutopilotHeading { deg } => {
                let h = deg_to_rad(normalize_deg(range(deg, -360.0, 720.0)?));
                self.state.autopilot.heading_target = h;
                if self.state.autopilot.mode != AutopilotMode::Heading {
                    self.set_autopilot(AutopilotMode::Heading);
                }
            }
            Command::SetAnchor { on } => self.set_anchor(on),
            Command::ToggleAnchor => {
                let on = !self.state.vessel.anchored;
                self.set_anchor(on);
            }
            Command::SetValue { path, value } => {
                self.write_path(&path, value)?;
            }
            Command::SetOverride { path, value } => {
                self.write_path(&path, value)?;
                self.state.overrides.insert(path.clone(), value);
                self.events
                    .push(SimEvent::OverrideChanged { path, active: true });
            }
            Command::ReleaseOverride { path } => {
                if self.state.overrides.remove(&path).is_none() {
                    return Err(RejectReason::UnknownPath { path });
                }
                self.events.push(SimEvent::OverrideChanged {
                    path,
                    active: false,
                });
            }
            Command::TrackPoint {
                position,
                sog_kn,
                cog_deg,
                altitude,
                time_ms,
            } => {
                if !position.is_valid() {
                    return Err(RejectReason::Invalid {
                        detail: "position hors plage".into(),
                    });
                }
                let v = &mut self.state.vessel;
                v.position = position;
                if let Some(kn) = sog_kn {
                    v.sog = knots_to_mps(kn);
                    v.stw = v.sog;
                    self.commanded_speed = v.sog;
                }
                if let Some(c) = cog_deg {
                    v.cog = deg_to_rad(normalize_deg(c));
                    v.heading = v.cog;
                }
                if let Some(a) = altitude {
                    v.altitude = a;
                }
                v.sway = 0.0;
                v.rate_of_turn = 0.0;
                self.state.source_time_ms = time_ms;
                self.state.source_active = true;
                self.update_route();
            }
            Command::ReleaseSource => {
                self.state.source_active = false;
                self.state.source_time_ms = None;
            }
        }
        Ok(())
    }

    fn throttle_to_legacy_speed(&self, thr: f64) -> f64 {
        (thr * knots_to_mps(self.cfg.vessel.max_speed_ahead_kn)).max(0.0)
    }

    fn set_rudder(&mut self, deg: f64) {
        let d = if self.compat.instant_rudder {
            deg.round()
        } else {
            deg
        };
        self.state.vessel.rudder_command = d;
        if self.state.autopilot.mode != AutopilotMode::Off {
            self.set_autopilot(AutopilotMode::Off);
        }
    }

    fn set_engine(&mut self, i: usize, running: bool) {
        if self.state.engines[i].running != running {
            self.state.engines[i].running = running;
            self.events
                .push(SimEvent::EngineChanged { index: i, running });
        }
    }

    fn set_heading(&mut self, h: f64) {
        if self.compat.direct_speed {
            self.state.vessel.heading = h;
            self.state.vessel.cog = h;
            self.state.autopilot.heading_target = h;
        } else {
            self.state.autopilot.heading_target = h;
            if self.state.autopilot.mode != AutopilotMode::Heading {
                self.set_autopilot(AutopilotMode::Heading);
            }
        }
    }

    fn set_speed(&mut self, mps: f64) {
        if self.compat.direct_speed {
            self.commanded_speed = mps.max(0.0);
        } else {
            let vmax = if mps >= 0.0 {
                knots_to_mps(self.cfg.vessel.max_speed_ahead_kn)
            } else {
                knots_to_mps(self.cfg.vessel.max_speed_astern_kn.max(0.01))
            };
            let thr = (mps / vmax).clamp(-1.0, 1.0);
            for e in &mut self.state.engines {
                e.throttle = thr;
            }
            if thr != 0.0 {
                self.set_engine(0, true);
                self.set_engine(1, true);
            }
        }
    }

    fn set_autopilot(&mut self, mode: AutopilotMode) {
        if self.state.autopilot.mode != mode {
            self.state.autopilot.mode = mode;
            self.events.push(SimEvent::AutopilotChanged { mode });
        }
    }

    fn set_anchor(&mut self, on: bool) {
        if self.state.vessel.anchored != on {
            self.state.vessel.anchored = on;
            self.events.push(SimEvent::AnchorChanged { on });
        }
    }

    fn add_waypoint(&mut self, position: LatLon, name: Option<String>) {
        let r = &mut self.state.route;
        if r.waypoints.is_empty() {
            r.origin = Some(Waypoint {
                name: "origin".into(),
                position: self.state.vessel.position,
            });
            r.active = 0;
        }
        let n = r.waypoints.len() + 1;
        r.waypoints.push(Waypoint {
            name: short_name(&name.unwrap_or_else(|| format!("WP{n:03}"))),
            position,
        });
        self.update_route();
        self.events.push(SimEvent::RouteChanged);
    }

    fn advance_waypoint(&mut self) {
        let r = &mut self.state.route;
        r.origin = r.waypoints.get(r.active).cloned();
        r.active += 1;
        self.update_route();
        self.events.push(SimEvent::RouteChanged);
    }

    fn write_path(&mut self, path: &str, value: f64) -> Result<(), RejectReason> {
        if READ_ONLY.contains(&path) {
            return Err(RejectReason::ReadOnly);
        }
        let Some(&(_, min, max)) = PATHS.iter().find(|(p, _, _)| *p == path) else {
            return Err(RejectReason::UnknownPath { path: path.into() });
        };
        let v = range(value, min, max)?;
        self.set_path_value(path, v);
        match path {
            "env.wind.speedTrue" => self.nominals.wind_speed_kn = v,
            "env.wind.directionTrue" => self.nominals.wind_dir_deg = normalize_deg(v),
            "env.water.depth" => self.nominals.depth_m = v,
            "env.water.temperature" => self.nominals.water_temp_c = v,
            "gnss.hdop" => self.nominals.hdop = v,
            "gnss.vdop" => self.nominals.vdop = v,
            _ => {}
        }
        Ok(())
    }

    fn set_path_value(&mut self, path: &str, v: f64) {
        let s = &mut self.state;
        match path {
            "vessel.headingTrue" => {
                s.vessel.heading = deg_to_rad(normalize_deg(v));
                if self.compat.direct_speed {
                    s.vessel.cog = s.vessel.heading;
                }
            }
            "vessel.speedThroughWater" => s.vessel.stw = knots_to_mps(v),
            "vessel.speedOverGround" => {
                self.commanded_speed = knots_to_mps(v);
                s.vessel.sog = knots_to_mps(v);
                if self.compat.direct_speed {
                    s.vessel.stw = s.vessel.sog;
                }
            }
            "vessel.altitude" => s.vessel.altitude = v,
            "vessel.rudder.command" => {
                let m = self.cfg.vessel.max_rudder_deg;
                s.vessel.rudder_command = v.clamp(-m, m);
            }
            "env.wind.directionTrue" => s.wind.true_direction = deg_to_rad(normalize_deg(v)),
            "env.wind.speedTrue" => s.wind.true_speed = knots_to_mps(v),
            "env.water.depth" => s.water.depth = v,
            "env.water.temperature" => s.water.temperature = celsius_to_kelvin(v),
            "env.current.set" => s.water.current_set = deg_to_rad(normalize_deg(v)),
            "env.current.drift" => s.water.current_drift = knots_to_mps(v),
            "env.magneticVariation" => s.magnetic_variation = deg_to_rad(v),
            "gnss.hdop" => s.gnss.hdop = v,
            "gnss.vdop" => s.gnss.vdop = v,
            "gnss.pdop" => s.gnss.pdop = v,
            "gnss.fix" => {
                s.gnss.fix = v >= 0.5;
                s.gnss.quality = u8::from(s.gnss.fix);
                s.gnss.mode = if s.gnss.fix { 3 } else { 1 };
            }
            "propulsion.p0.rpm" => s.engines[0].rpm = v,
            "propulsion.p1.rpm" => s.engines[1].rpm = v,
            "propulsion.p0.temperature" => s.engines[0].temperature = celsius_to_kelvin(v),
            "propulsion.p1.temperature" => s.engines[1].temperature = celsius_to_kelvin(v),
            _ => unreachable!("chemin validé par PATHS"),
        }
    }

    fn reapply_overrides(&mut self) {
        let ov: Vec<(String, f64)> = self
            .state
            .overrides
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        for (p, v) in ov {
            self.set_path_value(&p, v);
        }
    }

    /// Avance d'un pas de `dt_ms`. Sans effet si arrêté ou en pause.
    pub fn step(&mut self, dt_ms: u32) {
        if !self.state.running || self.state.paused {
            return;
        }
        let dt = f64::from(dt_ms) / 1000.0;
        self.state.time_ms += i64::from(dt_ms);
        self.state.elapsed_ms += i64::from(dt_ms);
        self.state.steps += 1;
        if let Some(t) = self.state.source_time_ms.as_mut() {
            *t += i64::from(dt_ms);
        }

        self.step_engines(dt);
        if !self.state.source_active {
            self.step_autopilot();
            if self.compat.direct_speed {
                self.step_direct(dt);
            } else {
                self.step_dynamic(dt);
            }
        }
        if !self.compat.legacy_noise {
            self.step_modern_noise(dt);
        }
        self.step_gnss(dt);
        self.update_apparent_wind();
        self.update_route();
        self.reapply_overrides();
    }

    /// À appeler à chaque tick de publication : bruit legacy (`applySeed`),
    /// qui dans le legacy s'applique une fois par intervalle de sortie.
    pub fn publish_tick(&mut self) {
        if !self.state.running || self.state.paused || !self.compat.legacy_noise {
            return;
        }
        let d = self.cfg.drift.clone();
        let rng = &mut self.rng;
        let s = &mut self.state;
        if self.compat.direct_speed && !s.source_active {
            self.commanded_speed = knots_to_mps(drift::legacy_step(
                mps_to_knots(self.commanded_speed),
                &d.speed,
                rng,
            ));
            let steering = s.vessel.rudder_command != 0.0 || s.autopilot.mode != AutopilotMode::Off;
            if !steering {
                let h = drift::legacy_step(rad_to_deg(s.vessel.heading), &d.heading, rng);
                s.vessel.heading = deg_to_rad(h);
                s.vessel.cog = s.vessel.heading;
            }
        }
        s.gnss.hdop = drift::legacy_step(s.gnss.hdop, &d.hdop, rng);
        s.gnss.pdop = drift::legacy_step(s.gnss.pdop, &d.pdop, rng);
        s.gnss.vdop = drift::legacy_step(s.gnss.vdop, &d.vdop, rng);
        let wd = drift::legacy_step(rad_to_deg(s.wind.true_direction), &d.wind_direction, rng);
        s.wind.true_direction = deg_to_rad(wd);
        let ws = drift::legacy_step(mps_to_knots(s.wind.true_speed), &d.wind_speed, rng);
        s.wind.true_speed = knots_to_mps(ws);
        s.water.depth = drift::legacy_step(s.water.depth, &d.depth, rng);
        let wt = drift::legacy_step(
            kelvin_to_celsius(s.water.temperature),
            &d.water_temperature,
            rng,
        );
        s.water.temperature = celsius_to_kelvin(wt);
        for e in &mut s.engines {
            e.rpm = drift::legacy_step(e.rpm, &d.rpm, rng);
            let t =
                drift::legacy_step(kelvin_to_celsius(e.temperature), &d.engine_temperature, rng);
            e.temperature = celsius_to_kelvin(t);
        }
        self.update_apparent_wind();
        self.reapply_overrides();
    }

    fn step_engines(&mut self, dt: f64) {
        let v = &self.cfg.vessel;
        let water_k = self.state.water.temperature;
        let legacy_rpm = self.compat.legacy_noise;
        for e in &mut self.state.engines {
            if legacy_rpm {
                // Legacy : le régime ne dépend que de sa dérive ; la charge et
                // le pas suivent la commande pour que la sortie reste lisible.
                e.load = e.throttle.abs();
                e.pitch_percent = pitch(e.throttle);
                continue;
            }
            let target = if e.running {
                v.idle_rpm + e.throttle.abs() * (v.max_rpm - v.idle_rpm)
            } else {
                0.0
            };
            let max_delta = v.rpm_rate * dt;
            e.rpm += (target - e.rpm).clamp(-max_delta, max_delta);
            e.load = if e.running { e.throttle.abs() } else { 0.0 };
            e.pitch_percent = if e.running { pitch(e.throttle) } else { 0.0 };
            let target_k = if e.running {
                celsius_to_kelvin(80.0 + 10.0 * e.load)
            } else {
                water_k
            };
            let tau = if e.running { 90.0 } else { 900.0 };
            e.temperature += (target_k - e.temperature) * (1.0 - (-dt / tau).exp());
        }
    }

    fn step_autopilot(&mut self) {
        let target = match self.state.autopilot.mode {
            AutopilotMode::Off => return,
            AutopilotMode::Heading => self.state.autopilot.heading_target,
            AutopilotMode::Route => match &self.state.route.geometry {
                Some(g) => g.bearing_to_dest,
                None => return,
            },
        };
        let v = &self.state.vessel;
        let err = angle_diff_deg(rad_to_deg(v.heading), rad_to_deg(target));
        let r = rad_to_deg(v.rate_of_turn);
        let limit = 25.0f64.min(self.cfg.vessel.max_rudder_deg);
        let mut cmd = (1.2 * err - 12.0 * r).clamp(-limit, limit);
        if self.compat.instant_rudder {
            cmd = cmd.round();
        }
        self.state.vessel.rudder_command = cmd;
    }

    fn move_rudder(&mut self, dt: f64) {
        let v = &mut self.state.vessel;
        if self.compat.instant_rudder {
            v.rudder_angle = v.rudder_command;
        } else {
            let max = self.cfg.vessel.rudder_rate_deg_s * dt;
            v.rudder_angle += (v.rudder_command - v.rudder_angle).clamp(-max, max);
        }
    }

    fn yaw(&mut self, target: f64, dt: f64) {
        let v = &mut self.state.vessel;
        if self.compat.stop_turn_at_zero {
            v.rate_of_turn = target;
        } else {
            let k = 1.0 - 0.5f64.powf(dt * 1000.0 / self.cfg.vessel.yaw_half_life_ms.max(1.0));
            v.rate_of_turn += (target - v.rate_of_turn) * k;
        }
        v.heading = normalize_rad(v.heading + v.rate_of_turn * dt);
    }

    fn step_direct(&mut self, dt: f64) {
        self.move_rudder(dt);
        let u = self.commanded_speed;
        let delta = self.state.vessel.rudder_angle;
        let target = if self.compat.stop_turn_at_zero {
            if delta == 0.0 {
                0.0
            } else {
                // updateHeading : speed_kn / (R / 1852) °/min.
                let deg_per_s = mps_to_knots(u) / (legacy_turn_radius(delta) / 1852.0) / 60.0;
                deg_to_rad(deg_per_s) * delta.signum()
            }
        } else {
            u * curvature(delta) * delta.signum()
        };
        self.yaw(target, dt);
        let v = &mut self.state.vessel;
        v.acceleration = (u - v.stw) / dt;
        v.stw = u;
        v.sway = 0.0;
        v.sog = if v.anchored { 0.0 } else { u };
        v.cog = v.heading;
        if !v.anchored {
            v.position = v.position.destination(v.cog, v.sog * dt);
        }
    }

    fn step_dynamic(&mut self, dt: f64) {
        self.move_rudder(dt);
        let vc = &self.cfg.vessel;
        let (idle, maxr) = (vc.idle_rpm, vc.max_rpm);
        let mut sum = 0.0;
        for e in &self.state.engines {
            if e.running && e.throttle.abs() > 0.02 {
                let r = ((e.rpm - idle) / (maxr - idle)).clamp(0.0, 1.0);
                // En prise, au moins une poussée de ralenti ; à l'équilibre,
                // vitesse = commande × vitesse maximale.
                sum += e.throttle.signum() * r.max(0.08);
            }
        }
        let frac = sum / 2.0;
        let anchored = self.state.vessel.anchored;
        let target_u = if anchored {
            0.0
        } else if frac >= 0.0 {
            frac * knots_to_mps(vc.max_speed_ahead_kn)
        } else {
            frac * knots_to_mps(vc.max_speed_astern_kn)
        };
        let u = self.state.vessel.stw;
        let tau = if anchored {
            5.0
        } else if target_u.abs() > u.abs() && target_u * u >= 0.0 {
            vc.accel_time_s
        } else {
            vc.decel_time_s
        };
        let new_u = u + (target_u - u) * (1.0 - (-dt / tau).exp());
        let leeway = vc.leeway_coeff;
        {
            let v = &mut self.state.vessel;
            v.acceleration = (new_u - u) / dt;
            v.stw = new_u;
        }
        let delta = self.state.vessel.rudder_angle;
        let target_r = new_u * curvature(delta) * delta.signum();
        self.yaw(target_r, dt);

        let s = &mut self.state;
        let h = s.vessel.heading;
        let to = s.wind.true_direction + PI;
        let (we, wn) = (s.wind.true_speed * to.sin(), s.wind.true_speed * to.cos());
        let cross = we * h.cos() - wn * h.sin();
        let sway_target = if anchored { 0.0 } else { leeway * cross };
        s.vessel.sway += (sway_target - s.vessel.sway) * (1.0 - (-dt / 10.0).exp());
        let (u, sw) = (s.vessel.stw, s.vessel.sway);
        let (mut ge, mut gn) = (u * h.sin() + sw * h.cos(), u * h.cos() - sw * h.sin());
        ge += s.water.current_drift * s.water.current_set.sin();
        gn += s.water.current_drift * s.water.current_set.cos();
        if anchored {
            ge = 0.0;
            gn = 0.0;
        }
        let sog = ge.hypot(gn);
        s.vessel.sog = sog;
        s.vessel.cog = if sog > 1e-6 {
            normalize_rad(ge.atan2(gn))
        } else {
            h
        };
        if sog > 0.0 {
            s.vessel.position = s.vessel.position.destination(s.vessel.cog, sog * dt);
        }
    }

    fn step_modern_noise(&mut self, dt: f64) {
        let d = &self.cfg.drift;
        let n = &self.nominals;
        let rng = &mut self.rng;
        let s = &mut self.state;
        let ws = drift::modern_step(
            mps_to_knots(s.wind.true_speed),
            n.wind_speed_kn,
            &d.wind_speed,
            dt,
            rng,
        );
        s.wind.true_speed = knots_to_mps(ws);
        let wd = drift::modern_step_deg(
            rad_to_deg(s.wind.true_direction),
            n.wind_dir_deg,
            &d.wind_direction,
            dt,
            rng,
        );
        s.wind.true_direction = deg_to_rad(wd);
        s.water.depth = drift::modern_step(s.water.depth, n.depth_m, &d.depth, dt, rng);
        let wt = drift::modern_step(
            kelvin_to_celsius(s.water.temperature),
            n.water_temp_c,
            &d.water_temperature,
            dt,
            rng,
        );
        s.water.temperature = celsius_to_kelvin(wt);
        s.gnss.hdop = drift::modern_step(s.gnss.hdop, n.hdop, &d.hdop, dt, rng);
        s.gnss.vdop = drift::modern_step(s.gnss.vdop, n.vdop, &d.vdop, dt, rng);
        s.gnss.pdop = s.gnss.hdop.hypot(s.gnss.vdop);
    }

    fn step_gnss(&mut self, dt: f64) {
        if self.compat.legacy_sentences {
            return;
        }
        for sat in &mut self.state.gnss.satellites {
            sat.azimuth = normalize_deg(sat.azimuth + 0.004 * dt);
        }
    }

    fn update_apparent_wind(&mut self) {
        let s = &mut self.state;
        let to = s.wind.true_direction + PI;
        let (we, wn) = (s.wind.true_speed * to.sin(), s.wind.true_speed * to.cos());
        let (ge, gn) = (
            s.vessel.sog * s.vessel.cog.sin(),
            s.vessel.sog * s.vessel.cog.cos(),
        );
        let (ae, an) = (we - ge, wn - gn);
        s.wind.apparent_speed = ae.hypot(an);
        let from = normalize_rad((-ae).atan2(-an));
        let rel = normalize_rad(from - s.vessel.heading);
        s.wind.apparent_angle = if rel > PI { rel - 2.0 * PI } else { rel };
    }

    fn update_route(&mut self) {
        let radius = self.cfg.arrival_radius_m;
        let s = &mut self.state;
        let Some(dest) = s.route.destination().cloned() else {
            s.route.geometry = None;
            return;
        };
        let pos = s.vessel.position;
        let origin = s.route.origin.as_ref().map_or(pos, |o| o.position);
        let leg = origin.distance_to(dest.position);
        let xte = if leg > 1e-3 {
            pos.cross_track_distance(origin, dest.position)
        } else {
            0.0
        };
        let dtg = pos.distance_to(dest.position);
        let brg_c = pos.bearing_to(dest.position);
        let g = RouteGeometry {
            xte,
            dtg,
            bearing_origin_to_dest: if leg > 1e-3 {
                origin.bearing_to(dest.position)
            } else {
                brg_c
            },
            bearing_to_dest: brg_c,
            closing_speed: s.vessel.sog * (brg_c - s.vessel.cog).cos(),
            arrived: dtg <= radius,
            perpendicular_passed: leg > 1e-3
                && pos.along_track_distance(origin, dest.position) >= leg,
        };
        let arrived = g.arrived;
        s.route.geometry = Some(g);
        if arrived && s.autopilot.mode == AutopilotMode::Route {
            let idx = s.route.active;
            if idx + 1 < s.route.waypoints.len() {
                self.events.push(SimEvent::WaypointReached { index: idx });
                self.advance_waypoint();
            }
        }
    }
}

fn pitch(throttle: f64) -> f64 {
    if throttle.abs() < 0.02 {
        0.0
    } else {
        100.0 * throttle.signum()
    }
}

fn short_name(n: &str) -> String {
    n.chars()
        .filter(|c| *c != ',' && *c != '*')
        .take(8)
        .collect()
}

fn nominals_from(cfg: &SimConfig) -> Nominals {
    Nominals {
        wind_speed_kn: cfg.environment.wind_speed_kn,
        wind_dir_deg: cfg.environment.wind_direction_deg,
        depth_m: cfg.environment.depth_m,
        water_temp_c: cfg.environment.water_temperature_c,
        hdop: cfg.gnss.hdop,
        vdop: cfg.gnss.vdop,
    }
}

fn initial_state(
    cfg: &SimConfig,
    compat: &CompatProfile,
    seed: u64,
    start_ms: i64,
    rng: &mut Xoshiro256StarStar,
) -> SimState {
    let i = &cfg.initial;
    let env = &cfg.environment;
    let heading = deg_to_rad(normalize_deg(i.heading_deg));
    let speed = knots_to_mps(i.speed_kn);
    let mut sat_rng = rng.fork();
    let (satellites, used) = if compat.legacy_sentences {
        let prns = [8u8, 11, 15, 22];
        let sats = prns
            .iter()
            .enumerate()
            .map(|(k, &prn)| Satellite {
                prn,
                elevation: 30.0 + 10.0 * k as f64,
                azimuth: 90.0 * k as f64,
                snr: 40.0,
            })
            .collect();
        (sats, prns.to_vec())
    } else {
        let mut prns: Vec<u8> = (1..=32).collect();
        let mut sats = Vec::new();
        for _ in 0..cfg.gnss.satellites {
            let k = sat_rng.below(prns.len() as u64) as usize;
            let prn = prns.swap_remove(k);
            sats.push(Satellite {
                prn,
                elevation: sat_rng.range_f64(5.0, 85.0).round(),
                azimuth: sat_rng.range_f64(0.0, 359.0).round(),
                snr: sat_rng.range_f64(30.0, 48.0).round(),
            });
        }
        sats.sort_by_key(|s| s.prn);
        let used = sats
            .iter()
            .filter(|s| s.elevation > 10.0)
            .take(12)
            .map(|s| s.prn)
            .collect();
        (sats, used)
    };
    // Modern : une vitesse initiale non nulle est tenue par la propulsion.
    let (running, throttle) = if !compat.direct_speed && i.throttle == 0.0 && i.speed_kn > 0.0 {
        (true, (i.speed_kn / cfg.vessel.max_speed_ahead_kn).min(1.0))
    } else {
        (
            i.engines_running || (!compat.direct_speed && i.throttle != 0.0),
            i.throttle,
        )
    };
    let vc = &cfg.vessel;
    let rpm0 = if compat.legacy_noise {
        vc.idle_rpm
    } else if running {
        vc.idle_rpm + throttle.abs() * (vc.max_rpm - vc.idle_rpm)
    } else {
        0.0
    };
    let engine = |id: &str, label: &str| Engine {
        id: id.into(),
        label: label.into(),
        running,
        throttle,
        rpm: rpm0,
        temperature: celsius_to_kelvin(if compat.legacy_noise {
            85.0
        } else if running {
            80.0 + 10.0 * throttle.abs()
        } else {
            env.water_temperature_c
        }),
        load: if running { throttle.abs() } else { 0.0 },
        pitch_percent: if running { pitch(throttle) } else { 0.0 },
    };
    let fix = cfg.gnss.fix;
    let mut st = SimState {
        time_ms: start_ms,
        elapsed_ms: 0,
        steps: 0,
        running: false,
        paused: false,
        seed,
        vessel: Vessel {
            position: i.position,
            altitude: i.altitude_m,
            heading,
            stw: speed,
            sway: 0.0,
            sog: speed,
            cog: heading,
            acceleration: 0.0,
            rate_of_turn: 0.0,
            rudder_command: 0.0,
            rudder_angle: 0.0,
            anchored: false,
        },
        engines: [
            engine("p0", &cfg.vessel.engine_labels[0]),
            engine("p1", &cfg.vessel.engine_labels[1]),
        ],
        wind: Wind {
            true_direction: deg_to_rad(normalize_deg(env.wind_direction_deg)),
            true_speed: knots_to_mps(env.wind_speed_kn),
            apparent_angle: 0.0,
            apparent_speed: 0.0,
        },
        water: Water {
            depth: env.depth_m,
            temperature: celsius_to_kelvin(env.water_temperature_c),
            current_set: deg_to_rad(normalize_deg(env.current_set_deg)),
            current_drift: knots_to_mps(env.current_drift_kn),
        },
        gnss: Gnss {
            fix,
            quality: u8::from(fix),
            mode: if fix { 3 } else { 1 },
            hdop: cfg.gnss.hdop,
            vdop: cfg.gnss.vdop,
            pdop: if compat.legacy_noise {
                cfg.gnss.pdop
            } else {
                cfg.gnss.hdop.hypot(cfg.gnss.vdop)
            },
            satellites,
            used,
        },
        route: Route::default(),
        autopilot: Autopilot {
            mode: AutopilotMode::Off,
            heading_target: heading,
        },
        marked: None,
        magnetic_variation: deg_to_rad(env.magnetic_variation_deg),
        overrides: BTreeMap::new(),
        source_time_ms: None,
        source_active: false,
    };
    // Vent apparent initial cohérent.
    let to = st.wind.true_direction + PI;
    let (we, wn) = (
        st.wind.true_speed * to.sin() - speed * heading.sin(),
        st.wind.true_speed * to.cos() - speed * heading.cos(),
    );
    st.wind.apparent_speed = we.hypot(wn);
    let rel = normalize_rad(normalize_rad((-we).atan2(-wn)) - heading);
    st.wind.apparent_angle = if rel > PI { rel - 2.0 * PI } else { rel };
    st
}
