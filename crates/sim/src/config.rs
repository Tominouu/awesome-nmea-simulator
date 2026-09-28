//! Configuration de la simulation. Les valeurs sont exprimées dans les unités
//! d'usage (nœuds, degrés, °C) pour rester lisibles ; la conversion vers le SI
//! se fait à la construction du simulateur.

use nmeasim_core::geo::LatLon;
use serde::{Deserialize, Serialize};

use crate::compat::CompatConfig;

/// Configuration complète de la simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SimConfig {
    /// Graine ; `None` = graine tirée au démarrage, puis journalisée.
    pub seed: Option<u64>,
    /// Pas d'intégration, ms.
    pub step_ms: u32,
    /// Heure de départ de la simulation (ms epoch) ; `None` = heure murale.
    pub start_time_ms: Option<i64>,
    /// Compatibilité (D1).
    pub compatibility: CompatConfig,
    /// Caractéristiques du navire.
    pub vessel: VesselConfig,
    /// État initial.
    pub initial: InitialConfig,
    /// Environnement initial.
    pub environment: EnvironmentConfig,
    /// GNSS.
    pub gnss: GnssConfig,
    /// Bruit des grandeurs dérivantes (D3).
    pub drift: DriftConfig,
    /// Rayon d'arrivée sur un waypoint, mètres.
    pub arrival_radius_m: f64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            seed: None,
            step_ms: 20,
            start_time_ms: None,
            compatibility: CompatConfig::default(),
            vessel: VesselConfig::default(),
            initial: InitialConfig::default(),
            environment: EnvironmentConfig::default(),
            gnss: GnssConfig::default(),
            drift: DriftConfig::default(),
            arrival_radius_m: 100.0,
        }
    }
}

/// Paramètres physiques du navire (modèle simple, décision 010).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VesselConfig {
    /// Nom affiché.
    pub name: String,
    /// Longueur hors tout, m (VBW arrière, D5).
    pub length_m: f64,
    /// Vitesse maximale en marche avant, nœuds.
    pub max_speed_ahead_kn: f64,
    /// Vitesse maximale en marche arrière, nœuds.
    pub max_speed_astern_kn: f64,
    /// Constante de temps d'accélération, s.
    pub accel_time_s: f64,
    /// Constante de temps de décélération (inertie), s.
    pub decel_time_s: f64,
    /// Vitesse de barre, °/s (D1).
    pub rudder_rate_deg_s: f64,
    /// Angle de barre maximal, degrés.
    pub max_rudder_deg: f64,
    /// Demi-vie du taux de giration, ms (D2).
    pub yaw_half_life_ms: f64,
    /// Régime de ralenti, tr/min.
    pub idle_rpm: f64,
    /// Régime maximal, tr/min.
    pub max_rpm: f64,
    /// Vitesse de variation du régime, tr/min par seconde.
    pub rpm_rate: f64,
    /// Coefficient de dérive au vent (fraction de la composante travers).
    pub leeway_coeff: f64,
    /// Libellés des moteurs (Signal K `propulsion.<id>.label`).
    pub engine_labels: [String; 2],
}

impl Default for VesselConfig {
    fn default() -> Self {
        Self {
            name: "NMEASIM".into(),
            length_m: 20.0,
            max_speed_ahead_kn: 12.0,
            max_speed_astern_kn: 4.0,
            accel_time_s: 12.0,
            decel_time_s: 25.0,
            rudder_rate_deg_s: 8.0,
            max_rudder_deg: 40.0,
            yaw_half_life_ms: 1500.0,
            idle_rpm: 600.0,
            max_rpm: 3600.0,
            rpm_rate: 800.0,
            leeway_coeff: 0.03,
            engine_labels: ["Port Engine".into(), "Starboard Engine".into()],
        }
    }
}

/// État initial du navire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InitialConfig {
    /// Position.
    pub position: LatLon,
    /// Altitude de l'antenne, m.
    pub altitude_m: f64,
    /// Cap vrai, degrés.
    pub heading_deg: f64,
    /// Vitesse, nœuds (vitesse commandée en legacy, vitesse initiale en modern).
    pub speed_kn: f64,
    /// Moteurs en marche au départ.
    pub engines_running: bool,
    /// Commande de propulsion initiale, `[-1, 1]`.
    pub throttle: f64,
}

