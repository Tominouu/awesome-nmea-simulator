//! Fenêtre principale. L'interface ne modifie jamais l'état du navire : elle
//! lit l'état publié et soumet des commandes à l'API de contrôle.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use egui::{Color32, RichText};
use nmeasim_app::config::{self, AppConfig};
use nmeasim_app::gamepad::GamepadDriver;
use nmeasim_app::http::ApiServer;
use nmeasim_app::runtime::{AppEvent, Producer};
use nmeasim_app::{APP_VERSION, Runtime};
use nmeasim_core::units::{kelvin_to_celsius, mps_to_knots, normalize_deg, rad_to_deg};
use nmeasim_input::keyboard::{self, Key, KeyContext};
use nmeasim_sim::command::{Command, EngineSel, Source};
use nmeasim_sim::compat::Profile;
use nmeasim_sim::state::AutopilotMode;

use crate::map::{MapAction, MapView, fmt_deg};
use crate::panels;

/// Onglets du bas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Moniteur NMEA.
    Nmea,
    /// Réseau.
    Network,
    /// Manette.
    Gamepad,
    /// Traces et rejeu.
    Sources,
    /// Journal d'événements et enregistrement.
    Log,
    /// Scénarios.
    Scenario,
    /// Réglages.
    Settings,
}

impl Tab {
    const ALL: [Tab; 7] = [
        Tab::Nmea,
        Tab::Network,
        Tab::Gamepad,
        Tab::Sources,
        Tab::Log,
        Tab::Scenario,
        Tab::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::Nmea => "NMEA",
            Tab::Network => "NETWORK",
            Tab::Gamepad => "GAMEPAD",
            Tab::Sources => "GPX / KML / REPLAY",
            Tab::Log => "LOG",
            Tab::Scenario => "SCENARIO",
            Tab::Settings => "SETTINGS",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Tab::Nmea => "nmea",
            Tab::Network => "network",
            Tab::Gamepad => "gamepad",
            Tab::Sources => "sources",
            Tab::Log => "log",
            Tab::Scenario => "scenario",
            Tab::Settings => "settings",
        }
    }
}

/// État du moniteur NMEA.
#[derive(Default)]
pub struct MonitorUi {
    /// Pause d'affichage.
    pub paused: bool,
    /// Instantané gelé pendant la pause.
    pub frozen: Vec<nmeasim_app::runtime::MonitorEntry>,
    /// Recherche plein texte.
    pub search: String,
    /// Filtre formateur.
    pub sentence: String,
    /// Filtre talker.
    pub talker: String,
    /// Afficher les entrées reçues.
    pub show_input: bool,
    /// Afficher Signal K.
    pub show_signalk: bool,
    /// Saisie du terminal.
    pub terminal: String,
}

/// Application.
pub struct NmeaSimApp {
    /// Moteur.
    pub rt: Runtime,
    /// Fichier de configuration.
    pub cfg_path: PathBuf,
    /// Copie éditable de la configuration.
    pub draft: AppConfig,
    /// Manette.
    pub gamepad: Option<GamepadDriver>,
    /// API distante.
    pub api: Option<ApiServer>,
    /// Carte.
    pub map: MapView,
    /// Événements.
    pub events: Receiver<AppEvent>,
    /// Journal d'affichage.
    pub log: VecDeque<(String, String)>,
    /// Onglet actif.
    pub tab: Tab,
    /// Moniteur.
    pub monitor: MonitorUi,
    /// Message d'état (bas de fenêtre).
    pub status: String,
    /// Scénario courant.
    pub scenario_path: Option<PathBuf>,
    /// Nom du scénario.
    pub scenario_name: String,
    /// Description du scénario.
    pub scenario_desc: String,
    /// Assistant de calibration : axe et bornes mesurées.
    pub calib: Option<(nmeasim_input::Axis, f64, f64)>,
    /// Profil de manette en cours d'édition.
    pub pad_profile: nmeasim_input::Profile,
    /// Cap saisi pour le pilote.
    pub ap_heading: f64,
    /// Thème sombre.
    pub dark: bool,
    /// Fond de carte en tuiles (créé à la demande).
    pub tiles: Option<crate::tiles::TileCache>,
}

