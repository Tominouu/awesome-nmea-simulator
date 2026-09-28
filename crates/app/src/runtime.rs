//! Moteur d'exécution headless.
//!
//! Un thread unique possède le [`Simulator`] : il applique les requêtes reçues
//! par canal (API de contrôle), intègre à pas fixe, et publie à l'intervalle
//! de sortie vers les transports, le journal, ViewSync et le moniteur.
//! L'interface ne lit que [`Published`] et n'écrit que des requêtes.

use std::collections::{BTreeMap, VecDeque};
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nmeasim_core::geo::LatLon;
use nmeasim_core::random::{Rng, Xoshiro256StarStar};
use nmeasim_encode::nmea::{self, Ctx};
use nmeasim_encode::sentence::{Sentence, prefix};
use nmeasim_encode::{parse, signalk, viewsync};
use nmeasim_sim::command::{Command, RejectReason, Source};
use nmeasim_sim::state::{AutopilotMode, SimState};
use nmeasim_sim::{SimEvent, Simulator};
use nmeasim_source::journal::{self, JournalWriter};
use nmeasim_source::kml::KmlMode;
use nmeasim_source::replay::{ReplayCommand, ReplayPlayer, ReplayStatus};
use nmeasim_source::{TrackPlayer, read_track_file};
use nmeasim_transport::{Frame, HelloFn, Inbox, Transport, TransportState, TransportStatus};
use serde::Serialize;
use serde_json::json;

use crate::config::{AppConfig, ConfigError, OutputFormat};

/// Version applicative (hello Signal K, en-tête de journal).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Producteur de messages actif (arbitre à jeton unique, `docs/10` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Producer {
    /// Simulation.
    Live,
    /// Suivi de trace GPX/KML.
    Track,
    /// Rejeu d'un journal.
    Replay,
}

/// Sens d'une ligne du moniteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Émise.
    Out,
    /// Reçue.
    In,
}

/// Ligne du moniteur NMEA.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorEntry {
    /// Numéro de séquence.
    pub seq: u64,
    /// Heure murale, ms epoch.
    pub time_ms: i64,
    /// Sens.
    pub direction: Direction,
    /// Canal (`nmea`, `signalk`, `replay`, `user`, identifiant d'entrée).
    pub channel: String,
    /// Texte, sans terminateur.
    pub text: String,
}

/// Trace chargée.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    /// Nom.
    pub name: String,
    /// Index courant.
    pub index: usize,
    /// Nombre de points.
    pub total: usize,
    /// Points, pour la carte.
    pub points: Vec<LatLon>,
    /// Suivi actif.
    pub following: bool,
    /// Fin atteinte.
    pub ended: bool,
}

/// Enregistrement en cours.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingInfo {
    /// Fichier.
    pub path: String,
    /// Blocs écrits.
    pub written: u64,
    /// Blocs abandonnés.
    pub dropped: u64,
    /// Erreur disque.
    pub error: Option<String>,
}

/// Événement typé (`docs/09` §7.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum AppEvent {
    /// Événement du simulateur.
    Sim {
        /// Détail.
        detail: SimEvent,
    },
    /// Commande acceptée.
    CommandAccepted {
        /// Nom de la commande.
        kind: String,
        /// Source.
        source: Source,
    },
    /// Commande refusée.
    CommandRejected {
        /// Nom de la commande.
        kind: String,
        /// Source.
        source: Source,
        /// Raison.
        reason: RejectReason,
    },
    /// Changement d'état d'un transport.
    TransportState {
        /// Identifiant.
        id: String,
        /// État.
        state: TransportState,
        /// Dernière erreur.
        error: Option<String>,
    },
    /// Fin de trace.
    TrackEnded,
    /// Fin de rejeu.
    ReplayEnded,
    /// Enregistrement démarré.
    RecordingStarted {
        /// Fichier.
        path: String,
    },
    /// Enregistrement arrêté.
    RecordingStopped,
    /// Avertissement.
    Warning {
        /// Message.
        message: String,
    },
}

/// État publié, lu par l'interface et l'API distante.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Published {
    /// Instantané de la simulation.
    pub snapshot: SimState,
    /// Transports.
    pub transports: Vec<TransportStatus>,
    /// Producteur actif.
    pub producer: Producer,
    /// Trace.
    pub track: Option<TrackInfo>,
    /// Rejeu.
    pub replay: Option<ReplayStatus>,
    /// Enregistrement.
    pub recording: Option<RecordingInfo>,
    /// Sillage du navire.
    pub trail: VecDeque<LatLon>,
    /// Phrases émises depuis le démarrage.
    pub sentences_sent: u64,
    /// Débit, phrases par seconde.
    pub sentences_per_s: f64,
    /// Fréquence par formateur, phrases par seconde.
    pub sentence_rates: BTreeMap<String, f64>,
    /// Derniers avertissements.
    pub warnings: VecDeque<String>,
    /// Configuration courante.
    #[serde(skip)]
    pub config: AppConfig,
    /// Moniteur (anneau).
    #[serde(skip)]
    pub monitor: VecDeque<MonitorEntry>,
}

