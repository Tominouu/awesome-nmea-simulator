//! Onglets du bas : NMEA, NETWORK, GAMEPAD, SOURCES, LOG, SCENARIO, SETTINGS.

use std::collections::BTreeSet;

use egui::{Color32, RichText, Ui};
use nmeasim_app::config::{OutputFormat, TransportConfig};
use nmeasim_app::runtime::{Direction, complete_user_sentence};
use nmeasim_app::scenario::{self, Scenario};
use nmeasim_encode::nmea::{ALL_SENTENCES, LEGACY_ORDER, MODERN_ORDER, default_talker};
use nmeasim_encode::parse;
use nmeasim_input::gamepad::{
    Axis, AxisAction, AxisConfig, AxisMode, Button, ButtonAction, Calibration, Curve,
    Profile as PadProfile,
};
use nmeasim_sim::command::Command;
use nmeasim_sim::compat::Profile;
use nmeasim_source::replay::ReplayCommand;
use nmeasim_transport::{Parity, TransportSpec, TransportState, interfaces, serial_ports};

use crate::app::NmeaSimApp;

/// Couleur d'un état de transport.
pub fn state_color(s: TransportState) -> Color32 {
    match s {
        TransportState::Running => Color32::from_rgb(80, 220, 100),
        TransportState::Starting | TransportState::Reconnecting => Color32::from_rgb(240, 190, 60),
        TransportState::Failed => Color32::from_rgb(230, 70, 70),
        TransportState::Stopped => Color32::GRAY,
    }
}

// ─────────────────────────────── NMEA ────────────────────────────────

/// Moniteur NMEA.
pub fn nmea(app: &mut NmeaSimApp, ui: &mut Ui) {
    let (entries, rates) = {
        let p = app.rt.published();
        let e: Vec<_> = if app.monitor.paused {
            app.monitor.frozen.clone()
        } else {
            p.monitor.iter().cloned().collect()
        };
        (e, p.sentence_rates.clone())
    };
    ui.horizontal(|ui| {
        let label = if app.monitor.paused {
            "▶ Reprendre"
        } else {
            "⏸ Pause"
        };
        if ui.button(label).clicked() {
            app.monitor.paused = !app.monitor.paused;
            if app.monitor.paused {
                app.monitor.frozen = app.rt.published().monitor.iter().cloned().collect();
            }
        }
        ui.label("🔎");
        ui.add(
            egui::TextEdit::singleline(&mut app.monitor.search)
                .hint_text("recherche")
                .desired_width(140.0),
        );
        let seen: BTreeSet<String> = entries
            .iter()
            .filter_map(|e| parse::parse(&e.text))
            .map(|p| p.formatter)
            .collect();
        let talkers: BTreeSet<String> = entries
            .iter()
            .filter_map(|e| parse::parse(&e.text))
            .map(|p| p.talker)
            .collect();
        egui::ComboBox::from_id_salt("flt-sentence")
            .selected_text(if app.monitor.sentence.is_empty() {
                "toutes phrases".into()
            } else {
                app.monitor.sentence.clone()
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.monitor.sentence, String::new(), "toutes phrases");
                for s in &seen {
                    ui.selectable_value(&mut app.monitor.sentence, s.clone(), s);
                }
            });
        egui::ComboBox::from_id_salt("flt-talker")
            .selected_text(if app.monitor.talker.is_empty() {
                "tous talkers".into()
            } else {
                app.monitor.talker.clone()
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.monitor.talker, String::new(), "tous talkers");
                for t in &talkers {
                    ui.selectable_value(&mut app.monitor.talker, t.clone(), t);
                }
            });
        ui.checkbox(&mut app.monitor.show_input, "entrées");
        ui.checkbox(&mut app.monitor.show_signalk, "Signal K");
        if ui.button("🗑").on_hover_text("vider").clicked() {
            app.rt.clear_monitor();
        }
    });
    let filtered: Vec<_> = entries
        .iter()
        .filter(|e| app.monitor.show_input || e.direction == Direction::Out)
        .filter(|e| app.monitor.show_signalk || e.channel != "signalk")
        .filter(|e| {
            app.monitor.search.is_empty()
                || e.text
                    .to_lowercase()
                    .contains(&app.monitor.search.to_lowercase())
        })
        .filter(|e| {
            if app.monitor.sentence.is_empty() && app.monitor.talker.is_empty() {
                return true;
            }
            parse::parse(&e.text).is_some_and(|p| {
                (app.monitor.sentence.is_empty() || p.formatter == app.monitor.sentence)
                    && (app.monitor.talker.is_empty() || p.talker == app.monitor.talker)
            })
        })
        .collect();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{} lignes affichées / {}",
                filtered.len(),
                entries.len()
            ))
            .weak(),
        );
        if ui
            .button("📋 Copier")
            .on_hover_text("copie les lignes filtrées")
            .clicked()
        {
            let txt: String = filtered.iter().map(|e| format!("{}\r\n", e.text)).collect();
            ui.ctx().copy_text(txt);
        }
        ui.separator();
        let top: Vec<String> = rates
            .iter()
            .filter(|(_, r)| **r > 0.0)
            .map(|(k, r)| format!("{k} {r:.1}/s"))
            .collect();
        ui.label(RichText::new(top.join("  ")).small().weak());
    });
    ui.horizontal(|ui| {
        ui.label("Terminal :");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut app.monitor.terminal)
                .hint_text("$GPTXT,01,01,02,TEXTE (checksum ajouté si absent)")
                .desired_width(420.0),
        );
        let send = ui.button("Envoyer").clicked()
            || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        if send {
            match complete_user_sentence(&app.monitor.terminal) {
                Some(s) => {
                    app.rt.send_user_sentence(&s);
                    app.status = format!("envoyée au prochain tick : {s}");
                    app.monitor.terminal.clear();
                }
                None => app.status = "phrase invalide : doit commencer par $ ou !".into(),
            }
        }
    });
    let row_h = 16.0;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(!app.monitor.paused)
        .show_rows(ui, row_h, filtered.len(), |ui, range| {
            for e in &filtered[range] {
                ui.horizontal(|ui| {
                    let t = chrono::DateTime::from_timestamp_millis(e.time_ms)
                        .map(|d| {
                            d.with_timezone(&chrono::Local)
                                .format("%H:%M:%S%.3f")
                                .to_string()
                        })
                        .unwrap_or_default();
                    ui.label(RichText::new(t).monospace().weak().small());
                    let (arrow, col) = match e.direction {
                        Direction::Out => ("→", Color32::from_rgb(120, 200, 255)),
                        Direction::In => ("←", Color32::from_rgb(255, 180, 90)),
                    };
                    ui.label(RichText::new(arrow).color(col));
                    match parse::parse(&e.text) {
                        Some(p) => {
                            let (mark, c) = if p.checksum_ok {
                                ("✓", Color32::from_rgb(80, 200, 100))
                            } else {
                                ("✗", Color32::RED)
                            };
                            ui.label(RichText::new(mark).color(c).small());
                            ui.label(RichText::new(&e.text).monospace());
                        }
                        None => {
                            ui.label(RichText::new(" ").small());
                            ui.label(RichText::new(truncate(&e.text, 400)).monospace().weak());
                        }
                    }
                });
            }
        });
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() > n {
        format!(
            "{}…",
            &s[..s.char_indices().nth(n).map_or(s.len(), |(i, _)| i)]
        )
    } else {
        s.to_string()
    }
}