fn now_hms() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

impl NmeaSimApp {
    fn new(
        cfg: AppConfig,
        warnings: Vec<String>,
        cfg_path: PathBuf,
        scenario: Option<PathBuf>,
    ) -> Result<Self, String> {
        let scenario_cmds = match &scenario {
            Some(p) => nmeasim_app::scenario::load(p)
                .map(|s| s.commands())
                .unwrap_or_default(),
            None => vec![],
        };
        let rt = Runtime::spawn(cfg.clone()).map_err(|e| e.to_string())?;
        for c in scenario_cmds {
            let _ = rt.submit(c, Source::Script);
        }
        let events = rt.subscribe();
        let gamepad = cfg
            .gamepad
            .enabled
            .then(|| GamepadDriver::spawn(rt.controller(), cfg.gamepad.active_profile()));
        let mut log = VecDeque::new();
        for w in warnings {
            log.push_back((now_hms(), w));
        }
        let api = if cfg.api.enabled {
            match ApiServer::start(&cfg.api, rt.controller(), rt.shared()) {
                Ok(s) => {
                    log.push_back((now_hms(), format!("API HTTP : http://{}/api/v1", s.addr)));
                    Some(s)
                }
                Err(e) => {
                    log.push_back((now_hms(), format!("API HTTP indisponible : {e}")));
                    None
                }
            }
        } else {
            None
        };
        let pos = rt.snapshot().vessel.position;
        let tab = Tab::ALL
            .iter()
            .copied()
            .find(|t| t.id() == cfg.ui.bottom_tab)
            .unwrap_or(Tab::Nmea);
        Ok(Self {
            map: MapView::new(pos, cfg.ui.map.zoom, cfg.ui.map.follow),
            pad_profile: cfg.gamepad.active_profile(),
            dark: cfg.ui.theme != "light",
            draft: cfg,
            rt,
            cfg_path,
            gamepad,
            api,
            events,
            log,
            tab,
            monitor: MonitorUi {
                show_input: true,
                ..Default::default()
            },
            status: String::new(),
            scenario_name: scenario
                .as_ref()
                .and_then(|p| p.file_stem())
                .map_or("Nouveau scénario".into(), |s| s.to_string_lossy().into()),
            scenario_path: scenario,
            scenario_desc: String::new(),
            calib: None,
            ap_heading: 0.0,
            tiles: None,
        })
    }

    /// Soumet une commande depuis l'interface ; un refus est affiché.
    pub fn cmd(&mut self, c: Command) {
        self.cmd_from(c, Source::Ui);
    }

    /// Soumet une commande avec une source donnée.
    pub fn cmd_from(&mut self, c: Command, s: Source) {
        let kind = c.kind();
        if let Err(e) = self.rt.submit(c, s) {
            self.status = format!("{kind} refusée : {e}");
        }
    }

    /// Applique et enregistre la configuration éditée.
    pub fn apply_draft(&mut self) {
        match self.rt.apply_config(self.draft.clone()) {
            Ok(()) => {
                if let Some(g) = &self.gamepad {
                    g.set_profile(self.draft.gamepad.active_profile());
                }
                match config::save(&self.cfg_path, &self.draft) {
                    Ok(()) => {
                        self.status = format!(
                            "configuration appliquée et enregistrée ({})",
                            self.cfg_path.display()
                        )
                    }
                    Err(e) => self.status = format!("appliquée, mais non enregistrée : {e}"),
                }
            }
            Err(e) => self.status = format!("configuration refusée : {e}"),
        }
    }

