//! Configuration persistante (J7, `docs/03` CFG-01 à CFG-04).
//!
//! - schéma versionné (`schema: 2`) ;
//! - migration de la configuration legacy `nmeasim_config` ([`crate::legacy`]) ;
//! - aucun champ perdu : les clés inconnues sont conservées dans `extra` et
//!   réécrites à l'enregistrement, et signalées ;
//! - validation explicite : toute erreur nomme le champ fautif.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nmeasim_encode::nmea::{ALL_SENTENCES, NmeaOptions};
use nmeasim_encode::signalk::SignalKOptions;
use nmeasim_encode::viewsync::ViewSyncOptions;
use nmeasim_input::gamepad::Profile as GamepadProfile;
use nmeasim_sim::SimConfig;
use nmeasim_source::PlayMode;
use nmeasim_transport::TransportSpec;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version courante du schéma.
pub const SCHEMA: u32 = 2;

/// Format de sortie d'un transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// NMEA 0183.
    #[default]
    Nmea,
    /// Signal K (JSON).
    Signalk,
}

/// Transport configuré.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportConfig {
    /// Identifiant unique.
    pub id: String,
    /// Actif.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Format émis.
    #[serde(default)]
    pub format: OutputFormat,
    /// Spécification.
    #[serde(flatten)]
    pub spec: TransportSpec,
}

fn yes() -> bool {
    true
}

/// Sortie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OutputConfig {
    /// Intervalle de publication, ms (100 à 10 000, SIM-03).
    pub interval_ms: u32,
    /// Options NMEA.
    pub nmea: NmeaOptions,
    /// Options Signal K.
    pub signalk: SignalKOptions,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            interval_ms: 1000,
            nmea: NmeaOptions::default(),
            signalk: SignalKOptions::default(),
        }
    }
}

/// Manette.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GamepadConfig {
    /// Prise en charge active.
    pub enabled: bool,
    /// Profil actif.
    pub active: String,
    /// Profils.
    pub profiles: Vec<GamepadProfile>,
}

impl Default for GamepadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            active: "default".into(),
            profiles: vec![GamepadProfile::default()],
        }
    }
}

impl GamepadConfig {
    /// Profil actif (le premier à défaut).
    pub fn active_profile(&self) -> GamepadProfile {
        self.profiles
            .iter()
            .find(|p| p.name == self.active)
            .or(self.profiles.first())
            .cloned()
            .unwrap_or_default()
    }
}

/// Carte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MapConfig {
    /// Centre suivi sur le navire.
    pub follow: bool,
    /// Niveau de zoom (Web Mercator).
    pub zoom: f64,
    /// Fond OpenStreetMap (réseau requis) ; sinon carroyage seul.
    pub tiles: bool,
    /// Modèle d'URL de tuiles.
    pub tile_url: String,
    /// Longueur de trace affichée, points.
    pub trail_points: usize,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            follow: true,
            zoom: 13.0,
            tiles: false,
            tile_url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png".into(),
            trail_points: 2000,
        }
    }
}

/// Interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiConfig {
    /// Thème (`dark`, `light`).
    pub theme: String,
    /// Onglet du bas actif.
    pub bottom_tab: String,
    /// Carte.
    pub map: MapConfig,
    /// Taille du moniteur NMEA, lignes.
    pub monitor_lines: usize,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            bottom_tab: "nmea".into(),
            map: MapConfig::default(),
            monitor_lines: 2000,
        }
    }
}

/// Journal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LoggingConfig {
    /// Ligne `meta:` (D7) dans les journaux écrits.
    #[serde(default = "yes")]
    pub meta: bool,
    /// Dernier répertoire utilisé.
    pub directory: Option<String>,
}

/// API distante.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ApiConfig {
    /// Active.
    pub enabled: bool,
    /// Adresse d'écoute (`127.0.0.1` par défaut, `docs/09` §7.5).
    pub bind: String,
    /// Port.
    pub port: u16,
    /// Jeton `Authorization: Bearer`, facultatif.
    pub token: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: "127.0.0.1".into(),
            port: 8375,
            token: None,
        }
    }
}