enum Request {
    Command(Command, Source, Option<Sender<Result<(), RejectReason>>>),
    ApplyConfig(Box<AppConfig>, Sender<Result<(), ConfigError>>),
    LoadTrack(PathBuf, usize, Sender<Result<String, String>>),
    FollowTrack(bool),
    RewindTrack,
    ClearTrack,
    LoadRoute(PathBuf, usize, Sender<Result<usize, String>>),
    LoadJournal(PathBuf, Sender<Result<usize, String>>),
    Replay(ReplayCommand),
    CloseReplay,
    StartRecording(PathBuf, Sender<Result<String, String>>),
    StopRecording,
    UserSentence(String),
    ClearMonitor,
    Shutdown,
}

/// Poignée clonable pour soumettre des commandes (manette, API distante).
#[derive(Clone)]
pub struct Controller {
    tx: Sender<Request>,
}

impl Controller {
    /// Soumet une commande et attend la réponse (2 s au plus).
    pub fn submit(&self, cmd: Command, source: Source) -> Result<(), RejectReason> {
        let (tx, rx) = channel();
        self.tx
            .send(Request::Command(cmd, source, Some(tx)))
            .map_err(|_| RejectReason::InvalidState {
                detail: "moteur arrêté".into(),
            })?;
        rx.recv_timeout(Duration::from_secs(2))
            .unwrap_or(Err(RejectReason::InvalidState {
                detail: "pas de réponse du moteur".into(),
            }))
    }

    /// Soumet sans attendre.
    pub fn submit_async(&self, cmd: Command, source: Source) {
        let _ = self.tx.send(Request::Command(cmd, source, None));
    }

    /// Commande de rejeu.
    pub fn replay(&self, c: ReplayCommand) {
        let _ = self.tx.send(Request::Replay(c));
    }
}

/// Le moteur, vu de l'extérieur. C'est l'implémentation de l'API de contrôle.
pub struct Runtime {
    tx: Sender<Request>,
    shared: Arc<RwLock<Published>>,
    subscribers: Arc<Mutex<Vec<Sender<AppEvent>>>>,
    worker: Option<JoinHandle<()>>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn reply<T>(tx: Sender<T>, v: T) {
    let _ = tx.send(v);
}

impl Runtime {
    /// Démarre le moteur avec une configuration validée.
    pub fn spawn(config: AppConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        let sim = Simulator::new(config.simulation.clone())
            .map_err(|e| ConfigError::new(format!("simulation.{}", e.field), e.reason))?;
        let (tx, rx) = channel();
        let shared = Arc::new(RwLock::new(Published {
            snapshot: sim.snapshot(),
            transports: vec![],
            producer: Producer::Live,
            track: None,
            replay: None,
            recording: None,
            trail: VecDeque::new(),
            sentences_sent: 0,
            sentences_per_s: 0.0,
            sentence_rates: BTreeMap::new(),
            warnings: VecDeque::new(),
            config: config.clone(),
            monitor: VecDeque::new(),
        }));
        let subscribers: Arc<Mutex<Vec<Sender<AppEvent>>>> = Arc::new(Mutex::new(Vec::new()));
        let auto = config.auto_start;
        let mut engine = Engine::new(config, sim, shared.clone(), subscribers.clone());
        let worker = thread::Builder::new()
            .name("nmeasim-engine".into())
            .spawn(move || engine.run(rx))
            .map_err(|e| ConfigError::new("(moteur)", e.to_string()))?;
        let rt = Self {
            tx,
            shared,
            subscribers,
            worker: Some(worker),
        };
        if auto {
            let _ = rt.submit(Command::Start, Source::Script);
        }
        Ok(rt)
    }

    /// Poignée de commande clonable.
    pub fn controller(&self) -> Controller {
        Controller {
            tx: self.tx.clone(),
        }
    }

    /// Soumet une commande et attend la réponse.
    pub fn submit(&self, cmd: Command, source: Source) -> Result<(), RejectReason> {
        self.controller().submit(cmd, source)
    }

    /// Soumet sans attendre.
    pub fn submit_async(&self, cmd: Command, source: Source) {
        self.controller().submit_async(cmd, source);
    }