    /// Journalise une ligne.
    pub fn note(&mut self, s: impl Into<String>) {
        self.log.push_back((now_hms(), s.into()));
        while self.log.len() > 1000 {
            self.log.pop_front();
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let legacy = self.rt.published().config.simulation.compatibility.profile == Profile::Legacy;
        let keys: Vec<(Key, bool)> = ctx.input(|i| {
            let mut v = vec![];
            let shift = i.modifiers.shift;
            let map = [
                (egui::Key::ArrowUp, Key::Up),
                (egui::Key::ArrowDown, Key::Down),
                (egui::Key::ArrowLeft, Key::Left),
                (egui::Key::ArrowRight, Key::Right),
                (egui::Key::PageUp, Key::PageUp),
                (egui::Key::PageDown, Key::PageDown),
                (egui::Key::Home, Key::Home),
                (egui::Key::Space, Key::Space),
                (egui::Key::A, Key::Char('A')),
                (egui::Key::E, Key::Char('E')),
                (egui::Key::N, Key::Char('N')),
                (egui::Key::M, Key::Char('M')),
                (egui::Key::W, Key::Char('W')),
                (egui::Key::Num0, Key::Char('0')),
            ];
            for (ek, k) in map {
                if i.key_pressed(ek) {
                    v.push((k, shift));
                }
            }
            v
        });
        for (k, shift) in keys {
            if let Some(c) = keyboard::command_for(k, KeyContext { legacy, shift }) {
                self.cmd_from(c, Source::Keyboard);
            }
        }
    }

    fn drain_events(&mut self) {
        let evs: Vec<AppEvent> = self.events.try_iter().collect();
        for e in evs {
            let line = match &e {
                AppEvent::CommandAccepted { .. } => continue,
                AppEvent::CommandRejected {
                    kind,
                    source,
                    reason,
                } => format!("refus {kind} ({source:?}) : {reason}"),
                AppEvent::Sim { detail } => format!("{detail:?}"),
                AppEvent::TransportState { id, state, error } => {
                    format!(
                        "transport {id} : {state:?}{}",
                        error
                            .as_ref()
                            .map(|e| format!(" — {e}"))
                            .unwrap_or_default()
                    )
                }
                AppEvent::TrackEnded => "fin de trace : End of track has been reached.".into(),
                AppEvent::ReplayEnded => "fin du rejeu".into(),
                AppEvent::RecordingStarted { path } => format!("enregistrement : {path}"),
                AppEvent::RecordingStopped => "enregistrement arrêté".into(),
                AppEvent::Warning { message } => format!("⚠ {message}"),
            };
            self.note(line);
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let (running, paused, producer, profile, rec) = {
            let p = self.rt.published();
            (
                p.snapshot.running,
                p.snapshot.paused,
                p.producer,
                p.config.simulation.compatibility.profile,
                p.recording.is_some(),
            )
        };
        ui.horizontal(|ui| {
            ui.heading(RichText::new("⚓ NMEA SIMULATOR").strong());
            ui.separator();
            let (txt, col) = match (running, paused) {
                (true, false) => ("● RUNNING", Color32::from_rgb(80, 220, 100)),
                (true, true) => ("● PAUSED", Color32::from_rgb(240, 190, 60)),
                _ => ("● STOPPED", Color32::from_rgb(220, 80, 80)),
            };
            ui.label(RichText::new(txt).color(col).strong());
            if producer != Producer::Live {
                ui.label(
                    RichText::new(format!("source : {producer:?}")).color(Color32::LIGHT_BLUE),
                );
            }
            if rec {
                ui.label(RichText::new("⏺ REC").color(Color32::RED));
            }
            ui.separator();
            if running {
                if ui.button("⏹ Stop").clicked() {
                    self.cmd(Command::Stop);
                }
                if ui
                    .button(if paused { "▶ Reprendre" } else { "⏸ Pause" })
                    .clicked()
                {
                    self.cmd(Command::TogglePause);
                }
            } else if ui.button("▶ Start").clicked() {
                self.cmd(Command::Start);
            }
            if ui.button("↺ Reset").clicked() {
                self.cmd(Command::Reset);
            }
            ui.separator();
            ui.label(format!(
                "profil : {}",
                if profile == Profile::Legacy {
                    "legacy"
                } else {
                    "modern"
                }
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("v{APP_VERSION}")).weak());
                if ui
                    .button(if self.dark { "☀" } else { "🌙" })
                    .on_hover_text("thème")
                    .clicked()
                {
                    self.dark = !self.dark;
                    self.draft.ui.theme = if self.dark {
                        "dark".into()
                    } else {
                        "light".into()
                    };
                }
            });
        });
    }