/// Traces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TrackConfig {
    /// Lecture KML limitée au legacy.
    pub kml_legacy: bool,
    /// Mode de lecture.
    pub play_mode: PlayMode,
    /// Boucle en fin de trace.
    pub repeat: bool,
}

impl Default for TrackConfig {
    fn default() -> Self {
        Self {
            kml_legacy: false,
            play_mode: PlayMode::FixedRate,
            repeat: false,
        }
    }
}

/// Configuration complète.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    /// Version du schéma.
    pub schema: u32,
    /// Démarrage automatique.
    pub auto_start: bool,
    /// Simulation.
    pub simulation: SimConfig,
    /// Sortie.
    pub output: OutputConfig,
    /// Transports.
    pub transports: Vec<TransportConfig>,
    /// ViewSync.
    pub viewsync: ViewSyncOptions,
    /// Manette.
    pub gamepad: GamepadConfig,
    /// Interface.
    pub ui: UiConfig,
    /// Journal.
    pub logging: LoggingConfig,
    /// API distante.
    pub api: ApiConfig,
    /// Traces.
    pub track: TrackConfig,
    /// Dernier scénario ouvert.
    pub last_scenario: Option<String>,
    /// Clés inconnues, conservées telles quelles.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            auto_start: false,
            simulation: SimConfig::default(),
            output: OutputConfig::default(),
            transports: vec![
                TransportConfig {
                    id: "nmea-tcp".into(),
                    enabled: true,
                    format: OutputFormat::Nmea,
                    spec: TransportSpec::TcpServer {
                        bind: "0.0.0.0".into(),
                        port: 10110,
                    },
                },
                TransportConfig {
                    id: "nmea-udp".into(),
                    enabled: false,
                    format: OutputFormat::Nmea,
                    spec: TransportSpec::UdpClient {
                        host: "127.0.0.1".into(),
                        port: 3100,
                    },
                },
                TransportConfig {
                    id: "signalk-ws".into(),
                    enabled: false,
                    format: OutputFormat::Signalk,
                    spec: TransportSpec::WebsocketServer {
                        bind: "0.0.0.0".into(),
                        port: 3000,
                    },
                },
            ],
            viewsync: ViewSyncOptions {
                enabled: false,
                ..ViewSyncOptions::default()
            },
            gamepad: GamepadConfig::default(),
            ui: UiConfig::default(),
            logging: LoggingConfig {
                meta: true,
                directory: None,
            },
            api: ApiConfig::default(),
            track: TrackConfig::default(),
            last_scenario: None,
            extra: BTreeMap::new(),
        }
    }
}

/// Erreur de configuration : champ et raison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// Champ (chemin pointé).
    pub field: String,
    /// Raison.
    pub reason: String,
}

impl ConfigError {
    /// Construit une erreur.
    pub fn new(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} : {}", self.field, self.reason)
    }
}

impl std::error::Error for ConfigError {}