impl Default for InitialConfig {
    fn default() -> Self {
        Self {
            position: LatLon::new(-35.0, 138.5),
            altitude_m: 2.0,
            heading_deg: 15.0,
            speed_kn: 5.0,
            engines_running: false,
            throttle: 0.0,
        }
    }
}

/// Environnement initial.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EnvironmentConfig {
    /// Direction d'où vient le vent vrai, degrés.
    pub wind_direction_deg: f64,
    /// Vitesse du vent vrai, nœuds.
    pub wind_speed_kn: f64,
    /// Profondeur sous transducteur, m.
    pub depth_m: f64,
    /// Température de l'eau, °C.
    pub water_temperature_c: f64,
    /// Direction vers laquelle porte le courant, degrés.
    pub current_set_deg: f64,
    /// Vitesse du courant, nœuds.
    pub current_drift_kn: f64,
    /// Déclinaison magnétique, degrés (est positive).
    pub magnetic_variation_deg: f64,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            wind_direction_deg: 285.0,
            wind_speed_kn: 11.0,
            depth_m: 8.0,
            water_temperature_c: 7.5,
            current_set_deg: 0.0,
            current_drift_kn: 0.0,
            magnetic_variation_deg: 0.0,
        }
    }
}

/// GNSS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GnssConfig {
    /// Fix disponible.
    pub fix: bool,
    /// HDOP initial.
    pub hdop: f64,
    /// VDOP initial.
    pub vdop: f64,
    /// PDOP initial (legacy seulement ; en modern, dérivé de HDOP et VDOP).
    pub pdop: f64,
    /// Nombre de satellites simulés en modern.
    pub satellites: u8,
}

impl Default for GnssConfig {
    fn default() -> Self {
        Self {
            fix: true,
            hdop: 1.5,
            vdop: 1.5,
            pdop: 1.5,
            satellites: 10,
        }
    }
}

/// Spécification de bruit d'une grandeur (D3).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DriftSpec {
    /// Grandeur bruitée.
    pub enabled: bool,
    /// Borne basse, unité d'usage.
    pub min: f64,
    /// Borne haute, unité d'usage.
    pub max: f64,
    /// Amplitude absolue par racine de seconde (modern).
    pub amplitude: f64,
    /// Demi-vie du rappel vers le nominal, ms (modern).
    pub half_life_ms: f64,
    /// Pas relatif montant (legacy).
    pub plus: f64,
    /// Pas relatif descendant (legacy).
    pub minus: f64,
}

impl Default for DriftSpec {
    fn default() -> Self {
        Self {
            enabled: true,
            min: 0.0,
            max: 1.0,
            amplitude: 0.0,
            half_life_ms: 60_000.0,
            plus: 0.0,
            minus: 0.0,
        }
    }
}

impl DriftSpec {
    fn new(min: f64, max: f64, amplitude: f64, rel: f64) -> Self {
        Self {
            enabled: true,
            min,
            max,
            amplitude,
            half_life_ms: 60_000.0,
            plus: rel,
            minus: rel,
        }
    }
}

/// Bruit par grandeur. Défauts legacy = seeds de `docs/02` §2.4.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DriftConfig {
    /// Vitesse (legacy seulement : en modern, la vitesse vient de la physique), kn.
    pub speed: DriftSpec,
    /// Cap (legacy seulement), degrés.
    pub heading: DriftSpec,
    /// HDOP.
    pub hdop: DriftSpec,
    /// VDOP.
    pub vdop: DriftSpec,
    /// PDOP (legacy seulement).
    pub pdop: DriftSpec,
    /// Direction du vent, degrés.
    pub wind_direction: DriftSpec,
    /// Vitesse du vent, nœuds.
    pub wind_speed: DriftSpec,
    /// Profondeur, m.
    pub depth: DriftSpec,
    /// Température de l'eau, °C.
    pub water_temperature: DriftSpec,
    /// Régime moteur (legacy seulement), tr/min.
    pub rpm: DriftSpec,
    /// Température moteur, °C.
    pub engine_temperature: DriftSpec,
}

