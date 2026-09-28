//! Transports de nmeasim-rs (`docs/08`).
//!
//! Un transport ne connaît pas la structure des phrases : il reçoit des
//! [`Frame`]s déjà découpées (une phrase, un delta JSON, un bloc de rejeu) et
//! applique **son** framing (CRLF, un datagramme par trame, un message
//! WebSocket par trame). Chaque transport écrit depuis ses propres threads, à
//! travers des files bornées : un client lent est déconnecté, jamais le noyau
//! bloqué.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

mod serial;
mod tcp;
mod udp;
mod ws;

pub use serial::{SerialTransport, ports as serial_ports};
pub use tcp::{TcpClientTransport, TcpServerTransport};
pub use udp::{UdpTransport, broadcast_address, interfaces};
pub use ws::WebSocketServerTransport;

/// Nature d'une trame, qui décide du framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameKind {
    /// Une phrase NMEA, sans terminateur : le transport ajoute CRLF.
    Sentence,
    /// Un document JSON (Signal K) : envoyé tel quel (CRLF en série).
    Json,
    /// Octets bruts déjà framés (bloc de rejeu legacy, phrase utilisateur).
    Raw,
}

/// Trame à émettre.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Nature.
    pub kind: FrameKind,
    /// Contenu.
    pub data: String,
}

impl Frame {
    /// Phrase NMEA.
    pub fn sentence(s: impl Into<String>) -> Self {
        Self {
            kind: FrameKind::Sentence,
            data: s.into(),
        }
    }

    /// Document JSON.
    pub fn json(s: impl Into<String>) -> Self {
        Self {
            kind: FrameKind::Json,
            data: s.into(),
        }
    }

    /// Octets bruts.
    pub fn raw(s: impl Into<String>) -> Self {
        Self {
            kind: FrameKind::Raw,
            data: s.into(),
        }
    }

    /// Octets pour un flux ou un datagramme (TCP, UDP, WebSocket).
    pub fn wire(&self) -> Vec<u8> {
        match self.kind {
            FrameKind::Sentence => format!("{}\r\n", self.data).into_bytes(),
            FrameKind::Json | FrameKind::Raw => self.data.clone().into_bytes(),
        }
    }

    /// Octets pour une liaison série : le legacy termine aussi le JSON par CRLF.
    pub fn wire_serial(&self) -> Vec<u8> {
        match self.kind {
            FrameKind::Sentence | FrameKind::Json => format!("{}\r\n", self.data).into_bytes(),
            FrameKind::Raw => self.data.clone().into_bytes(),
        }
    }
}

/// Parité série.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Parity {
    /// Aucune.
    #[default]
    None,
    /// Paire.
    Even,
    /// Impaire.
    Odd,
}

/// Contrôle de flux série.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlowControl {
    /// Aucun.
    #[default]
    None,
    /// XON/XOFF.
    Software,
    /// RTS/CTS.
    Hardware,
}

/// Spécification d'un transport. Les noms suivent `docs/08` §2 ; la
/// migration des valeurs numériques legacy est faite par l'application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TransportSpec {
    /// Serveur TCP (legacy 1).
    #[serde(rename_all = "camelCase")]
    TcpServer {
        /// Adresse d'écoute (`0.0.0.0` = toutes, comme le legacy).
        #[serde(default = "any_addr")]
        bind: String,
        /// Port.
        port: u16,
    },
    /// Client TCP (legacy 2), avec reconnexion.
    #[serde(rename_all = "camelCase")]
    TcpClient {
        /// Hôte.
        host: String,
        /// Port.
        port: u16,
    },
    /// Liaison série (legacy 3).
    #[serde(rename_all = "camelCase")]
    Serial {
        /// Chemin du port (`/dev/ttyUSB0`, `COM3`).
        port: String,
        /// Débit.
        #[serde(default = "default_baud")]
        baud_rate: u32,
        /// Bits de données (5 à 8).
        #[serde(default = "default_data_bits")]
        data_bits: u8,
        /// Bits d'arrêt (1 ou 2).
        #[serde(default = "default_stop_bits")]
        stop_bits: u8,
        /// Parité.
        #[serde(default)]
        parity: Parity,
        /// Contrôle de flux.
        #[serde(default)]
        flow_control: FlowControl,
    },
    /// Diffusion UDP (legacy 4) : adresse de diffusion de l'interface.
    #[serde(rename_all = "camelCase")]
    UdpBroadcast {
        /// Interface (`lo`, `eth0`…) ; vide = 255.255.255.255.
        #[serde(default)]
        interface: String,
        /// Port destination.
        port: u16,
    },
    /// Client UDP unicast (legacy 5).
    #[serde(rename_all = "camelCase")]
    UdpClient {
        /// Hôte.
        host: String,
        /// Port.
        port: u16,
    },
    /// Multicast UDP (legacy 6).
    #[serde(rename_all = "camelCase")]
    UdpMulticast {
        /// Groupe.
        group: String,
        /// Port.
        port: u16,
        /// Adresse IPv4 de l'interface de sortie ; vide = défaut système.
        #[serde(default)]
        interface: String,
        /// TTL (128 comme le legacy).
        #[serde(default = "default_ttl")]
        ttl: u32,
        /// Bouclage local.
        #[serde(default = "yes")]
        loopback: bool,
    },
    /// Serveur WebSocket (legacy 0).
    #[serde(rename_all = "camelCase")]
    WebsocketServer {
        /// Adresse d'écoute.
        #[serde(default = "any_addr")]
        bind: String,
        /// Port.
        port: u16,
    },
}