    /// Lecture de l'état publié.
    pub fn published(&self) -> std::sync::RwLockReadGuard<'_, Published> {
        self.shared
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Instantané.
    pub fn snapshot(&self) -> SimState {
        self.published().snapshot.clone()
    }

    /// Accès partagé (API distante).
    pub fn shared(&self) -> Arc<RwLock<Published>> {
        self.shared.clone()
    }

    /// Abonnement aux événements.
    pub fn subscribe(&self) -> Receiver<AppEvent> {
        let (tx, rx) = channel();
        self.subscribers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(tx);
        rx
    }

    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> Request, timeout: Duration) -> Option<T> {
        let (tx, rx) = channel();
        self.tx.send(make(tx)).ok()?;
        rx.recv_timeout(timeout).ok()
    }

    /// Applique une nouvelle configuration.
    pub fn apply_config(&self, c: AppConfig) -> Result<(), ConfigError> {
        self.ask(
            |tx| Request::ApplyConfig(Box::new(c), tx),
            Duration::from_secs(10),
        )
        .unwrap_or_else(|| Err(ConfigError::new("(moteur)", "pas de réponse")))
    }

    /// Charge une trace (index dans le fichier) pour la suivre.
    pub fn load_track(&self, path: &Path, index: usize) -> Result<String, String> {
        self.ask(
            |tx| Request::LoadTrack(path.to_path_buf(), index, tx),
            Duration::from_secs(10),
        )
        .unwrap_or_else(|| Err("pas de réponse".into()))
    }

    /// Démarre ou suspend le suivi de trace.
    pub fn follow_track(&self, on: bool) {
        let _ = self.tx.send(Request::FollowTrack(on));
    }

    /// Revient au début de la trace.
    pub fn rewind_track(&self) {
        let _ = self.tx.send(Request::RewindTrack);
    }

    /// Décharge la trace.
    pub fn clear_track(&self) {
        let _ = self.tx.send(Request::ClearTrack);
    }

    /// Charge une trace GPX/KML comme route pour le pilote.
    pub fn load_route(&self, path: &Path, index: usize) -> Result<usize, String> {
        self.ask(
            |tx| Request::LoadRoute(path.to_path_buf(), index, tx),
            Duration::from_secs(10),
        )
        .unwrap_or_else(|| Err("pas de réponse".into()))
    }

    /// Charge un journal pour le rejeu.
    pub fn load_journal(&self, path: &Path) -> Result<usize, String> {
        self.ask(
            |tx| Request::LoadJournal(path.to_path_buf(), tx),
            Duration::from_secs(30),
        )
        .unwrap_or_else(|| Err("pas de réponse".into()))
    }

    /// Commande de rejeu.
    pub fn replay(&self, c: ReplayCommand) {
        let _ = self.tx.send(Request::Replay(c));
    }

    /// Ferme le rejeu.
    pub fn close_replay(&self) {
        let _ = self.tx.send(Request::CloseReplay);
    }

    /// Démarre l'enregistrement.
    pub fn start_recording(&self, path: &Path) -> Result<String, String> {
        self.ask(
            |tx| Request::StartRecording(path.to_path_buf(), tx),
            Duration::from_secs(10),
        )
        .unwrap_or_else(|| Err("pas de réponse".into()))
    }

    /// Arrête l'enregistrement.
    pub fn stop_recording(&self) {
        let _ = self.tx.send(Request::StopRecording);
    }

    /// Envoie une phrase utilisateur (terminal), une fois.
    pub fn send_user_sentence(&self, s: &str) {
        let _ = self.tx.send(Request::UserSentence(s.to_string()));
    }

    /// Vide le moniteur.
    pub fn clear_monitor(&self) {
        let _ = self.tx.send(Request::ClearMonitor);
    }

    /// Arrête le moteur et ses transports.
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(Request::Shutdown);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct LiveTransport {
    cfg: crate::config::TransportConfig,
    t: Box<dyn Transport>,
    last_state: TransportState,
}

struct Engine {
    cfg: AppConfig,
    sim: Simulator,
    shared: Arc<RwLock<Published>>,
    subscribers: Arc<Mutex<Vec<Sender<AppEvent>>>>,
    transports: Vec<LiveTransport>,
    transports_on: bool,
    inbox: Inbox,
    enc_rng: Xoshiro256StarStar,
    viewsync: Option<UdpSocket>,
    vs_counter: u64,
    journal: Option<JournalWriter>,
    track: Option<TrackPlayer>,
    following: bool,
    replay: Option<ReplayPlayer>,
    pending_user: Vec<String>,
    publish_acc: u32,
    monitor_seq: u64,
    sent_total: u64,
    rate_window: VecDeque<(Instant, u64, BTreeMap<String, u64>)>,
    trail_acc: u32,
    stop: bool,
}

impl Engine {
    fn new(
        cfg: AppConfig,
        sim: Simulator,
        shared: Arc<RwLock<Published>>,
        subscribers: Arc<Mutex<Vec<Sender<AppEvent>>>>,
    ) -> Self {
        let seed = sim.state().seed ^ 0x5EED_E4C0_DE00_0001;
        let mut e = Self {
            cfg,
            sim,
            shared,
            subscribers,
            transports: vec![],
            transports_on: false,
            inbox: Arc::new(Mutex::new(Vec::new())),
            enc_rng: Xoshiro256StarStar::seed_from_u64(seed),
            viewsync: None,
            vs_counter: 1,
            journal: None,
            track: None,
            following: false,
            replay: None,
            pending_user: vec![],
            publish_acc: 0,
            monitor_seq: 0,
            sent_total: 0,
            rate_window: VecDeque::new(),
            trail_acc: 0,
            stop: false,
        };
        e.build_transports();
        e
    }