// ────────────────────────────── NETWORK ──────────────────────────────

fn spec_kind(s: &TransportSpec) -> &'static str {
    s.type_name()
}

fn default_spec(kind: &str) -> TransportSpec {
    match kind {
        "tcp-server" => TransportSpec::TcpServer {
            bind: "0.0.0.0".into(),
            port: 10110,
        },
        "tcp-client" => TransportSpec::TcpClient {
            host: "127.0.0.1".into(),
            port: 10110,
        },
        "serial" => TransportSpec::Serial {
            port: serial_ports()
                .first()
                .cloned()
                .unwrap_or_else(|| "/dev/ttyUSB0".into()),
            baud_rate: 4800,
            data_bits: 8,
            stop_bits: 1,
            parity: Parity::None,
            flow_control: Default::default(),
        },
        "udp-broadcast" => TransportSpec::UdpBroadcast {
            interface: String::new(),
            port: 10110,
        },
        "udp-multicast" => TransportSpec::UdpMulticast {
            group: "239.192.0.1".into(),
            port: 60001,
            interface: String::new(),
            ttl: 128,
            loopback: true,
        },
        "websocket-server" => TransportSpec::WebsocketServer {
            bind: "0.0.0.0".into(),
            port: 3000,
        },
        _ => TransportSpec::UdpClient {
            host: "127.0.0.1".into(),
            port: 10110,
        },
    }
}

/// Vue réseau : état des transports et édition.
pub fn network(app: &mut NmeaSimApp, ui: &mut Ui) {
    let statuses = app.rt.published().transports.clone();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            ui.heading("État");
            if statuses.is_empty() {
                ui.label("Aucun transport actif (les transports s'ouvrent au démarrage de la simulation).");
            }
            for t in &statuses {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{} ({})", t.id, t.kind)).strong());
                        ui.label(RichText::new(format!("● {:?}", t.state)).color(state_color(t.state)));
                    });
                    ui.label(RichText::new(&t.endpoint).monospace());
                    ui.label(format!("clients : {}   trames : {}   octets : {}   erreurs : {}", t.clients, t.messages, t.bytes, t.errors));
                    if let Some(e) = &t.last_error {
                        ui.label(RichText::new(format!("dernière erreur : {e}")).color(Color32::from_rgb(240, 150, 90)).small());
                    }
                });
            }
            let vs = &app.draft.viewsync;
            ui.label(format!("ViewSync : {} {}:{}", if vs.enabled { "● actif" } else { "○ inactif" }, vs.host, vs.port));
            match &app.api {
                Some(a) => ui.label(format!("API HTTP : ● http://{}/api/v1", a.addr)),
                None => ui.label("API HTTP : ○ inactive"),
            };
            let ifs: Vec<String> = interfaces().iter().map(|(n, ip, b)| format!("{n} {ip}{}", b.map(|b| format!(" (diffusion {b})")).unwrap_or_default())).collect();
            ui.collapsing("Interfaces réseau", |ui| {
                for i in ifs {
                    ui.label(RichText::new(i).monospace().small());
                }
            });
            let ui = &mut cols[1];
            ui.heading("Configuration");
            let mut remove = None;
            let n = app.draft.transports.len();
            for i in 0..n {
                let t = &mut app.draft.transports[i];
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut t.enabled, "");
                        ui.add(egui::TextEdit::singleline(&mut t.id).desired_width(90.0));
                        let mut kind = spec_kind(&t.spec).to_string();
                        egui::ComboBox::from_id_salt(("kind", i)).selected_text(kind.clone()).show_ui(ui, |ui| {
                            for k in nmeasim_app::legacy::LEGACY_TYPES {
                                ui.selectable_value(&mut kind, k.to_string(), k);
                            }
                        });
                        if kind != spec_kind(&t.spec) {
                            t.spec = default_spec(&kind);
                        }
                        egui::ComboBox::from_id_salt(("fmt", i))
                            .selected_text(if t.format == OutputFormat::Nmea { "NMEA" } else { "Signal K" })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut t.format, OutputFormat::Nmea, "NMEA");
                                ui.selectable_value(&mut t.format, OutputFormat::Signalk, "Signal K");
                            });
                        if ui.small_button("🗑").clicked() {
                            remove = Some(i);
                        }
                    });
                    spec_editor(ui, &mut t.spec, i);
                });
            }
            if let Some(i) = remove {
                app.draft.transports.remove(i);
            }
            ui.horizontal(|ui| {
                if ui.button("＋ Ajouter").clicked() {
                    let k = app.draft.transports.len() + 1;
                    app.draft.transports.push(TransportConfig { id: format!("out-{k}"), enabled: true, format: OutputFormat::Nmea, spec: default_spec("udp-client") });
                }
                if ui.button("✔ Appliquer").clicked() {
                    app.apply_draft();
                }
            });
            ui.separator();
            ui.label(RichText::new("ViewSync").strong());
            let vs = &mut app.draft.viewsync;
            ui.horizontal(|ui| {
                ui.checkbox(&mut vs.enabled, "actif");
                ui.add(egui::TextEdit::singleline(&mut vs.host).desired_width(110.0));
                ui.add(egui::DragValue::new(&mut vs.port).range(1..=65535));
                ui.label("alt.");
                ui.add(egui::DragValue::new(&mut vs.altitude).range(0.0..=10000.0));
                ui.label("tilt");
                ui.add(egui::DragValue::new(&mut vs.tilt).range(0.0..=90.0));
            });
            ui.label(RichText::new("Signal K").strong());
            ui.horizontal(|ui| {
                ui.label("vesselId");
                ui.add(egui::TextEdit::singleline(&mut app.draft.output.signalk.vessel_id).desired_width(320.0));
            });
        });
    });
}