impl AppConfig {
    /// Validation complète.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema != SCHEMA {
            return Err(ConfigError::new(
                "schema",
                format!("version {} inconnue, attendu {SCHEMA}", self.schema),
            ));
        }
        self.simulation
            .validate()
            .map_err(|e| ConfigError::new(format!("simulation.{}", e.field), e.reason))?;
        if !(1..=10_000).contains(&self.output.interval_ms) {
            return Err(ConfigError::new(
                "output.intervalMs",
                "doit être dans 1..=10000",
            ));
        }
        if let Some(list) = &self.output.nmea.sentences {
            if let Some(bad) = list.iter().find(|s| !ALL_SENTENCES.contains(&s.as_str())) {
                return Err(ConfigError::new(
                    "output.nmea.sentences",
                    format!(
                        "phrase inconnue `{bad}` ; acceptées : {}",
                        ALL_SENTENCES.join(", ")
                    ),
                ));
            }
        }
        for (k, v) in &self.output.nmea.talkers {
            if v.len() != 2 || !v.chars().all(|c| c.is_ascii_uppercase()) {
                return Err(ConfigError::new(
                    format!("output.nmea.talkers.{k}"),
                    "talker de deux lettres majuscules",
                ));
            }
        }
        self.output
            .nmea
            .ais
            .validate()
            .map_err(|(f, r)| ConfigError::new(format!("output.nmea.{f}"), r))?;
        let mut ids = std::collections::BTreeSet::new();
        for (i, t) in self.transports.iter().enumerate() {
            let f = format!("transports[{i}]");
            if t.id.trim().is_empty() {
                return Err(ConfigError::new(format!("{f}.id"), "identifiant vide"));
            }
            if !ids.insert(t.id.clone()) {
                return Err(ConfigError::new(
                    format!("{f}.id"),
                    format!("identifiant `{}` en double", t.id),
                ));
            }
            let port = match &t.spec {
                TransportSpec::TcpServer { port, .. }
                | TransportSpec::TcpClient { port, .. }
                | TransportSpec::UdpBroadcast { port, .. }
                | TransportSpec::UdpClient { port, .. }
                | TransportSpec::UdpMulticast { port, .. }
                | TransportSpec::WebsocketServer { port, .. } => Some(*port),
                TransportSpec::Serial { port, .. } => {
                    if port.trim().is_empty() {
                        return Err(ConfigError::new(
                            format!("{f}.port"),
                            "chemin de port série vide",
                        ));
                    }
                    None
                }
            };
            if port == Some(0) {
                return Err(ConfigError::new(format!("{f}.port"), "port 0 interdit"));
            }
            if let TransportSpec::UdpMulticast { group, .. } = &t.spec {
                if !group
                    .parse::<std::net::Ipv4Addr>()
                    .is_ok_and(|g| g.is_multicast())
                {
                    return Err(ConfigError::new(
                        format!("{f}.group"),
                        format!("`{group}` n'est pas une adresse multicast IPv4"),
                    ));
                }
            }
        }
        for p in &self.gamepad.profiles {
            p.validate()
                .map_err(|r| ConfigError::new("gamepad.profiles", r))?;
        }
        if self.api.enabled && self.api.port == 0 {
            return Err(ConfigError::new("api.port", "port 0 interdit"));
        }
        Ok(())
    }

    /// Clés inconnues conservées (pour avertir l'utilisateur).
    pub fn unknown_keys(&self) -> Vec<String> {
        self.extra.keys().cloned().collect()
    }
}

/// Résultat d'un chargement.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// Configuration.
    pub config: AppConfig,
    /// Avertissements (migration, clés inconnues).
    pub warnings: Vec<String>,
    /// Migrée depuis le legacy.
    pub migrated: bool,
}

/// Charge une configuration depuis du JSON : schéma 2, ou legacy (migré).
pub fn from_json(text: &str) -> Result<Loaded, ConfigError> {
    let v: Value = serde_json::from_str(text)
        .map_err(|e| ConfigError::new("(fichier)", format!("JSON invalide : {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| ConfigError::new("(racine)", "objet JSON attendu"))?;
    if obj.contains_key("schema") {
        let config: AppConfig = serde_json::from_value(v.clone())
            .map_err(|e| ConfigError::new("(schéma 2)", e.to_string()))?;
        config.validate()?;
        let warnings = config
            .unknown_keys()
            .into_iter()
            .map(|k| format!("clé inconnue conservée : {k}"))
            .collect();
        Ok(Loaded {
            config,
            warnings,
            migrated: false,
        })
    } else {
        let (config, warnings) = crate::legacy::migrate(&v)?;
        config.validate()?;
        Ok(Loaded {
            config,
            warnings,
            migrated: true,
        })
    }
}

/// Emplacement par défaut : `~/.config/nmeasim-rs/config.json`.
pub fn default_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("nmeasim-rs")
        .join("config.json")
}

/// Lit un fichier ; absent → configuration par défaut.
pub fn load(path: &Path) -> Result<Loaded, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(t) => from_json(&t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Loaded {
            config: AppConfig::default(),
            warnings: vec![],
            migrated: false,
        }),
        Err(e) => Err(ConfigError::new(path.display().to_string(), e.to_string())),
    }
}

/// Écrit de façon atomique (fichier temporaire puis renommage).
pub fn save(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    config.validate()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| ConfigError::new(dir.display().to_string(), e.to_string()))?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| ConfigError::new("(sérialisation)", e.to_string()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)
        .map_err(|e| ConfigError::new(tmp.display().to_string(), e.to_string()))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| ConfigError::new(path.display().to_string(), e.to_string()))
}