    fn emit(&self, ev: AppEvent) {
        let mut subs = self
            .subscribers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        subs.retain(|s| s.send(ev.clone()).is_ok());
    }

    fn warn(&self, msg: impl Into<String>) {
        let m = msg.into();
        {
            let mut p = self
                .shared
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            p.warnings.push_back(m.clone());
            while p.warnings.len() > 50 {
                p.warnings.pop_front();
            }
        }
        self.emit(AppEvent::Warning { message: m });
    }

    fn hello_fn(&self) -> HelloFn {
        let opts = self.cfg.output.signalk.clone();
        Arc::new(move || signalk::hello(&opts, APP_VERSION, now_ms()))
    }

    fn build_transports(&mut self) {
        self.stop_transports();
        self.transports = self
            .cfg
            .transports
            .iter()
            .filter(|t| t.enabled)
            .map(|t| {
                let hello = (t.format == OutputFormat::Signalk).then(|| self.hello_fn());
                LiveTransport {
                    cfg: t.clone(),
                    t: nmeasim_transport::build(&t.id, &t.spec, hello, self.inbox.clone()),
                    last_state: TransportState::Stopped,
                }
            })
            .collect();
    }

    fn start_transports(&mut self) {
        if self.transports_on {
            return;
        }
        self.transports_on = true;
        let mut errs = vec![];
        for lt in &mut self.transports {
            if let Err(e) = lt.t.start() {
                errs.push(format!("transport {} : {e}", lt.cfg.id));
            }
        }
        for e in errs {
            self.warn(e);
        }
        if self.cfg.viewsync.enabled {
            self.viewsync = UdpSocket::bind("0.0.0.0:0").ok();
        }
    }

    fn stop_transports(&mut self) {
        for lt in &mut self.transports {
            lt.t.stop();
        }
        self.transports_on = false;
        self.viewsync = None;
    }

    fn sync_transports(&mut self) {
        let want = self.sim.state().running || self.replay.is_some();
        if want && !self.transports_on {
            self.start_transports();
        } else if !want && self.transports_on {
            self.stop_transports();
        }
    }

    fn producer(&self) -> Producer {
        if self.replay.is_some() {
            Producer::Replay
        } else if self.following && self.track.is_some() {
            Producer::Track
        } else {
            Producer::Live
        }
    }

    fn submit(&mut self, cmd: Command, source: Source) -> Result<(), RejectReason> {
        let kind = cmd.kind();
        // Un rejeu détient le jeton : la simulation live ne produit rien.
        if self.replay.is_some() && matches!(cmd, Command::Start) {
            let r = Err(RejectReason::ProducerLocked);
            self.emit(AppEvent::CommandRejected {
                kind,
                source,
                reason: RejectReason::ProducerLocked,
            });
            return r;
        }
        let res = self.sim.submit(cmd, &source);
        match &res {
            Ok(()) => {
                if !matches!(source, Source::Track | Source::Replay) {
                    self.emit(AppEvent::CommandAccepted { kind, source });
                }
            }
            Err(reason) => self.emit(AppEvent::CommandRejected {
                kind,
                source,
                reason: reason.clone(),
            }),
        }
        for ev in self.sim.take_events() {
            if matches!(ev, SimEvent::Stopped) && self.following {
                self.following = false;
                let _ = self.sim.submit(Command::ReleaseSource, &Source::Track);
            }
            self.emit(AppEvent::Sim { detail: ev });
        }
        self.sync_transports();
        res
    }