fn any_addr() -> String {
    "0.0.0.0".into()
}
fn default_baud() -> u32 {
    4800
}
fn default_data_bits() -> u8 {
    8
}
fn default_stop_bits() -> u8 {
    1
}
fn default_ttl() -> u32 {
    128
}
fn yes() -> bool {
    true
}

impl TransportSpec {
    /// Nom canonique du type.
    pub fn type_name(&self) -> &'static str {
        match self {
            TransportSpec::TcpServer { .. } => "tcp-server",
            TransportSpec::TcpClient { .. } => "tcp-client",
            TransportSpec::Serial { .. } => "serial",
            TransportSpec::UdpBroadcast { .. } => "udp-broadcast",
            TransportSpec::UdpClient { .. } => "udp-client",
            TransportSpec::UdpMulticast { .. } => "udp-multicast",
            TransportSpec::WebsocketServer { .. } => "websocket-server",
        }
    }

    /// Point de terminaison lisible.
    pub fn endpoint(&self) -> String {
        match self {
            TransportSpec::TcpServer { bind, port }
            | TransportSpec::WebsocketServer { bind, port } => {
                format!("{bind}:{port}")
            }
            TransportSpec::TcpClient { host, port } | TransportSpec::UdpClient { host, port } => {
                format!("{host}:{port}")
            }
            TransportSpec::Serial {
                port, baud_rate, ..
            } => format!("{port} @ {baud_rate}"),
            TransportSpec::UdpBroadcast { interface, port } => {
                format!(
                    "{}:{port}",
                    broadcast_address(interface)
                        .map_or("255.255.255.255".into(), |a| a.to_string())
                )
            }
            TransportSpec::UdpMulticast { group, port, .. } => format!("{group}:{port}"),
        }
    }
}

/// État d'un transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportState {
    /// Arrêté.
    Stopped,
    /// Démarrage ou connexion en cours.
    Starting,
    /// Opérationnel : bind réussi, ou connecté.
    Running,
    /// Connexion perdue, reprise en cours.
    Reconnecting,
    /// Échec.
    Failed,
}

/// Statut observable (`docs/08` §8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportStatus {
    /// Identifiant.
    pub id: String,
    /// Type canonique.
    pub kind: String,
    /// État.
    pub state: TransportState,
    /// Point de terminaison.
    pub endpoint: String,
    /// Clients connectés (serveurs).
    pub clients: usize,
    /// Trames émises.
    pub messages: u64,
    /// Octets émis.
    pub bytes: u64,
    /// Erreurs.
    pub errors: u64,
    /// Dernière erreur.
    pub last_error: Option<String>,
    /// Dernier envoi, ms epoch.
    pub last_sent_ms: Option<i64>,
}

/// Compteurs partagés entre un transport et ses threads.
#[derive(Debug, Clone)]
pub struct Shared(Arc<Mutex<TransportStatus>>);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl Shared {
    /// Nouveau statut.
    pub fn new(id: &str, spec: &TransportSpec) -> Self {
        Self(Arc::new(Mutex::new(TransportStatus {
            id: id.into(),
            kind: spec.type_name().into(),
            state: TransportState::Stopped,
            endpoint: spec.endpoint(),
            clients: 0,
            messages: 0,
            bytes: 0,
            errors: 0,
            last_error: None,
            last_sent_ms: None,
        })))
    }

