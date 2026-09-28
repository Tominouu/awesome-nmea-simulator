//! Commandes de l'API de contrôle (`docs/09` §7.3). Toute écriture de l'état
//! passe par une [`Command`] ; aucune source d'entrée ne touche l'état.

use nmeasim_core::geo::LatLon;
use serde::{Deserialize, Serialize};

use crate::state::AutopilotMode;

/// Moteur(s) visé(s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineSel {
    /// Bâbord.
    P0,
    /// Tribord.
    P1,
    /// Les deux.
    All,
}

impl EngineSel {
    /// Index concernés.
    pub fn indices(self) -> &'static [usize] {
        match self {
            EngineSel::P0 => &[0],
            EngineSel::P1 => &[1],
            EngineSel::All => &[0, 1],
        }
    }
}

/// Origine d'une commande.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase", tag = "type", content = "id")]
pub enum Source {
    /// Clavier.
    Keyboard,
    /// Manette, avec son identifiant.
    Gamepad(u32),
    /// Interface graphique.
    #[default]
    Ui,
    /// Carte.
    Map,
    /// Script ou scénario.
    Script,
    /// API distante.
    Remote,
    /// Entrée série.
    Serial,
    /// Rejeu.
    Replay,
    /// Suivi de trace.
    Track,
}

/// Commande. Forme JSON : `{"kind": "helm.rudder.set", "deg": 10}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Command {
    /// Démarre la simulation.
    #[serde(rename = "sim.start")]
    Start,
    /// Arrête la simulation.
    #[serde(rename = "sim.stop")]
    Stop,
    /// Gèle le temps simulé.
    #[serde(rename = "sim.pause")]
    Pause,
    /// Reprend.
    #[serde(rename = "sim.resume")]
    Resume,
    /// Bascule pause / reprise.
    #[serde(rename = "sim.togglePause")]
    TogglePause,
    /// Restaure l'état initial.
    #[serde(rename = "sim.reset")]
    Reset,
    /// Consigne de barre, degrés.
    #[serde(rename = "helm.rudder.set")]
    SetRudder {
        /// Degrés, tribord positif.
        deg: f64,
    },
    /// Consigne de barre ± delta.
    #[serde(rename = "helm.rudder.nudge")]
    NudgeRudder {
        /// Delta, degrés.
        deg: f64,
    },
    /// Barre au centre.
    #[serde(rename = "helm.rudder.center")]
    CenterRudder,
    /// Commande de propulsion.
    #[serde(rename = "propulsion.throttle.set")]
    SetThrottle {
        /// Moteur(s).
        engine: EngineSel,
        /// `[-1, 1]`.
        value: f64,
    },
    /// Commande de propulsion ± delta.
    #[serde(rename = "propulsion.throttle.nudge")]
    NudgeThrottle {
        /// Moteur(s).
        engine: EngineSel,
        /// Delta.
        delta: f64,
    },
    /// Marche / arrêt moteur.
    #[serde(rename = "propulsion.engine.set")]
    SetEngineState {
        /// Moteur(s).
        engine: EngineSel,
        /// En marche.
        running: bool,
    },
    /// Bascule marche / arrêt des deux moteurs.
    #[serde(rename = "propulsion.engine.toggle")]
    ToggleEngines,
    /// Cap : override direct en legacy, consigne du pilote en modern.
    #[serde(rename = "nav.heading.set")]
    SetHeading {
        /// Degrés vrais.
        deg: f64,
    },
    /// Cap ± delta.
    #[serde(rename = "nav.heading.nudge")]
    NudgeHeading {
        /// Delta, degrés.
        deg: f64,
    },
    /// Vitesse commandée (legacy) ou vitesse imposée.
    #[serde(rename = "nav.speed.set")]
    SetSpeed {
        /// Nœuds.
        kn: f64,
    },
    /// Vitesse ± delta.
    #[serde(rename = "nav.speed.nudge")]
    NudgeSpeed {
        /// Nœuds.
        kn: f64,
    },
    /// Téléporte le navire.
    #[serde(rename = "nav.position.set")]
    SetPosition {
        /// Position.
        position: LatLon,
    },
    /// Pose la destination ; l'origine est figée sur la position courante.
    #[serde(rename = "route.destination.set")]
    SetDestination {
        /// Position.
        position: LatLon,
        /// Nom.
        #[serde(default)]
        name: Option<String>,
    },
    /// Efface la route.
    #[serde(rename = "route.destination.clear")]
    ClearDestination,
    /// Remplace la route.
    #[serde(rename = "route.set")]
    SetRoute {
        /// Points.
        points: Vec<LatLon>,
    },
    /// Ajoute un point.
    #[serde(rename = "route.waypoint.add")]
    AddWaypoint {
        /// Position.
        position: LatLon,
        /// Nom.
        #[serde(default)]
        name: Option<String>,
    },
    /// Ajoute un point à la position du navire.
    #[serde(rename = "route.waypoint.addAtVessel")]
    AddWaypointAtVessel,
    /// Passe au point suivant.
    #[serde(rename = "route.waypoint.next")]
    NextWaypoint,
    /// Marque la position (waypoint `WPL`, comme « Mark Position »).
    #[serde(rename = "route.mark")]
    MarkPosition,
    /// Pilote automatique.
    #[serde(rename = "autopilot.set")]
    SetAutopilot {
        /// Mode.
        mode: AutopilotMode,
    },
    /// Bascule le pilote (tenue de cap sur le cap courant).
    #[serde(rename = "autopilot.toggle")]
    ToggleAutopilot,
    /// Consigne de cap du pilote.
    #[serde(rename = "autopilot.heading.set")]
    SetAutopilotHeading {
        /// Degrés.
        deg: f64,
    },
    /// Mouillage.
    #[serde(rename = "anchor.set")]
    SetAnchor {
        /// Mouillé.
        on: bool,
    },
    /// Bascule le mouillage.
    #[serde(rename = "anchor.toggle")]
    ToggleAnchor,
    /// Modifie une valeur (et son nominal) sans la figer.
    #[serde(rename = "value.set")]
    SetValue {
        /// Chemin pointé.
        path: String,
        /// Valeur, unité d'usage.
        value: f64,
    },
    /// Fige une grandeur.
    #[serde(rename = "override.set")]
    SetOverride {
        /// Chemin pointé.
        path: String,
        /// Valeur, unité d'usage.
        value: f64,
    },
    /// Rend une grandeur au modèle.
    #[serde(rename = "override.release")]
    ReleaseOverride {
        /// Chemin pointé.
        path: String,
    },
    /// Point de trace (source GPX/KML) : position, vitesse et cap imposés.
    #[serde(rename = "source.trackPoint")]
    TrackPoint {
        /// Position.
        position: LatLon,
        /// Vitesse, nœuds, si connue.
        sog_kn: Option<f64>,
        /// Cap, degrés, si connu.
        cog_deg: Option<f64>,
        /// Altitude, m.
        altitude: Option<f64>,
        /// Heure du point, ms epoch.
        time_ms: Option<i64>,
    },
    /// Fin de pilotage par une source.
    #[serde(rename = "source.release")]
    ReleaseSource,
}