fn spec_editor(ui: &mut Ui, spec: &mut TransportSpec, i: usize) {
    ui.horizontal(|ui| match spec {
        TransportSpec::TcpServer { bind, port } | TransportSpec::WebsocketServer { bind, port } => {
            ui.label("écoute");
            ui.add(egui::TextEdit::singleline(bind).desired_width(110.0));
            ui.add(egui::DragValue::new(port).range(1..=65535));
        }
        TransportSpec::TcpClient { host, port } | TransportSpec::UdpClient { host, port } => {
            ui.label("hôte");
            ui.add(egui::TextEdit::singleline(host).desired_width(130.0));
            ui.add(egui::DragValue::new(port).range(1..=65535));
        }
        TransportSpec::UdpBroadcast { interface, port } => {
            egui::ComboBox::from_id_salt(("if", i))
                .selected_text(if interface.is_empty() {
                    "255.255.255.255".to_string()
                } else {
                    interface.clone()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(interface, String::new(), "255.255.255.255");
                    for (n, _, _) in interfaces() {
                        ui.selectable_value(interface, n.clone(), n);
                    }
                });
            ui.add(egui::DragValue::new(port).range(1..=65535));
        }
        TransportSpec::UdpMulticast {
            group,
            port,
            interface,
            ttl,
            loopback,
        } => {
            ui.label("groupe");
            ui.add(egui::TextEdit::singleline(group).desired_width(110.0));
            ui.add(egui::DragValue::new(port).range(1..=65535));
            ui.label("if");
            ui.add(
                egui::TextEdit::singleline(interface)
                    .hint_text("défaut")
                    .desired_width(80.0),
            );
            ui.label("TTL");
            ui.add(egui::DragValue::new(ttl).range(1..=255));
            ui.checkbox(loopback, "loop");
        }
        TransportSpec::Serial {
            port,
            baud_rate,
            data_bits,
            stop_bits,
            parity,
            ..
        } => {
            ui.add(egui::TextEdit::singleline(port).desired_width(120.0));
            egui::ComboBox::from_id_salt(("sp", i))
                .selected_text("ports")
                .show_ui(ui, |ui| {
                    for p in serial_ports() {
                        if ui.selectable_label(false, &p).clicked() {
                            *port = p;
                        }
                    }
                });
            egui::ComboBox::from_id_salt(("baud", i))
                .selected_text(baud_rate.to_string())
                .show_ui(ui, |ui| {
                    for b in [4800, 9600, 19200, 38400, 57600, 115200] {
                        ui.selectable_value(baud_rate, b, b.to_string());
                    }
                });
            ui.add(egui::DragValue::new(data_bits).range(5..=8))
                .on_hover_text("bits de données");
            ui.add(egui::DragValue::new(stop_bits).range(1..=2))
                .on_hover_text("bits d'arrêt");
            egui::ComboBox::from_id_salt(("par", i))
                .selected_text(format!("{parity:?}"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(parity, Parity::None, "None");
                    ui.selectable_value(parity, Parity::Even, "Even");
                    ui.selectable_value(parity, Parity::Odd, "Odd");
                });
        }
    });
}

// ────────────────────────────── GAMEPAD ──────────────────────────────

const AXIS_ACTIONS: [(AxisAction, &str); 6] = [
    (AxisAction::None, "—"),
    (AxisAction::Rudder, "barre"),
    (AxisAction::Throttle, "propulsion"),
    (AxisAction::ThrottleForward, "avant (gâchette)"),
    (AxisAction::ThrottleReverse, "arrière (gâchette)"),
    (AxisAction::Heading, "cap pilote (taux)"),
];

fn button_actions() -> Vec<(ButtonAction, &'static str)> {
    vec![
        (ButtonAction::None, "—"),
        (ButtonAction::ToggleEngines, "moteurs marche/arrêt"),
        (ButtonAction::ToggleAutopilot, "pilote automatique"),
        (ButtonAction::Reset, "reset"),
        (ButtonAction::ToggleAnchor, "mouillage"),
        (ButtonAction::TogglePause, "pause"),
        (ButtonAction::NextWaypoint, "point suivant"),
        (ButtonAction::AddWaypointAtVessel, "waypoint ici"),
        (ButtonAction::MarkPosition, "marquer position"),
        (ButtonAction::CenterRudder, "barre au centre"),
        (ButtonAction::RudderNudge { deg: -1.0 }, "barre −1°"),
        (ButtonAction::RudderNudge { deg: 1.0 }, "barre +1°"),
        (
            ButtonAction::ThrottleNudge { delta: 0.1 },
            "propulsion +10 %",
        ),
        (
            ButtonAction::ThrottleNudge { delta: -0.1 },
            "propulsion −10 %",
        ),
        (ButtonAction::HeadingNudge { deg: -10.0 }, "cap −10°"),
        (ButtonAction::HeadingNudge { deg: 10.0 }, "cap +10°"),
    ]
}

