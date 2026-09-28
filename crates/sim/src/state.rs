//! État du monde simulé. Unités SI : m, s, m/s, rad, K. Positions en degrés
//! décimaux. Temps en ms epoch.

use std::collections::BTreeMap;

use nmeasim_core::geo::LatLon;
use serde::{Deserialize, Serialize};

/// Navire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vessel {
    /// Position.
    pub position: LatLon,
    /// Altitude de l'antenne, m.
    pub altitude: f64,
    /// Cap vrai, rad `[0, 2π)`.
    pub heading: f64,
    /// Vitesse surface longitudinale (STW), m/s, négative en arrière.
    pub stw: f64,
    /// Vitesse surface transversale (dérive), m/s, positive vers tribord.
    pub sway: f64,
    /// Vitesse fond, m/s.
    pub sog: f64,
    /// Route fond, rad `[0, 2π)`.
    pub cog: f64,
    /// Accélération longitudinale, m/s².
    pub acceleration: f64,
    /// Taux de giration, rad/s, positif sur tribord.
    pub rate_of_turn: f64,
    /// Consigne de barre, degrés, positive sur tribord.
    pub rudder_command: f64,
    /// Angle réel de barre, degrés.
    pub rudder_angle: f64,
    /// Au mouillage.
    pub anchored: bool,
}

/// Moteur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Engine {
    /// Identifiant Signal K (`p0`, `p1`).
    pub id: String,
    /// Libellé.
    pub label: String,
    /// En marche.
    pub running: bool,
    /// Commande `[-1, 1]` : négatif = marche arrière.
    pub throttle: f64,
    /// Régime, tr/min.
    pub rpm: f64,
    /// Température, K.
    pub temperature: f64,
    /// Charge `[0, 1]`.
    pub load: f64,
    /// Pas d'hélice, % du maximum, négatif en arrière (D5).
    pub pitch_percent: f64,
}

/// Vent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wind {
    /// Direction d'où vient le vent vrai, rad.
    pub true_direction: f64,
    /// Vitesse du vent vrai, m/s.
    pub true_speed: f64,
    /// Angle apparent relatif à l'étrave, rad `(-π, π]`, positif tribord.
    pub apparent_angle: f64,
    /// Vitesse apparente, m/s.
    pub apparent_speed: f64,
}

/// Eau et courant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Water {
    /// Profondeur sous transducteur, m.
    pub depth: f64,
    /// Température, K.
    pub temperature: f64,
    /// Direction vers laquelle porte le courant, rad.
    pub current_set: f64,
    /// Vitesse du courant, m/s.
    pub current_drift: f64,
}

/// Satellite visible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Satellite {
    /// Numéro PRN.
    pub prn: u8,
    /// Élévation, degrés.
    pub elevation: f64,
    /// Azimut, degrés.
    pub azimuth: f64,
    /// Rapport signal/bruit, dB-Hz.
    pub snr: f64,
}

/// GNSS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gnss {
    /// Fix disponible.
    pub fix: bool,
    /// Qualité GGA (0 pas de fix, 1 GPS).
    pub quality: u8,
    /// Mode GSA (1 pas de fix, 2 2D, 3 3D).
    pub mode: u8,
    /// HDOP.
    pub hdop: f64,
    /// VDOP.
    pub vdop: f64,
    /// PDOP.
    pub pdop: f64,
    /// Satellites visibles.
    pub satellites: Vec<Satellite>,
    /// PRN utilisés pour le fix.
    pub used: Vec<u8>,
}

/// Point de route nommé.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Waypoint {
    /// Identifiant court (WPL, RMB), au plus 8 caractères.
    pub name: String,
    /// Position.
    pub position: LatLon,
}

/// Géométrie de route, calculée d'une seule source (docs/06 §10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGeometry {
    /// Écart de route signé, m : négatif à gauche de la route.
    pub xte: f64,
    /// Distance restante, m.
    pub dtg: f64,
    /// Relèvement origine → destination, rad.
    pub bearing_origin_to_dest: f64,
    /// Relèvement position → destination, rad.
    pub bearing_to_dest: f64,
    /// Vitesse de rapprochement (VMG vers la destination), m/s.
    pub closing_speed: f64,
    /// Cercle d'arrivée atteint.
    pub arrived: bool,
    /// Perpendiculaire à la destination franchie.
    pub perpendicular_passed: bool,
}

/// Route active.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    /// Origine de la branche active (figée à la pose, comme le legacy).
    pub origin: Option<Waypoint>,
    /// Points de route.
    pub waypoints: Vec<Waypoint>,
    /// Index du point actif.
    pub active: usize,
    /// Géométrie courante, `None` sans destination.
    pub geometry: Option<RouteGeometry>,
}

impl Route {
    /// Destination active.
    pub fn destination(&self) -> Option<&Waypoint> {
        self.waypoints.get(self.active)
    }
}

/// Mode du pilote automatique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutopilotMode {
    /// Barre manuelle.
    #[default]
    Off,
    /// Tenue de cap.
    Heading,
    /// Suivi de route vers la destination active.
    Route,
}

/// Pilote automatique.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Autopilot {
    /// Mode.
    pub mode: AutopilotMode,
    /// Cap consigne, rad.
    pub heading_target: f64,
}

/// État complet, publié en instantané.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimState {
    /// Heure simulée, ms epoch.
    pub time_ms: i64,
    /// Temps simulé écoulé depuis le départ, ms.
    pub elapsed_ms: i64,
    /// Nombre de pas intégrés.
    pub steps: u64,
    /// Simulation démarrée.
    pub running: bool,
    /// Simulation en pause (temps gelé).
    pub paused: bool,
    /// Graine effective.
    pub seed: u64,
    /// Navire.
    pub vessel: Vessel,
    /// Moteurs bâbord (`p0`) et tribord (`p1`).
    pub engines: [Engine; 2],
    /// Vent.
    pub wind: Wind,
    /// Eau.
    pub water: Water,
    /// GNSS.
    pub gnss: GnssState,
    /// Route.
    pub route: Route,
    /// Pilote.
    pub autopilot: Autopilot,
    /// Waypoint marqué (`WPL`), muet par défaut.
    pub marked: Option<Waypoint>,
    /// Déclinaison magnétique, rad.
    pub magnetic_variation: f64,
    /// Grandeurs figées par override, unités d'usage.
    pub overrides: BTreeMap<String, f64>,
    /// Horodatage imposé par une source de trace (heure `RMC`), ms epoch.
    pub source_time_ms: Option<i64>,
    /// Une trace ou un rejeu pilote le navire.
    pub source_active: bool,
}

/// Alias : l'état GNSS.
pub type GnssState = Gnss;

impl SimState {
    /// Heure à utiliser dans les phrases : celle de la trace si imposée.
    pub fn sentence_time_ms(&self) -> i64 {
        self.source_time_ms.unwrap_or(self.time_ms)
    }

    /// Cap magnétique, rad.
    pub fn heading_magnetic(&self) -> f64 {
        nmeasim_core::units::normalize_rad(self.vessel.heading - self.magnetic_variation)
    }
}