    fn with<R>(&self, f: impl FnOnce(&mut TransportStatus) -> R) -> R {
        let mut g = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut g)
    }

    /// Copie du statut.
    pub fn get(&self) -> TransportStatus {
        self.with(|s| s.clone())
    }

    /// Change l'état.
    pub fn set_state(&self, st: TransportState) {
        self.with(|s| s.state = st);
    }

    /// État courant.
    pub fn state(&self) -> TransportState {
        self.with(|s| s.state)
    }

    /// Compte un envoi.
    pub fn sent(&self, bytes: usize) {
        self.with(|s| {
            s.messages += 1;
            s.bytes += bytes as u64;
            s.last_sent_ms = Some(now_ms());
        });
    }

    /// Compte une erreur.
    pub fn error(&self, e: impl ToString) {
        let e = e.to_string();
        self.with(|s| {
            s.errors += 1;
            s.last_error = Some(e);
        });
    }

    /// Nombre de clients.
    pub fn set_clients(&self, n: usize) {
        self.with(|s| s.clients = n);
    }
}

/// Fournit le hello Signal K à l'ouverture d'une connexion.
pub type HelloFn = Arc<dyn Fn() -> String + Send + Sync>;

/// Lignes reçues (entrée série, clients TCP), consommées par l'application.
pub type Inbox = Arc<Mutex<Vec<String>>>;

/// Erreur de démarrage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub String);

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TransportError {}

/// Un transport.
pub trait Transport: Send {
    /// Démarre ; ne rend `Ok` qu'une fois le bind réussi (correction legacy).
    /// Un client TCP rend `Ok` immédiatement et se connecte en tâche de fond.
    fn start(&mut self) -> Result<(), TransportError>;
    /// Émet une trame. Ne bloque jamais.
    fn send(&mut self, frame: &Frame);
    /// Arrête et libère les ressources ; idempotent.
    fn stop(&mut self);
    /// Statut.
    fn status(&self) -> TransportStatus;
}

/// Construit un transport depuis sa spécification.
pub fn build(
    id: &str,
    spec: &TransportSpec,
    hello: Option<HelloFn>,
    inbox: Inbox,
) -> Box<dyn Transport> {
    match spec {
        TransportSpec::TcpServer { bind, port } => {
            Box::new(TcpServerTransport::new(id, spec, bind, *port, hello, inbox))
        }
        TransportSpec::TcpClient { host, port } => {
            Box::new(TcpClientTransport::new(id, spec, host, *port, hello, inbox))
        }
        TransportSpec::WebsocketServer { bind, port } => {
            Box::new(WebSocketServerTransport::new(id, spec, bind, *port, hello))
        }
        TransportSpec::Serial { .. } => Box::new(SerialTransport::new(id, spec, inbox)),
        TransportSpec::UdpBroadcast { .. }
        | TransportSpec::UdpClient { .. }
        | TransportSpec::UdpMulticast { .. } => Box::new(UdpTransport::new(id, spec)),
    }
}

/// Taille des files d'émission par client.
pub const QUEUE_DEPTH: usize = 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing() {
        assert_eq!(Frame::sentence("$A").wire(), b"$A\r\n");
        assert_eq!(Frame::json("{}").wire(), b"{}");
        assert_eq!(Frame::json("{}").wire_serial(), b"{}\r\n");
        assert_eq!(Frame::raw("x\r\n").wire(), b"x\r\n");
    }

    #[test]
    fn spec_json() {
        let s: TransportSpec =
            serde_json::from_str(r#"{"type":"tcp-server","port":10110}"#).unwrap();
        assert_eq!(
            s,
            TransportSpec::TcpServer {
                bind: "0.0.0.0".into(),
                port: 10110
            }
        );
        assert_eq!(s.endpoint(), "0.0.0.0:10110");
        let m: TransportSpec =
            serde_json::from_str(r#"{"type":"udp-multicast","group":"239.1.2.3","port":5000}"#)
                .unwrap();
        assert!(matches!(
            m,
            TransportSpec::UdpMulticast {
                ttl: 128,
                loopback: true,
                ..
            }
        ));
        assert!(serde_json::from_str::<TransportSpec>(r#"{"type":"udp","port":1}"#).is_err());
    }
}