fn label_of(a: ButtonAction) -> String {
    button_actions()
        .into_iter()
        .find(|(x, _)| *x == a)
        .map_or_else(|| format!("{a:?}"), |(_, l)| l.to_string())
}

fn value_bar(ui: &mut Ui, v: f64, trigger: bool) {
    let frac = if trigger {
        v.clamp(0.0, 1.0)
    } else {
        (v.clamp(-1.0, 1.0) + 1.0) / 2.0
    };
    ui.add(
        egui::ProgressBar::new(frac as f32)
            .desired_width(120.0)
            .text(format!("{v:+.2}")),
    );
}

/// Écran manette : périphériques, valeurs brutes et normalisées, test,
/// calibration, mapping.
pub fn gamepad(app: &mut NmeaSimApp, ui: &mut Ui) {
    let Some((view, mut muted)) = app.gamepad.as_ref().map(|d| (d.view(), d.muted())) else {
        ui.label("Prise en charge de la manette désactivée (Réglages → manette).");
        return;
    };
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.columns(2, |cols| {
                let ui = &mut cols[0];
                ui.heading("Controller");
                if !view.available {
                    ui.label(
                        RichText::new(format!(
                            "pilote indisponible : {}",
                            view.error.clone().unwrap_or_default()
                        ))
                        .color(Color32::from_rgb(240, 150, 90)),
                    );
                }
                if view.devices.is_empty() {
                    ui.label(
                        "Aucune manette détectée. Branchez-en une : elle est reconnue à chaud.",
                    );
                }
                if ui
                    .checkbox(&mut muted, "Mode test (aucune commande envoyée)")
                    .changed()
                {
                    if let Some(d) = app.gamepad.as_ref() {
                        d.set_muted(muted);
                    }
                }
                for d in &view.devices {
                    ui.separator();
                    ui.label(RichText::new(format!("Device : {} (#{})", d.name, d.id)).strong());
                    egui::Grid::new(("axes", d.id))
                        .num_columns(3)
                        .show(ui, |ui| {
                            ui.label("axe");
                            ui.label("brut");
                            ui.label("normalisé");
                            ui.end_row();
                            for a in Axis::ALL {
                                ui.label(a.label());
                                value_bar(
                                    ui,
                                    d.raw.get(&a).copied().unwrap_or(0.0),
                                    a.is_trigger(),
                                );
                                value_bar(
                                    ui,
                                    d.normalized.get(&a).copied().unwrap_or(0.0),
                                    a.is_trigger(),
                                );
                                ui.end_row();
                            }
                        });
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Buttons :");
                        for b in Button::ALL {
                            let on = d.pressed.get(&b).copied().unwrap_or(false);
                            ui.label(
                                RichText::new(format!(
                                    "{} {}",
                                    b.label(),
                                    if on { "●" } else { "○" }
                                ))
                                .color(if on {
                                    Color32::from_rgb(80, 220, 100)
                                } else {
                                    Color32::GRAY
                                }),
                            );
                        }
                    });
                    // Calibration de l'axe choisi.
                    ui.horizontal(|ui| {
                        ui.label("Calibration :");
                        let current = app.calib.map(|c| c.0);
                        egui::ComboBox::from_id_salt(("cal", d.id))
                            .selected_text(current.map_or("axe…".into(), |a| a.label().to_string()))
                            .show_ui(ui, |ui| {
                                for a in Axis::ALL {
                                    if ui.selectable_label(current == Some(a), a.label()).clicked()
                                    {
                                        app.calib = Some((a, f64::MAX, f64::MIN));
                                    }
                                }
                            });
                        if let Some((a, lo, hi)) = app.calib.as_mut() {
                            let raw = d.raw.get(a).copied().unwrap_or(0.0);
                            *lo = lo.min(raw);
                            *hi = hi.max(raw);
                            ui.label(format!(
                                "min {:.2} max {:.2}",
                                if *lo == f64::MAX { 0.0 } else { *lo },
                                if *hi == f64::MIN { 0.0 } else { *hi }
                            ));
                            if ui.button("Centre = position actuelle").clicked() {
                                let e = app.pad_profile.axes.entry(*a).or_default();
                                e.calibration.center = raw;
                            }
                            if ui.button("Valider l'amplitude").clicked() && *lo < *hi {
                                let e = app.pad_profile.axes.entry(*a).or_default();
                                e.calibration.min = *lo;
                                e.calibration.max = *hi;
                                app.status =
                                    format!("calibration {} : [{lo:.2}, {hi:.2}]", a.label());
                            }
                        }
                    });
                }
                ui.separator();
                ui.label(RichText::new("Dernières commandes :").weak());
                ui.label(
                    RichText::new(view.last_commands.join("  "))
                        .monospace()
                        .small(),
                );
                let ui = &mut cols[1];
                ui.heading("Mapping");
                let p = &mut app.pad_profile;
                ui.horizontal(|ui| {
                    ui.label("profil");
                    ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(120.0));
                    ui.label("déconnexion");
                    egui::ComboBox::from_id_salt("ondisc")
                        .selected_text(format!("{:?}", p.on_disconnect))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut p.on_disconnect,
                                nmeasim_input::gamepad::OnDisconnect::Hold,
                                "Hold",
                            );
                            ui.selectable_value(
                                &mut p.on_disconnect,
                                nmeasim_input::gamepad::OnDisconnect::Center,
                                "Center",
                            );
                        });
                });
                egui::Grid::new("axis-map")
                    .striped(true)
                    .num_columns(8)
                    .show(ui, |ui| {
                        for h in [
                            "axe",
                            "action",
                            "mode",
                            "inv.",
                            "zone morte",
                            "courbe",
                            "sensib.",
                            "échelle",
                        ] {
                            ui.label(RichText::new(h).small().strong());
                        }
                        ui.end_row();
                        for a in Axis::ALL {
                            let c = p.axes.entry(a).or_insert_with(|| AxisConfig {
                                calibration: if a.is_trigger() {
                                    Calibration::TRIGGER
                                } else {
                                    Calibration::STICK
                                },
                                ..AxisConfig::default()
                            });
                            ui.label(a.label());
                            egui::ComboBox::from_id_salt(("aa", a))
                                .selected_text(
                                    AXIS_ACTIONS
                                        .iter()
                                        .find(|(x, _)| *x == c.action)
                                        .map_or("?", |x| x.1),
                                )
                                .show_ui(ui, |ui| {
                                    for (x, l) in AXIS_ACTIONS {
                                        ui.selectable_value(&mut c.action, x, l);
                                    }
                                });
                            egui::ComboBox::from_id_salt(("am", a))
                                .selected_text(if c.mode == AxisMode::Position {
                                    "position"
                                } else {
                                    "taux"
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut c.mode,
                                        AxisMode::Position,
                                        "position",
                                    );
                                    ui.selectable_value(&mut c.mode, AxisMode::Rate, "taux");
                                });
                            ui.checkbox(&mut c.invert, "");
                            ui.add(
                                egui::Slider::new(&mut c.deadzone, 0.0..=0.45).fixed_decimals(2),
                            );
                            let mut kind = match c.curve {
                                Curve::Linear => 0,
                                Curve::Expo { .. } => 1,
                                Curve::Power { .. } => 2,
                            };
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt(("ac", a))
                                    .width(70.0)
                                    .selected_text(["linéaire", "expo", "puissance"][kind])
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut kind, 0, "linéaire");
                                        ui.selectable_value(&mut kind, 1, "expo");
                                        ui.selectable_value(&mut kind, 2, "puissance");
                                    });
                                c.curve = match (kind, c.curve.clone()) {
                                    (0, _) => Curve::Linear,
                                    (1, Curve::Expo { k }) => {
                                        let mut k = k;
                                        ui.add(
                                            egui::DragValue::new(&mut k)
                                                .range(0.0..=1.0)
                                                .speed(0.01),
                                        );
                                        Curve::Expo { k }
                                    }
                                    (1, _) => Curve::Expo { k: 0.3 },
                                    (_, Curve::Power { p }) => {
                                        let mut p = p;
                                        ui.add(
                                            egui::DragValue::new(&mut p)
                                                .range(0.2..=4.0)
                                                .speed(0.05),
                                        );
                                        Curve::Power { p }
                                    }
                                    _ => Curve::Power { p: 2.0 },
                                };
                            });
                            ui.add(
                                egui::Slider::new(&mut c.sensitivity, 0.1..=2.0).fixed_decimals(2),
                            );
                            ui.add(
                                egui::DragValue::new(&mut c.scale)
                                    .range(0.0..=90.0)
                                    .speed(0.5),
                            );
                            ui.end_row();
                        }
                    });
                egui::Grid::new("btn-map")
                    .striped(true)
                    .num_columns(3)
                    .show(ui, |ui| {
                        for b in Button::ALL {
                            let c = p.buttons.entry(b).or_default();
                            ui.label(b.label());
                            egui::ComboBox::from_id_salt(("ba", b))
                                .selected_text(label_of(c.action))
                                .show_ui(ui, |ui| {
                                    for (x, l) in button_actions() {
                                        ui.selectable_value(&mut c.action, x, l);
                                    }
                                });
                            ui.add(
                                egui::DragValue::new(&mut c.repeat_ms)
                                    .range(0..=2000)
                                    .suffix(" ms"),
                            )
                            .on_hover_text("répétition (0 = aucune)");
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("✔ Appliquer et enregistrer").clicked() {
                        let prof = app.pad_profile.clone();
                        match prof.validate() {
                            Ok(()) => {
                                let d = &mut app.draft.gamepad;
                                d.profiles.retain(|x| x.name != prof.name);
                                d.active = prof.name.clone();
                                d.profiles.push(prof);
                                app.apply_draft();
                            }
                            Err(e) => app.status = e,
                        }
                    }
                    if ui.button("Défaut").clicked() {
                        app.pad_profile = PadProfile::default();
                    }
                    if ui.button("Exporter…").clicked() {
                        if let Some(f) = rfd::FileDialog::new()
                            .add_filter("Profil", &["json"])
                            .set_file_name(format!("{}.json", app.pad_profile.name))
                            .save_file()
                        {
                            let _ = std::fs::write(
                                f,
                                serde_json::to_string_pretty(&app.pad_profile).unwrap_or_default(),
                            );
                        }
                    }
                    if ui.button("Importer…").clicked() {
                        if let Some(f) = rfd::FileDialog::new()
                            .add_filter("Profil", &["json"])
                            .pick_file()
                        {
                            match std::fs::read_to_string(&f)
                                .map_err(|e| e.to_string())
                                .and_then(|t| {
                                    serde_json::from_str::<PadProfile>(&t)
                                        .map_err(|e| e.to_string())
                                }) {
                                Ok(p) => app.pad_profile = p,
                                Err(e) => app.status = format!("profil invalide : {e}"),
                            }
                        }
                    }
                });
            });
        });
}