    fn apply_config(&mut self, c: AppConfig) -> Result<(), ConfigError> {
        c.validate()?;
        let sim_changed = c.simulation != self.cfg.simulation;
        let tr_changed =
            c.transports != self.cfg.transports || c.output.signalk != self.cfg.output.signalk;
        let vs_changed = c.viewsync != self.cfg.viewsync;
        if sim_changed {
            let running = self.sim.state().running;
            let mut sim = Simulator::new(c.simulation.clone())
                .map_err(|e| ConfigError::new(format!("simulation.{}", e.field), e.reason))?;
            if running {
                let _ = sim.submit(Command::Start, &Source::Script);
                sim.take_events();
            }
            self.sim = sim;
        }
        self.cfg = c;
        if tr_changed {
            let was_on = self.transports_on;
            self.build_transports();
            if was_on {
                self.start_transports();
            }
        } else if vs_changed && self.transports_on {
            self.viewsync = if self.cfg.viewsync.enabled {
                UdpSocket::bind("0.0.0.0:0").ok()
            } else {
                None
            };
        }
        self.shared
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .config = self.cfg.clone();
        Ok(())
    }

    fn kml_mode(&self) -> KmlMode {
        if self.cfg.track.kml_legacy {
            KmlMode::Legacy
        } else {
            KmlMode::Modern
        }
    }

    /// Répond après avoir publié l'état : l'appelant lit un état à jour.
    fn done<T>(&mut self, tx: Sender<T>, v: T) {
        self.refresh_shared();
        reply(tx, v);
    }

    fn handle(&mut self, r: Request) {
        match r {
            Request::Command(c, s, tx) => {
                let res = self.submit(c, s);
                if let Some(tx) = tx {
                    self.done(tx, res);
                }
            }
            Request::ApplyConfig(c, tx) => {
                let res = self.apply_config(*c);
                self.done(tx, res);
            }
            Request::LoadTrack(path, idx, tx) => {
                let res = read_track_file(&path, self.kml_mode())
                    .map_err(|e| e.0)
                    .and_then(|f| {
                        let t = f.tracks.get(idx).cloned().ok_or_else(|| {
                            format!("trace {idx} absente ({} traces)", f.tracks.len())
                        })?;
                        let name = t.name.clone();
                        let player =
                            TrackPlayer::new(t, self.cfg.track.play_mode, self.cfg.track.repeat)
                                .map_err(|e| e.0)?;
                        let first = player.track().points[0].position;
                        let _ = self
                            .sim
                            .submit(Command::SetPosition { position: first }, &Source::Track);
                        self.track = Some(player);
                        self.following = true;
                        Ok(name)
                    });
                self.done(tx, res);
            }
            Request::FollowTrack(on) => {
                self.following = on && self.track.is_some();
                if !self.following {
                    let _ = self.sim.submit(Command::ReleaseSource, &Source::Track);
                }
            }
            Request::RewindTrack => {
                if let Some(p) = self.track.as_mut() {
                    p.rewind();
                    let first = p.track().points[0].position;
                    let _ = self
                        .sim
                        .submit(Command::SetPosition { position: first }, &Source::Track);
                }
            }
            Request::ClearTrack => {
                self.track = None;
                self.following = false;
                let _ = self.sim.submit(Command::ReleaseSource, &Source::Track);
            }
            Request::LoadRoute(path, idx, tx) => {
                let res = read_track_file(&path, self.kml_mode())
                    .map_err(|e| e.0)
                    .and_then(|f| {
                        let pts: Vec<LatLon> = match f.tracks.get(idx) {
                            Some(t) => t.points.iter().map(|p| p.position).collect(),
                            None => f.waypoints.iter().map(|w| w.position).collect(),
                        };
                        let n = pts.len();
                        self.submit(Command::SetRoute { points: pts }, Source::Script)
                            .map_err(|e| e.to_string())?;
                        Ok(n)
                    });
                self.done(tx, res);
            }
            Request::LoadJournal(path, tx) => {
                let res = journal::read(&path).map_err(|e| e.0).map(|j| {
                    if let Some(w) = &j.warning {
                        self.warn(w.clone());
                    }
                    let n = j.blocks.len();
                    self.replay = Some(ReplayPlayer::new(j));
                    n
                });
                self.sync_transports();
                self.done(tx, res);
            }
            Request::Replay(c) => {
                if let Some(r) = self.replay.as_mut() {
                    if let Some(i) = r.command(c) {
                        self.emit_replay_block(i);
                    }
                }
            }
            Request::CloseReplay => {
                self.replay = None;
                let _ = self.sim.submit(Command::ReleaseSource, &Source::Replay);
                self.sync_transports();
            }
            Request::StartRecording(path, tx) => {
                let meta = self.cfg.logging.meta.then(|| {
                    json!({
                        "format": journal::META_FORMAT,
                        "recordedAt": nmeasim_encode::sentence::iso(now_ms()),
                        "intervalMs": self.cfg.output.interval_ms,
                        "stepMs": self.cfg.simulation.step_ms,
                        "seed": self.sim.state().seed,
                        "output": "nmea",
                        "compatibility": format!("{:?}", self.sim.compat().profile).to_lowercase(),
                        "app": format!("nmeasim-rs {APP_VERSION}"),
                    })
                });
                let res = JournalWriter::create(&path, APP_VERSION, meta.as_ref())
                    .map_err(|e| e.0)
                    .map(|w| {
                        let p = w.path().display().to_string();
                        self.journal = Some(w);
                        p
                    });
                if let Ok(p) = &res {
                    self.emit(AppEvent::RecordingStarted { path: p.clone() });
                }
                self.done(tx, res);
            }
            Request::StopRecording => {
                if let Some(w) = self.journal.take() {
                    let (_, dropped, err) = w.close();
                    if dropped > 0 {
                        self.warn(format!("journal : {dropped} blocs abandonnés"));
                    }
                    if let Some(e) = err {
                        self.warn(format!("journal : {e}"));
                    }
                    self.emit(AppEvent::RecordingStopped);
                }
            }
            Request::UserSentence(s) => {
                let s = s.trim().to_string();
                if !s.is_empty() {
                    self.pending_user.push(s);
                }
            }
            Request::ClearMonitor => {
                self.shared
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .monitor
                    .clear();
            }
            Request::Shutdown => self.stop = true,
        }
    }

