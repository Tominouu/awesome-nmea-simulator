//! Scénarios : un fichier JSON qui capture position, navire, environnement,
//! graine, route, destination et paramètres réseau. Enregistrer depuis un état
//! en cours fige la position, le cap, la vitesse et l'environnement courants
//! comme état initial.

use std::path::Path;

use nmeasim_core::geo::LatLon;
use nmeasim_core::units::{kelvin_to_celsius, mps_to_knots, normalize_deg, rad_to_deg};
use nmeasim_encode::nmea::NmeaOptions;
use nmeasim_encode::viewsync::ViewSyncOptions;
use nmeasim_sim::SimConfig;
use nmeasim_sim::command::Command;
use nmeasim_sim::state::{AutopilotMode, SimState};
use serde::{Deserialize, Serialize};

use crate::config::{AppConfig, ConfigError, TransportConfig};

/// Point de route nommé.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioWaypoint {
    /// Nom.
    pub name: String,
    /// Position.
    pub position: LatLon,
}

/// Scénario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Scenario {
    /// Version du format.
    pub schema: u32,
    /// Nom.
    pub name: String,
    /// Description.
    pub description: String,
    /// Simulation : graine, navire, état initial, environnement, compatibilité.
    pub simulation: SimConfig,
    /// Intervalle de sortie, ms.
    pub interval_ms: u32,
    /// Options NMEA (talkers, phrases, préfixe, AIS).
    pub nmea: NmeaOptions,
    /// Transports ; `None` = conserver ceux de la configuration.
    pub transports: Option<Vec<TransportConfig>>,
    /// ViewSync ; `None` = conserver.
    pub viewsync: Option<ViewSyncOptions>,
    /// Route (points successifs).
    pub route: Vec<ScenarioWaypoint>,
    /// Pilote engagé au lancement.
    pub autopilot: AutopilotMode,
    /// Démarrage automatique au chargement.
    pub auto_start: bool,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            schema: 1,
            name: "Nouveau scénario".into(),
            description: String::new(),
            simulation: SimConfig::default(),
            interval_ms: 1000,
            nmea: NmeaOptions::default(),
            transports: None,
            viewsync: None,
            route: vec![],
            autopilot: AutopilotMode::Off,
            auto_start: false,
        }
    }
}

impl Scenario {
    /// Capture la configuration et l'état courants.
    pub fn capture(name: &str, cfg: &AppConfig, s: &SimState) -> Self {
        let mut sim = cfg.simulation.clone();
        sim.seed = Some(s.seed);
        sim.start_time_ms = None;
        sim.initial.position = s.vessel.position;
        sim.initial.altitude_m = s.vessel.altitude;
        sim.initial.heading_deg = normalize_deg(rad_to_deg(s.vessel.heading));
        sim.initial.speed_kn = mps_to_knots(s.vessel.stw.max(0.0));
        sim.initial.throttle = s.engines[0].throttle;
        sim.initial.engines_running = s.engines[0].running || s.engines[1].running;
        sim.environment.wind_direction_deg = normalize_deg(rad_to_deg(s.wind.true_direction));
        sim.environment.wind_speed_kn = mps_to_knots(s.wind.true_speed);
        sim.environment.depth_m = s.water.depth;
        sim.environment.water_temperature_c = kelvin_to_celsius(s.water.temperature);
        sim.environment.current_set_deg = normalize_deg(rad_to_deg(s.water.current_set));
        sim.environment.current_drift_kn = mps_to_knots(s.water.current_drift);
        Self {
            schema: 1,
            name: name.into(),
            description: String::new(),
            simulation: sim,
            interval_ms: cfg.output.interval_ms,
            nmea: cfg.output.nmea.clone(),
            transports: Some(cfg.transports.clone()),
            viewsync: Some(cfg.viewsync.clone()),
            route: s
                .route
                .waypoints
                .iter()
                .map(|w| ScenarioWaypoint {
                    name: w.name.clone(),
                    position: w.position,
                })
                .collect(),
            autopilot: s.autopilot.mode,
            auto_start: false,
        }
    }

    /// Configuration résultante (base + scénario).
    pub fn apply_to(&self, base: &AppConfig) -> AppConfig {
        let mut c = base.clone();
        c.simulation = self.simulation.clone();
        c.output.interval_ms = self.interval_ms;
        c.output.nmea = self.nmea.clone();
        if let Some(t) = &self.transports {
            c.transports = t.clone();
        }
        if let Some(v) = &self.viewsync {
            c.viewsync = v.clone();
        }
        c
    }

    /// Commandes à soumettre après application (route, pilote).
    pub fn commands(&self) -> Vec<Command> {
        let mut v = vec![];
        if !self.route.is_empty() {
            v.push(Command::SetRoute {
                points: self.route.iter().map(|w| w.position).collect(),
            });
        }
        if self.autopilot != AutopilotMode::Off
            && (!self.route.is_empty() || self.autopilot == AutopilotMode::Heading)
        {
            v.push(Command::SetAutopilot {
                mode: self.autopilot,
            });
        }
        if self.auto_start {
            v.push(Command::Start);
        }
        v
    }
}

/// Lit un scénario.
pub fn load(path: &Path) -> Result<Scenario, ConfigError> {
    let t = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::new(path.display().to_string(), e.to_string()))?;
    let s: Scenario = serde_json::from_str(&t)
        .map_err(|e| ConfigError::new(path.display().to_string(), e.to_string()))?;
    if s.schema != 1 {
        return Err(ConfigError::new(
            "schema",
            format!("version de scénario {} inconnue", s.schema),
        ));
    }
    s.simulation
        .validate()
        .map_err(|e| ConfigError::new(format!("simulation.{}", e.field), e.reason))?;
    Ok(s)
}

/// Enregistre un scénario.
pub fn save(path: &Path, s: &Scenario) -> Result<(), ConfigError> {
    let j = serde_json::to_string_pretty(s)
        .map_err(|e| ConfigError::new("(scénario)", e.to_string()))?;
    std::fs::write(path, j).map_err(|e| ConfigError::new(path.display().to_string(), e.to_string()))
}