// ────────────────────────────── SOURCES ──────────────────────────────

/// Traces GPX/KML et rejeu de journal.
pub fn sources(app: &mut NmeaSimApp, ui: &mut Ui) {
    let (track, replay) = {
        let p = app.rt.published();
        (p.track.clone(), p.replay.clone())
    };
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        ui.heading("Suivi de trace (GPX / KML)");
        ui.horizontal(|ui| {
            if ui.button("📂 Charger une trace…").clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("Traces", &["gpx", "kml"]).pick_file() {
                    match app.rt.load_track(&f, 0) {
                        Ok(n) => app.note(format!("trace chargée : {n}")),
                        Err(e) => app.status = format!("trace : {e}"),
                    }
                }
            }
            if ui.button("⎈ Charger comme route…").on_hover_text("les points deviennent la route du pilote automatique").clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("Traces", &["gpx", "kml"]).pick_file() {
                    match app.rt.load_route(&f, 0) {
                        Ok(n) => app.note(format!("route chargée : {n} points")),
                        Err(e) => app.status = format!("route : {e}"),
                    }
                }
            }
        });
        ui.checkbox(&mut app.draft.track.kml_legacy, "KML : limites legacy (gx:Track seul, un niveau de Folder)");
        ui.checkbox(&mut app.draft.track.repeat, "boucler en fin de trace");
        match &track {
            Some(t) => {
                ui.label(format!("{} — point {} / {}{}", t.name, t.index, t.total, if t.ended { " (fin)" } else { "" }));
                ui.add(egui::ProgressBar::new(t.index as f32 / t.total.max(1) as f32));
                ui.horizontal(|ui| {
                    if ui.selectable_label(t.following, "▶ suivre").clicked() {
                        app.rt.follow_track(!t.following);
                    }
                    if ui.button("⏮ début").clicked() {
                        app.rt.rewind_track();
                    }
                    if ui.button("✖ décharger").clicked() {
                        app.rt.clear_track();
                    }
                });
                ui.label(RichText::new("La trace pilote le navire quand la simulation tourne (un point par tick).").weak().small());
            }
            None => {
                ui.label("Aucune trace chargée.");
            }
        }
        let ui = &mut cols[1];
        ui.heading("Rejeu de journal (.nmeasim)");
        ui.horizontal(|ui| {
            if ui.button("📂 Ouvrir un journal…").clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("Journal", &["nmeasim"]).pick_file() {
                    match app.rt.load_journal(&f) {
                        Ok(n) => app.note(format!("journal : {n} blocs")),
                        Err(e) => app.status = format!("journal : {e}"),
                    }
                }
            }
            if replay.is_some() && ui.button("✖ fermer").clicked() {
                app.rt.close_replay();
            }
        });
        if let Some(r) = replay {
            ui.label(format!("Sentence group : {} of {}{}", r.next.min(r.total), r.total, if r.ended { " (fin)" } else { "" }));
            ui.add(egui::ProgressBar::new(r.next as f32 / r.total.max(1) as f32));
            ui.horizontal(|ui| {
                if ui.button("⏮").clicked() {
                    app.rt.replay(ReplayCommand::Seek { index: 0 });
                }
                if ui.button("◀").clicked() {
                    app.rt.replay(ReplayCommand::StepBack);
                }
                if r.playing {
                    if ui.button("⏸").clicked() {
                        app.rt.replay(ReplayCommand::Pause);
                    }
                } else if ui.button("▶").clicked() {
                    app.rt.replay(ReplayCommand::Play);
                }
                if ui.button("▶|").clicked() {
                    app.rt.replay(ReplayCommand::StepForward);
                }
                let mut speed = r.speed;
                if ui.add(egui::Slider::new(&mut speed, 0.1..=20.0).logarithmic(true).text("×")).changed() {
                    app.rt.replay(ReplayCommand::Speed { factor: speed });
                }
                let mut rep = r.repeat;
                if ui.checkbox(&mut rep, "boucle").changed() {
                    app.rt.replay(ReplayCommand::Repeat { on: rep });
                }
            });
            ui.label(RichText::new("Pendant le rejeu, le journal est le seul producteur : la simulation live est suspendue.").weak().small());
        }
    });
}