    fn monitor(&mut self, entries: Vec<(Direction, String, String)>) {
        if entries.is_empty() {
            return;
        }
        let t = now_ms();
        let max = self.cfg.ui.monitor_lines.max(100);
        let mut p = self
            .shared
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (direction, channel, text) in entries {
            self.monitor_seq += 1;
            p.monitor.push_back(MonitorEntry {
                seq: self.monitor_seq,
                time_ms: t,
                direction,
                channel,
                text,
            });
        }
        while p.monitor.len() > max {
            p.monitor.pop_front();
        }
    }

    fn send_frames(&mut self, nmea_frames: &[Frame], json_frame: Option<&Frame>) {
        for lt in &mut self.transports {
            match lt.cfg.format {
                OutputFormat::Nmea => {
                    for f in nmea_frames {
                        lt.t.send(f);
                    }
                }
                OutputFormat::Signalk => {
                    if let Some(f) = json_frame {
                        lt.t.send(f);
                    }
                }
            }
        }
    }

    fn count(&mut self, lines: &[String]) {
        self.sent_total += lines.len() as u64;
        let mut by = BTreeMap::new();
        for l in lines {
            if let Some(p) = parse::parse(l) {
                *by.entry(format!("{}{}", p.talker, p.formatter))
                    .or_insert(0u64) += 1;
            }
        }
        let now = Instant::now();
        self.rate_window.push_back((now, lines.len() as u64, by));
        while self
            .rate_window
            .front()
            .is_some_and(|(t, _, _)| now.duration_since(*t) > Duration::from_secs(10))
        {
            self.rate_window.pop_front();
        }
    }

    fn emit_replay_block(&mut self, i: usize) {
        let Some(r) = self.replay.as_ref() else {
            return;
        };
        let block = r.journal().blocks[i].clone();
        let lines = r.journal().lines(i);
        let frames: Vec<Frame> = if self.sim.compat().legacy_sentences {
            // Legacy : le bloc entier en une seule écriture.
            vec![Frame::raw(block)]
        } else {
            lines.iter().map(|l| Frame::sentence(l.clone())).collect()
        };
        // Le journal pilote aussi le navire (position de la carte).
        if let Some(rmc) = lines
            .iter()
            .filter_map(|l| parse::parse(l))
            .find(|p| p.formatter == "RMC")
        {
            if let (Some(lat), Some(lon)) = (
                parse::coord(rmc.f(2), rmc.f(3)),
                parse::coord(rmc.f(4), rmc.f(5)),
            ) {
                let _ = self.sim.submit(
                    Command::TrackPoint {
                        position: LatLon::new(lat, lon),
                        sog_kn: rmc.num(6),
                        cog_deg: rmc.num(7),
                        altitude: None,
                        time_ms: None,
                    },
                    &Source::Replay,
                );
            }
        }
        for lt in &mut self.transports {
            for f in &frames {
                lt.t.send(f);
            }
        }
        if let Some(j) = &self.journal {
            j.push(&lines);
        }
        self.count(&lines);
        self.monitor(
            lines
                .into_iter()
                .map(|l| (Direction::Out, "replay".into(), l))
                .collect(),
        );
    }