    fn vessel_panel(&mut self, ui: &mut egui::Ui) {
        let s = self.rt.snapshot();
        let v = &s.vessel;
        ui.heading("VESSEL");
        ui.add_space(4.0);
        let big = |ui: &mut egui::Ui, k: &str, val: String| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{k:<6}")).monospace().weak());
                ui.label(RichText::new(val).monospace().size(20.0).strong());
            });
        };
        big(ui, "SOG", format!("{:5.1} kn", mps_to_knots(v.sog)));
        big(
            ui,
            "COG",
            format!("{:5.0}°", normalize_deg(rad_to_deg(v.cog))),
        );
        big(
            ui,
            "HDG",
            format!("{:5.0}°", normalize_deg(rad_to_deg(v.heading))),
        );
        big(ui, "STW", format!("{:5.1} kn", mps_to_knots(v.stw)));
        big(ui, "ROT", format!("{:+5.1}°/s", rad_to_deg(v.rate_of_turn)));
        big(ui, "DEPTH", format!("{:5.1} m", s.water.depth));
        big(
            ui,
            "RPM",
            format!(
                "{:5.0}",
                s.engines.iter().map(|e| e.rpm).fold(0.0, f64::max)
            ),
        );
        ui.separator();
        egui::Grid::new("vessel-grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Position");
                ui.label(
                    RichText::new(format!(
                        "{}\n{}",
                        fmt_deg(v.position.lat, true),
                        fmt_deg(v.position.lon, false)
                    ))
                    .monospace(),
                );
                ui.end_row();
                ui.label("Vent vrai");
                ui.label(format!(
                    "{:.0}° {:.1} kn",
                    normalize_deg(rad_to_deg(s.wind.true_direction)),
                    mps_to_knots(s.wind.true_speed)
                ));
                ui.end_row();
                ui.label("Vent app.");
                ui.label(format!(
                    "{:+.0}° {:.1} kn",
                    rad_to_deg(s.wind.apparent_angle),
                    mps_to_knots(s.wind.apparent_speed)
                ));
                ui.end_row();
                ui.label("Eau");
                ui.label(format!("{:.1} °C", kelvin_to_celsius(s.water.temperature)));
                ui.end_row();
                ui.label("Courant");
                ui.label(format!(
                    "{:.0}° {:.1} kn",
                    normalize_deg(rad_to_deg(s.water.current_set)),
                    mps_to_knots(s.water.current_drift)
                ));
                ui.end_row();
                ui.label("Barre");
                ui.label(format!(
                    "{:+.1}° (cons. {:+.1}°)",
                    v.rudder_angle, v.rudder_command
                ));
                ui.end_row();
                ui.label("GNSS");
                ui.label(format!(
                    "{} sat, HDOP {:.1}",
                    s.gnss.used.len(),
                    s.gnss.hdop
                ));
                ui.end_row();
                for e in &s.engines {
                    ui.label(&e.label);
                    ui.label(format!(
                        "{} {:.0} rpm {:.0} °C",
                        if e.running { "●" } else { "○" },
                        e.rpm,
                        kelvin_to_celsius(e.temperature)
                    ));
                    ui.end_row();
                }
                ui.label("Pilote");
                ui.label(match s.autopilot.mode {
                    AutopilotMode::Off => "manuel".to_string(),
                    AutopilotMode::Heading => format!(
                        "cap {:.0}°",
                        normalize_deg(rad_to_deg(s.autopilot.heading_target))
                    ),
                    AutopilotMode::Route => "route".to_string(),
                });
                ui.end_row();
                if let (Some(d), Some(g)) = (s.route.destination(), s.route.geometry.as_ref()) {
                    ui.label("Destination");
                    ui.label(format!(
                        "{} {:.2} NM / {:.0}°\nXTE {:.3} NM",
                        d.name,
                        g.dtg / 1852.0,
                        normalize_deg(rad_to_deg(g.bearing_to_dest)),
                        g.xte / 1852.0
                    ));
                    ui.end_row();
                }
                ui.label("Mouillage");
                ui.label(if v.anchored {
                    "⚓ au mouillage"
                } else {
                    "—"
                });
                ui.end_row();
            });
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        let s = self.rt.snapshot();
        let max_r = self.draft.simulation.vessel.max_rudder_deg;
        ui.horizontal(|ui| {
            ui.label(RichText::new("THROTTLE").strong());
            let thr = s.engines[0].throttle;
            let mut t = thr;
            let bar = egui::ProgressBar::new(thr.abs() as f32)
                .desired_width(160.0)
                .text(format!(
                    "{}{:.0} %",
                    if thr < 0.0 { "R " } else { "" },
                    thr.abs() * 100.0
                ));
            ui.add(bar);
            if ui
                .add(egui::Slider::new(&mut t, -1.0..=1.0).show_value(false))
                .changed()
            {
                self.cmd(Command::SetThrottle {
                    engine: EngineSel::All,
                    value: t,
                });
            }
            if ui.button("0").on_hover_text("point mort").clicked() {
                self.cmd(Command::SetThrottle {
                    engine: EngineSel::All,
                    value: 0.0,
                });
            }
            ui.separator();
            ui.label(RichText::new("RUDDER").strong());
            let mut r = s.vessel.rudder_command;
            if ui
                .add(
                    egui::Slider::new(&mut r, -max_r..=max_r)
                        .step_by(1.0)
                        .suffix("°"),
                )
                .changed()
            {
                self.cmd(Command::SetRudder { deg: r });
            }
            ui.label(RichText::new(format!("{:+.1}°", s.vessel.rudder_angle)).monospace());
            if ui.button("◆").on_hover_text("barre au centre").clicked() {
                self.cmd(Command::CenterRudder);
            }
            ui.separator();
            let eng = s.engines[0].running || s.engines[1].running;
            if ui.selectable_label(eng, "⚙ Moteurs").clicked() {
                self.cmd(Command::ToggleEngines);
            }
            let ap = s.autopilot.mode != AutopilotMode::Off;
            if ui.selectable_label(ap, "⎈ Pilote").clicked() {
                self.cmd(Command::ToggleAutopilot);
            }
            ui.add(
                egui::DragValue::new(&mut self.ap_heading)
                    .range(0.0..=359.0)
                    .suffix("°")
                    .speed(1.0),
            );
            if ui
                .button("Cap")
                .on_hover_text("consigne de cap du pilote")
                .clicked()
            {
                self.cmd(Command::SetAutopilotHeading {
                    deg: self.ap_heading,
                });
            }
            if s.route.destination().is_some()
                && ui
                    .selectable_label(s.autopilot.mode == AutopilotMode::Route, "Route")
                    .clicked()
            {
                self.cmd(Command::SetAutopilot {
                    mode: AutopilotMode::Route,
                });
            }
            if ui
                .selectable_label(s.vessel.anchored, "⚓")
                .on_hover_text("mouillage")
                .clicked()
            {
                self.cmd(Command::ToggleAnchor);
            }
            if ui
                .button("⚑")
                .on_hover_text("marquer la position (WPL)")
                .clicked()
            {
                self.cmd(Command::MarkPosition);
            }
            if s.route.destination().is_some() && ui.button("✖ route").clicked() {
                self.cmd(Command::ClearDestination);
            }
        });
    }
}