// ──────────────────────────────── LOG ────────────────────────────────

/// Événements et enregistrement.
pub fn log(app: &mut NmeaSimApp, ui: &mut Ui) {
    let rec = app.rt.published().recording.clone();
    ui.horizontal(|ui| {
        match &rec {
            Some(r) => {
                ui.label(
                    RichText::new(format!(
                        "⏺ {} — {} blocs, {} abandonnés",
                        r.path, r.written, r.dropped
                    ))
                    .color(Color32::RED),
                );
                if let Some(e) = &r.error {
                    ui.label(RichText::new(e).color(Color32::RED));
                }
                if ui.button("⏹ Arrêter l'enregistrement").clicked() {
                    app.rt.stop_recording();
                }
            }
            None => {
                if ui.button("⏺ Enregistrer un journal…").clicked() {
                    if let Some(f) = rfd::FileDialog::new()
                        .add_filter("Journal", &["nmeasim"])
                        .set_file_name("session.nmeasim")
                        .save_file()
                    {
                        match app.rt.start_recording(&f) {
                            Ok(p) => app.status = format!("enregistrement : {p}"),
                            Err(e) => app.status = format!("enregistrement impossible : {e}"),
                        }
                    }
                }
            }
        }
        ui.checkbox(&mut app.draft.logging.meta, "ligne meta: (D7)");
        if ui.button("🗑 vider").clicked() {
            app.log.clear();
        }
    });
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for (t, l) in &app.log {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t).monospace().weak());
                    ui.label(l);
                });
            }
        });
}

// ────────────────────────────── SCENARIO ─────────────────────────────