    fn tz_offset(&self) -> i32 {
        self.cfg
            .output
            .nmea
            .zda_offset_minutes
            .unwrap_or_else(|| chrono::Local::now().offset().local_minus_utc() / 60)
    }

    fn publish(&mut self) {
        // Suivi de trace : un point par tick de sortie.
        if self.following && self.sim.state().running && !self.sim.state().paused {
            let interval = i64::from(self.cfg.output.interval_ms);
            let out = self.track.as_mut().and_then(|p| p.next(interval));
            match out {
                Some(o) => {
                    let _ = self.sim.submit(
                        Command::TrackPoint {
                            position: o.position,
                            sog_kn: o.sog_kn,
                            cog_deg: o.cog_deg,
                            altitude: o.altitude,
                            time_ms: o.time_ms,
                        },
                        &Source::Track,
                    );
                    if o.ended && !self.cfg.track.repeat {
                        self.following = false;
                        let _ = self.sim.submit(Command::ReleaseSource, &Source::Track);
                        // Comportement legacy : arrêt en fin de trace.
                        let _ = self.submit(Command::Stop, Source::Track);
                        self.emit(AppEvent::TrackEnded);
                        self.warn("End of track has been reached.");
                    }
                }
                None => self.following = false,
            }
        }
        if !self.sim.state().running {
            return;
        }
        self.sim.publish_tick();
        let state = self.sim.snapshot();
        let ctx = Ctx {
            state: &state,
            compat: self.sim.compat(),
            opts: &self.cfg.output.nmea,
            tz_offset_minutes: self.tz_offset(),
            vessel_length_m: self.cfg.simulation.vessel.length_m,
        };
        let mut lines = nmea::block(&ctx, &mut self.enc_rng as &mut dyn Rng);
        let mut entries: Vec<(Direction, String, String)> = lines
            .iter()
            .map(|l| (Direction::Out, "nmea".into(), l.clone()))
            .collect();
        for u in std::mem::take(&mut self.pending_user) {
            let s = if self.cfg.output.nmea.prefix {
                format!(
                    "{}{u}",
                    prefix(
                        state.sentence_time_ms(),
                        &self.cfg.output.nmea.prefix_source
                    )
                )
            } else {
                u
            };
            entries.push((Direction::Out, "user".into(), s.clone()));
            lines.push(s);
        }
        let frames: Vec<Frame> = lines.iter().map(|l| Frame::sentence(l.clone())).collect();
        let has_sk = self
            .transports
            .iter()
            .any(|t| t.cfg.format == OutputFormat::Signalk);
        let delta =
            has_sk.then(|| signalk::delta(&state, self.sim.compat(), &self.cfg.output.signalk));
        if let Some(d) = &delta {
            entries.push((Direction::Out, "signalk".into(), d.clone()));
        }
        self.send_frames(
            &frames,
            delta.as_ref().map(|d| Frame::json(d.clone())).as_ref(),
        );
        if let (true, Some(sock)) = (self.cfg.viewsync.enabled, &self.viewsync) {
            let pkt = viewsync::packet(
                self.vs_counter,
                &state,
                &self.cfg.viewsync,
                self.cfg.output.interval_ms,
            );
            self.vs_counter += 1;
            let _ = sock.send_to(
                pkt.as_bytes(),
                (self.cfg.viewsync.host.as_str(), self.cfg.viewsync.port),
            );
        }
        if let Some(j) = &self.journal {
            j.push(&lines);
        }
        self.count(&lines);
        self.monitor(entries);
    }