impl eframe::App for NmeaSimApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        self.drain_events();
        self.keyboard(ctx);
        egui::TopBottomPanel::top("top").show(ctx, |ui| self.top_bar(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let p = self.rt.published();
                ui.label(
                    RichText::new(format!(
                        "{:.1} phrases/s  ·  {} émises",
                        p.sentences_per_s, p.sentences_sent
                    ))
                    .weak(),
                );
                for t in &p.transports {
                    let c = panels::state_color(t.state);
                    ui.label(RichText::new(format!("● {}", t.id)).color(c))
                        .on_hover_text(format!("{} {} {:?}", t.kind, t.endpoint, t.state));
                }
                drop(p);
                ui.separator();
                ui.label(&self.status);
            });
        });
        egui::TopBottomPanel::bottom("tabs")
            .resizable(true)
            .default_height(290.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for t in Tab::ALL {
                        if ui.selectable_label(self.tab == t, t.label()).clicked() {
                            self.tab = t;
                            self.draft.ui.bottom_tab = t.id().into();
                        }
                    }
                });
                ui.separator();
                match self.tab {
                    Tab::Nmea => panels::nmea(self, ui),
                    Tab::Network => panels::network(self, ui),
                    Tab::Gamepad => panels::gamepad(self, ui),
                    Tab::Sources => panels::sources(self, ui),
                    Tab::Log => panels::log(self, ui),
                    Tab::Scenario => panels::scenario(self, ui),
                    Tab::Settings => panels::settings(self, ui),
                }
            });
        egui::TopBottomPanel::bottom("controls").show(ctx, |ui| self.controls(ui));
        egui::SidePanel::right("vessel")
            .default_width(260.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.vessel_panel(ui));
            });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let (snap, trail, track) = {
                let p = self.rt.published();
                (p.snapshot.clone(), p.trail.iter().copied().collect::<Vec<_>>(), p.track.as_ref().map(|t| t.points.clone()))
            };
            ui.horizontal(|ui| {
                if ui.small_button("⌖ recentrer").clicked() {
                    self.map.follow = true;
                }
                if ui.small_button("+").clicked() {
                    self.map.zoom = (self.map.zoom + 1.0).min(19.0);
                }
                if ui.small_button("−").clicked() {
                    self.map.zoom = (self.map.zoom - 1.0).max(2.0);
                }
                if ui.checkbox(&mut self.draft.ui.map.tiles, "fond OSM").on_hover_text("tuiles OpenStreetMap (réseau requis)").changed() && !self.draft.ui.map.tiles {
                    self.tiles = None;
                }
                ui.label(RichText::new("clic droit : destination · Maj+clic droit : point de route · Ctrl+clic : déplacer le navire · double-clic : suivre").weak().small());
            });
            if self.draft.ui.map.tiles && self.tiles.is_none() {
                self.tiles = Some(crate::tiles::TileCache::new(&self.draft.ui.map.tile_url));
            }
            if let Some(a) = self.map.show(ui, &snap, &trail, track.as_deref(), self.dark, self.tiles.as_mut()) {
                match a {
                    MapAction::Destination(p) => self.cmd_from(Command::SetDestination { position: p, name: None }, Source::Map),
                    MapAction::AddWaypoint(p) => self.cmd_from(Command::AddWaypoint { position: p, name: None }, Source::Map),
                    MapAction::MoveVessel(p) => self.cmd_from(Command::SetPosition { position: p }, Source::Map),
                }
            }
        });
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.draft.ui.map.zoom = self.map.zoom;
        self.draft.ui.map.follow = self.map.follow;
        let _ = config::save(&self.cfg_path, &self.draft);
        self.rt.shutdown();
    }
}

/// Lance l'interface.
pub fn run(
    cfg: AppConfig,
    warnings: Vec<String>,
    cfg_path: PathBuf,
    scenario: Option<PathBuf>,
) -> Result<(), String> {
    let app = NmeaSimApp::new(cfg, warnings, cfg_path, scenario)?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("NMEA Simulator {APP_VERSION}"))
            .with_inner_size([1360.0, 880.0])
            .with_min_inner_size([900.0, 600.0])
            .with_app_id("nmeasim-rs"),
        ..Default::default()
    };
    eframe::run_native(
        "nmeasim-rs",
        options,
        Box::new(|cc| {
            install_fallback_fonts(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string())
}

/// Police de repli pour les symboles absents des polices embarquées d'egui
/// (●, ✓, →, ⚓…) : DejaVu (paquet `fonts-dejavu-core`) si présente.
fn install_fallback_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
        "C:\\Windows\\Fonts\\seguisym.ttf",
    ];
    let Some(bytes) = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "fallback".into(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(fam)
            .or_default()
            .push("fallback".into());
    }
    ctx.set_fonts(fonts);
}