/// Gestion des scénarios.
pub fn scenario(app: &mut NmeaSimApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("Nom");
        ui.add(egui::TextEdit::singleline(&mut app.scenario_name).desired_width(220.0));
        ui.label(
            RichText::new(
                app.scenario_path
                    .as_ref()
                    .map_or("(non enregistré)".into(), |p| p.display().to_string()),
            )
            .weak(),
        );
    });
    ui.add(
        egui::TextEdit::multiline(&mut app.scenario_desc)
            .hint_text("description")
            .desired_rows(2)
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        if ui.button("🆕 Nouveau").clicked() {
            let mut c = app.draft.clone();
            c.simulation = nmeasim_sim::SimConfig::default();
            app.draft = c;
            app.apply_draft();
            app.scenario_path = None;
            app.scenario_name = "Nouveau scénario".into();
            app.scenario_desc.clear();
            app.cmd(Command::ClearDestination);
        }
        if ui.button("📂 Ouvrir…").clicked() {
            if let Some(f) = rfd::FileDialog::new()
                .add_filter("Scénario", &["json"])
                .pick_file()
            {
                match scenario::load(&f) {
                    Ok(s) => {
                        app.draft = s.apply_to(&app.draft);
                        app.draft.last_scenario = Some(f.display().to_string());
                        app.apply_draft();
                        for c in s.commands() {
                            app.cmd(c);
                        }
                        app.scenario_name = s.name.clone();
                        app.scenario_desc = s.description.clone();
                        app.scenario_path = Some(f);
                        app.note(format!("scénario ouvert : {}", s.name));
                    }
                    Err(e) => app.status = format!("scénario : {e}"),
                }
            }
        }
        let save = |app: &mut NmeaSimApp, path: std::path::PathBuf| {
            let cfg = app.rt.published().config.clone();
            let snap = app.rt.snapshot();
            let mut s = Scenario::capture(&app.scenario_name, &cfg, &snap);
            s.description = app.scenario_desc.clone();
            match scenario::save(&path, &s) {
                Ok(()) => {
                    app.note(format!("scénario enregistré : {}", path.display()));
                    app.scenario_path = Some(path);
                }
                Err(e) => app.status = format!("scénario : {e}"),
            }
        };
        if ui.button("💾 Enregistrer").clicked() {
            match app.scenario_path.clone() {
                Some(p) => save(app, p),
                None => {
                    if let Some(f) = rfd::FileDialog::new()
                        .add_filter("Scénario", &["json"])
                        .set_file_name("scenario.json")
                        .save_file()
                    {
                        save(app, f);
                    }
                }
            }
        }
        if ui.button("💾 Enregistrer sous…").clicked() {
            if let Some(f) = rfd::FileDialog::new()
                .add_filter("Scénario", &["json"])
                .set_file_name("scenario.json")
                .save_file()
            {
                save(app, f);
            }
        }
        ui.separator();
        if ui.button("▶ Lancer").clicked() {
            app.cmd(Command::Start);
        }
        if ui.button("⏸ Pause").clicked() {
            app.cmd(Command::TogglePause);
        }
        if ui.button("⏹ Arrêter").clicked() {
            app.cmd(Command::Stop);
        }
    });
    ui.label(RichText::new("Un scénario enregistre la position, le cap, la vitesse, le navire, l'environnement, la graine, la route et les paramètres réseau courants.").weak().small());
}

// ────────────────────────────── SETTINGS ─────────────────────────────