impl Default for DriftConfig {
    fn default() -> Self {
        Self {
            speed: DriftSpec::new(0.0, 70.0, 0.0, 0.01),
            heading: DriftSpec::new(0.0, 359.0, 0.0, 0.05),
            hdop: DriftSpec::new(0.5, 5.0, 0.03, 0.07),
            vdop: DriftSpec::new(0.5, 5.0, 0.03, 0.07),
            pdop: DriftSpec::new(0.5, 5.0, 0.03, 0.07),
            wind_direction: DriftSpec::new(0.0, 359.0, 1.0, 0.05),
            wind_speed: DriftSpec::new(2.0, 40.0, 0.2, 0.2),
            depth: DriftSpec::new(1.0, 100.0, 0.05, 0.05),
            water_temperature: DriftSpec::new(0.0, 25.0, 0.005, 0.01),
            rpm: DriftSpec::new(600.0, 6000.0, 0.0, 0.02),
            engine_temperature: DriftSpec::new(85.0, 115.0, 0.02, 0.01),
        }
    }
}

/// Erreur de configuration explicite.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigError {
    /// Champ fautif (chemin pointé).
    pub field: String,
    /// Raison.
    pub reason: String,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

impl std::error::Error for ConfigError {}

fn check(ok: bool, field: &str, reason: &str) -> Result<(), ConfigError> {
    if ok {
        Ok(())
    } else {
        Err(ConfigError {
            field: field.into(),
            reason: reason.into(),
        })
    }
}

impl SimConfig {
    /// Valide la configuration ; toute incohérence est une erreur explicite.
    pub fn validate(&self) -> Result<(), ConfigError> {
        check(
            (1..=1000).contains(&self.step_ms),
            "simulation.stepMs",
            "doit être dans 1..=1000",
        )?;
        let v = &self.vessel;
        check(v.length_m > 0.0, "vessel.lengthM", "doit être > 0")?;
        check(
            v.max_speed_ahead_kn > 0.0,
            "vessel.maxSpeedAheadKn",
            "doit être > 0",
        )?;
        check(
            v.max_speed_astern_kn >= 0.0,
            "vessel.maxSpeedAsternKn",
            "doit être >= 0",
        )?;
        check(
            v.accel_time_s > 0.0 && v.decel_time_s > 0.0,
            "vessel.accelTimeS",
            "constantes de temps > 0",
        )?;
        check(
            v.rudder_rate_deg_s > 0.0,
            "vessel.rudderRateDegS",
            "doit être > 0",
        )?;
        check(
            (1.0..=90.0).contains(&v.max_rudder_deg),
            "vessel.maxRudderDeg",
            "doit être dans 1..=90",
        )?;
        check(
            v.idle_rpm >= 0.0 && v.max_rpm > v.idle_rpm,
            "vessel.maxRpm",
            "doit être > idleRpm",
        )?;
        check(
            self.initial.position.is_valid(),
            "initial.position",
            "latitude/longitude hors plage",
        )?;
        check(
            self.initial.speed_kn.is_finite() && self.initial.speed_kn >= 0.0,
            "initial.speedKn",
            "doit être >= 0",
        )?;
        check(
            (-1.0..=1.0).contains(&self.initial.throttle),
            "initial.throttle",
            "doit être dans [-1, 1]",
        )?;
        check(
            self.environment.depth_m > 0.0,
            "environment.depthM",
            "doit être > 0",
        )?;
        check(
            self.environment.wind_speed_kn >= 0.0,
            "environment.windSpeedKn",
            "doit être >= 0",
        )?;
        check(
            self.environment.current_drift_kn >= 0.0,
            "environment.currentDriftKn",
            "doit être >= 0",
        )?;
        check(self.gnss.satellites <= 32, "gnss.satellites", "au plus 32")?;
        check(
            self.arrival_radius_m > 0.0,
            "arrivalRadiusM",
            "doit être > 0",
        )?;
        Ok(())
    }
}