    fn drain_inbox(&mut self) {
        let lines: Vec<String> = std::mem::take(
            &mut *self
                .inbox
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        self.monitor(
            lines
                .into_iter()
                .map(|l| (Direction::In, "input".into(), l))
                .collect(),
        );
    }

    fn refresh_shared(&mut self) {
        let statuses: Vec<TransportStatus> = self.transports.iter().map(|t| t.t.status()).collect();
        let mut changes = vec![];
        for (lt, st) in self.transports.iter_mut().zip(&statuses) {
            if st.state != lt.last_state {
                lt.last_state = st.state;
                changes.push(AppEvent::TransportState {
                    id: st.id.clone(),
                    state: st.state,
                    error: st.last_error.clone(),
                });
            }
        }
        for c in changes {
            self.emit(c);
        }
        let producer = self.producer();
        let snap = self.sim.snapshot();
        let track = self.track.as_ref().map(|p| TrackInfo {
            name: p.track().name.clone(),
            index: p.index(),
            total: p.track().points.len(),
            points: p.track().points.iter().map(|x| x.position).collect(),
            following: self.following,
            ended: p.ended(),
        });
        let replay = self.replay.as_ref().map(ReplayPlayer::status);
        if replay.as_ref().is_some_and(|r| r.ended && !r.playing) {
            // signalé une seule fois par le lecteur ; rien à faire ici
        }
        let recording = self.journal.as_ref().map(|j| {
            let (written, dropped, error) = j.stats();
            RecordingInfo {
                path: j.path().display().to_string(),
                written,
                dropped,
                error,
            }
        });
        let (total, window): (u64, f64) = {
            let n: u64 = self.rate_window.iter().map(|(_, n, _)| n).sum();
            let secs = self
                .rate_window
                .front()
                .map_or(1.0, |(t, _, _)| t.elapsed().as_secs_f64().max(1.0));
            (n, secs)
        };
        let mut rates = BTreeMap::new();
        for (_, _, by) in &self.rate_window {
            for (k, v) in by {
                *rates.entry(k.clone()).or_insert(0.0) += *v as f64 / window;
            }
        }
        let trail_max = self.cfg.ui.map.trail_points.max(10);
        let mut p = self
            .shared
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if snap.running && !snap.paused {
            self.trail_acc += 1;
            if self.trail_acc >= 25 || p.trail.is_empty() {
                self.trail_acc = 0;
                p.trail.push_back(snap.vessel.position);
                while p.trail.len() > trail_max {
                    p.trail.pop_front();
                }
            }
        }
        p.snapshot = snap;
        p.transports = statuses;
        p.producer = producer;
        p.track = track;
        p.replay = replay;
        p.recording = recording;
        p.sentences_sent = self.sent_total;
        p.sentences_per_s = total as f64 / window;
        p.sentence_rates = rates;
    }

    fn run(&mut self, rx: Receiver<Request>) {
        let mut next = Instant::now();
        let mut replay_was_playing = false;
        let mut last_refresh = Instant::now();
        while !self.stop {
            // Le pas ne dépasse jamais l'intervalle de sortie (intervalles < 20 ms).
            let step = self
                .cfg
                .simulation
                .step_ms
                .min(self.cfg.output.interval_ms)
                .max(1);
            let now = Instant::now();
            if next > now {
                match rx.recv_timeout(next - now) {
                    Ok(r) => {
                        self.handle(r);
                        while let Ok(r) = rx.try_recv() {
                            self.handle(r);
                        }
                        self.refresh_shared();
                        continue;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            next += Duration::from_millis(u64::from(step));
            if Instant::now().duration_since(next) > Duration::from_secs(1) {
                next = Instant::now();
            }
            self.sim.step(step);
            for ev in self.sim.take_events() {
                self.emit(AppEvent::Sim { detail: ev });
            }
            if let Some(r) = self.replay.as_mut() {
                let blocks = r.tick(f64::from(step), f64::from(self.cfg.output.interval_ms));
                let playing = r.status().playing;
                let ended = r.status().ended;
                for i in blocks {
                    self.emit_replay_block(i);
                }
                if replay_was_playing && !playing && ended {
                    self.emit(AppEvent::ReplayEnded);
                }
                replay_was_playing = playing;
            } else {
                self.publish_acc += step;
                if self.publish_acc >= self.cfg.output.interval_ms {
                    self.publish_acc -= self.cfg.output.interval_ms;
                    self.publish();
                }
            }
            self.drain_inbox();
            // État publié rafraîchi à 50 Hz au plus : aux intervalles de sortie
            // très courts (1 ms), le rafraîchir à chaque pas limite le débit.
            if last_refresh.elapsed() >= Duration::from_millis(20) {
                last_refresh = Instant::now();
                self.refresh_shared();
            }
            if self.sim.state().autopilot.mode == AutopilotMode::Route
                && self.sim.state().route.destination().is_none()
            {
                let _ = self.sim.submit(
                    Command::SetAutopilot {
                        mode: AutopilotMode::Off,
                    },
                    &Source::Script,
                );
            }
        }
        if let Some(w) = self.journal.take() {
            w.close();
        }
        self.stop_transports();
    }
}

/// Phrase NMEA utilisateur valide ? Complète le checksum s'il manque.
pub fn complete_user_sentence(s: &str) -> Option<String> {
    let s = s.trim();
    if !(s.starts_with('$') || s.starts_with('!')) || s.len() < 6 {
        return None;
    }
    if s.contains('*') {
        return Some(s.to_string());
    }
    let (start, body) = s.split_at(1);
    let mut parts = body.split(',');
    let addr = parts.next()?;
    if addr.len() < 5 {
        return None;
    }
    let mut b = Sentence::new(start.chars().next()?, &addr[..2], &addr[2..]);
    for f in parts {
        b = b.field(f);
    }
    Some(b.finish())
}