/// Réglages.
pub fn settings(app: &mut NmeaSimApp, ui: &mut Ui) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.button("✔ Appliquer et enregistrer").clicked() {
                app.apply_draft();
            }
            if ui.button("Importer une config NMEASimulator 1.6.1…").clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() {
                    match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| nmeasim_app::config::from_json(&t).map_err(|e| e.to_string())) {
                        Ok(l) => {
                            for w in &l.warnings {
                                app.note(w.clone());
                            }
                            app.draft = l.config;
                            app.apply_draft();
                        }
                        Err(e) => app.status = format!("import : {e}"),
                    }
                }
            }
            ui.label(RichText::new(app.cfg_path.display().to_string()).weak().small());
        });
        ui.columns(3, |cols| {
            let d = &mut app.draft;
            let ui = &mut cols[0];
            ui.heading("Simulation");
            egui::Grid::new("sim").num_columns(2).show(ui, |ui| {
                ui.label("profil");
                egui::ComboBox::from_id_salt("prof").selected_text(format!("{:?}", d.simulation.compatibility.profile).to_lowercase()).show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.simulation.compatibility.profile, Profile::Modern, "modern");
                    ui.selectable_value(&mut d.simulation.compatibility.profile, Profile::Legacy, "legacy");
                });
                ui.end_row();
                ui.label("graine");
                let mut fixed = d.simulation.seed.is_some();
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut fixed, "fixe").changed() {
                        d.simulation.seed = fixed.then_some(1234);
                    }
                    if let Some(s) = d.simulation.seed.as_mut() {
                        ui.add(egui::DragValue::new(s));
                    }
                });
                ui.end_row();
                ui.label("intervalle (ms)");
                ui.add(egui::DragValue::new(&mut d.output.interval_ms).range(1..=10000).speed(1));
                ui.end_row();
                ui.label("pas (ms)");
                ui.add(egui::DragValue::new(&mut d.simulation.step_ms).range(1..=1000));
                ui.end_row();
                ui.label("démarrage auto");
                ui.checkbox(&mut d.auto_start, "");
                ui.end_row();
                let v = &mut d.simulation.vessel;
                ui.label("longueur (m)");
                ui.add(egui::DragValue::new(&mut v.length_m).range(1.0..=400.0));
                ui.end_row();
                ui.label("vmax avant / arrière (kn)");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut v.max_speed_ahead_kn).range(0.5..=60.0));
                    ui.add(egui::DragValue::new(&mut v.max_speed_astern_kn).range(0.0..=30.0));
                });
                ui.end_row();
                ui.label("accél. / décél. (s)");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut v.accel_time_s).range(0.5..=600.0));
                    ui.add(egui::DragValue::new(&mut v.decel_time_s).range(0.5..=600.0));
                });
                ui.end_row();
                ui.label("barre max (°) / vitesse (°/s)");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut v.max_rudder_deg).range(1.0..=90.0));
                    ui.add(egui::DragValue::new(&mut v.rudder_rate_deg_s).range(0.5..=90.0));
                });
                ui.end_row();
                ui.label("régime ralenti / max");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut v.idle_rpm).range(0.0..=5000.0));
                    ui.add(egui::DragValue::new(&mut v.max_rpm).range(100.0..=20000.0));
                });
                ui.end_row();
            });
            let ui = &mut cols[1];
            ui.heading("Départ et environnement");
            let i = &mut d.simulation.initial;
            let e = &mut d.simulation.environment;
            egui::Grid::new("env").num_columns(2).show(ui, |ui| {
                ui.label("latitude / longitude");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut i.position.lat).range(-89.0..=89.0).speed(0.001).max_decimals(6));
                    ui.add(egui::DragValue::new(&mut i.position.lon).range(-180.0..=180.0).speed(0.001).max_decimals(6));
                });
                ui.end_row();
                ui.label("cap (°) / vitesse (kn)");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut i.heading_deg).range(0.0..=359.9));
                    ui.add(egui::DragValue::new(&mut i.speed_kn).range(0.0..=70.0).speed(0.1));
                });
                ui.end_row();
                ui.label("altitude antenne (m)");
                ui.add(egui::DragValue::new(&mut i.altitude_m).range(-100.0..=10000.0));
                ui.end_row();
                ui.label("vent (° / kn)");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut e.wind_direction_deg).range(0.0..=359.9));
                    ui.add(egui::DragValue::new(&mut e.wind_speed_kn).range(0.0..=100.0).speed(0.1));
                });
                ui.end_row();
                ui.label("courant (° / kn)");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut e.current_set_deg).range(0.0..=359.9));
                    ui.add(egui::DragValue::new(&mut e.current_drift_kn).range(0.0..=10.0).speed(0.05));
                });
                ui.end_row();
                ui.label("profondeur (m) / eau (°C)");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut e.depth_m).range(0.2..=11000.0).speed(0.1));
                    ui.add(egui::DragValue::new(&mut e.water_temperature_c).range(-2.0..=40.0).speed(0.1));
                });
                ui.end_row();
                ui.label("déclinaison (°)");
                ui.add(egui::DragValue::new(&mut e.magnetic_variation_deg).range(-180.0..=180.0).speed(0.1));
                ui.end_row();
            });
            ui.label(RichText::new("Les valeurs de départ s'appliquent au prochain « Appliquer » (la simulation repart de ces valeurs).").weak().small());
            let ui = &mut cols[2];
            ui.heading("Sortie NMEA");
            let n = &mut d.output.nmea;
            ui.checkbox(&mut n.prefix, "préfixe 61162-450");
            let mut custom = n.sentences.is_some();
            if ui.checkbox(&mut custom, "choisir les phrases").changed() {
                n.sentences = custom.then(|| {
                    let base = if d.simulation.compatibility.profile == Profile::Legacy { LEGACY_ORDER } else { MODERN_ORDER };
                    base.iter().map(|s| s.to_string()).collect()
                });
            }
            if let Some(list) = n.sentences.as_mut() {
                ui.horizontal_wrapped(|ui| {
                    for s in ALL_SENTENCES {
                        let mut on = list.iter().any(|x| x == s);
                        if ui.checkbox(&mut on, *s).changed() {
                            if on {
                                list.push(s.to_string());
                                list.sort_by_key(|x| ALL_SENTENCES.iter().position(|y| y == x));
                            } else {
                                list.retain(|x| x != s);
                            }
                        }
                    }
                });
            }
            ui.collapsing("Talkers", |ui| {
                egui::Grid::new("talk").num_columns(4).show(ui, |ui| {
                    for (k, s) in ALL_SENTENCES.iter().enumerate() {
                        let cur = n.talkers.get(*s).cloned().unwrap_or_else(|| default_talker(s).to_string());
                        let mut t = cur.clone();
                        ui.label(*s);
                        if ui.add(egui::TextEdit::singleline(&mut t).desired_width(28.0)).changed() {
                            let t = t.to_uppercase().chars().take(2).collect::<String>();
                            if t.len() == 2 && t != default_talker(s) {
                                n.talkers.insert(s.to_string(), t);
                            } else {
                                n.talkers.remove(*s);
                            }
                        }
                        if k % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            ui.collapsing("Identité AIS", |ui| {
                let a = &mut n.ais;
                egui::Grid::new("ais").num_columns(2).show(ui, |ui| {
                    ui.label("MMSI");
                    ui.add(egui::DragValue::new(&mut a.mmsi).range(100_000_000..=999_999_999));
                    ui.end_row();
                    ui.label("indicatif");
                    ui.text_edit_singleline(&mut a.callsign);
                    ui.end_row();
                    ui.label("nom");
                    ui.text_edit_singleline(&mut a.name);
                    ui.end_row();
                    ui.label("destination");
                    ui.text_edit_singleline(&mut a.destination);
                    ui.end_row();
                    ui.label("type");
                    ui.add(egui::DragValue::new(&mut a.ship_type));
                    ui.end_row();
                });
            });
            ui.collapsing("API distante (HTTP)", |ui| {
                let api = &mut d.api;
                ui.checkbox(&mut api.enabled, "active (au prochain lancement)");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut api.bind).desired_width(100.0));
                    ui.add(egui::DragValue::new(&mut api.port).range(1..=65535));
                });
                let mut tok = api.token.clone().unwrap_or_default();
                if ui.add(egui::TextEdit::singleline(&mut tok).hint_text("jeton (facultatif)").password(true)).changed() {
                    api.token = (!tok.is_empty()).then_some(tok);
                }
            });
            ui.checkbox(&mut d.gamepad.enabled, "manette (au prochain lancement)");
        });
    });
}