/// Raison d'un refus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "camelCase")]
pub enum RejectReason {
    /// Chemin inconnu.
    UnknownPath {
        /// Chemin.
        path: String,
    },
    /// Hors bornes.
    OutOfRange {
        /// Minimum.
        min: f64,
        /// Maximum.
        max: f64,
    },
    /// Grandeur en lecture seule.
    ReadOnly,
    /// Une source (trace, rejeu) détient le jeton de producteur.
    ProducerLocked,
    /// Interdit dans l'état courant.
    InvalidState {
        /// Détail.
        detail: String,
    },
    /// Valeur invalide.
    Invalid {
        /// Détail.
        detail: String,
    },
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RejectReason::UnknownPath { path } => write!(f, "chemin inconnu : {path}"),
            RejectReason::OutOfRange { min, max } => write!(f, "hors bornes [{min}, {max}]"),
            RejectReason::ReadOnly => write!(f, "lecture seule"),
            RejectReason::ProducerLocked => write!(f, "une source pilote le navire"),
            RejectReason::InvalidState { detail } => write!(f, "état invalide : {detail}"),
            RejectReason::Invalid { detail } => write!(f, "valeur invalide : {detail}"),
        }
    }
}

impl Command {
    /// Vrai pour les commandes de navigation, refusées quand une source
    /// (trace, rejeu) pilote le navire (`docs/09` §3.6).
    pub fn is_navigation(&self) -> bool {
        matches!(
            self,
            Command::SetRudder { .. }
                | Command::NudgeRudder { .. }
                | Command::CenterRudder
                | Command::SetThrottle { .. }
                | Command::NudgeThrottle { .. }
                | Command::SetHeading { .. }
                | Command::NudgeHeading { .. }
                | Command::SetSpeed { .. }
                | Command::NudgeSpeed { .. }
                | Command::SetPosition { .. }
                | Command::SetAutopilot { .. }
                | Command::ToggleAutopilot
                | Command::SetAutopilotHeading { .. }
                | Command::SetAnchor { .. }
                | Command::ToggleAnchor
        )
    }

    /// Nom stable (`kind`).
    pub fn kind(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_owned))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let c: Command = serde_json::from_str(r#"{"kind":"helm.rudder.set","deg":10}"#).unwrap();
        assert_eq!(c, Command::SetRudder { deg: 10.0 });
        let t: Command = serde_json::from_str(
            r#"{"kind":"propulsion.throttle.set","engine":"all","value":0.5}"#,
        )
        .unwrap();
        assert_eq!(
            t,
            Command::SetThrottle {
                engine: EngineSel::All,
                value: 0.5
            }
        );
        assert_eq!(Command::Reset.kind(), "sim.reset");
        assert!(Command::CenterRudder.is_navigation());
        assert!(!Command::Stop.is_navigation());
    }
}
