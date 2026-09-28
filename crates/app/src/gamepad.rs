//! Pilote de manette : gilrs dans un thread dédié, traduit en [`RawEvent`],
//! puis en commandes par l'[`InputMapper`] pur. Les valeurs brutes et
//! normalisées sont publiées pour l'écran « Gamepad ».

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nmeasim_input::gamepad::{Axis, Button, InputMapper, Profile, RawEvent};

use crate::runtime::Controller;

/// Vue publiée d'une manette.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceView {
    /// Identifiant de session.
    pub id: u32,
    /// Nom.
    pub name: String,
    /// Valeurs brutes par axe.
    pub raw: BTreeMap<Axis, f64>,
    /// Valeurs normalisées par axe.
    pub normalized: BTreeMap<Axis, f64>,
    /// Boutons enfoncés.
    pub pressed: BTreeMap<Button, bool>,
}

/// Vue publiée du sous-système manette.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GamepadView {
    /// Pilote disponible.
    pub available: bool,
    /// Erreur d'initialisation.
    pub error: Option<String>,
    /// Manettes connectées.
    pub devices: Vec<DeviceView>,
    /// Dernières commandes émises (pour l'affichage).
    pub last_commands: Vec<String>,
}

/// Sous-système manette en tâche de fond.
pub struct GamepadDriver {
    view: Arc<RwLock<GamepadView>>,
    profile_tx: Sender<Profile>,
    muted: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl GamepadDriver {
    /// Démarre le pilote. Sans matériel ou sans pilote, la vue l'indique et
    /// l'application reste pleinement utilisable.
    pub fn spawn(ctl: Controller, profile: Profile) -> Self {
        let view = Arc::new(RwLock::new(GamepadView::default()));
        let (profile_tx, profile_rx) = channel::<Profile>();
        let muted = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (v, m, s) = (view.clone(), muted.clone(), stop.clone());
        let worker = thread::Builder::new()
            .name("nmeasim-gamepad".into())
            .spawn(move || run(ctl, profile, profile_rx, v, m, s))
            .ok();
        Self {
            view,
            profile_tx,
            muted,
            stop,
            worker,
        }
    }

    /// Vue courante.
    pub fn view(&self) -> GamepadView {
        self.view
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Remplace le profil actif.
    pub fn set_profile(&self, p: Profile) {
        let _ = self.profile_tx.send(p);
    }

    /// Mode test : les entrées sont affichées mais aucune commande n'est émise.
    pub fn set_muted(&self, m: bool) {
        self.muted.store(m, Ordering::Relaxed);
    }

    /// Mode test actif ?
    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }
}

impl Drop for GamepadDriver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

fn publish(mapper: &InputMapper, view: &RwLock<GamepadView>, last: &[String]) {
    let devices = mapper
        .devices()
        .into_iter()
        .map(|(id, name)| DeviceView {
            id,
            name,
            raw: Axis::ALL.iter().map(|a| (*a, mapper.raw(id, *a))).collect(),
            normalized: Axis::ALL
                .iter()
                .map(|a| (*a, mapper.normalized(id, *a)))
                .collect(),
            pressed: Button::ALL
                .iter()
                .map(|b| (*b, mapper.pressed(id, *b)))
                .collect(),
        })
        .collect();
    let mut v = view
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    v.devices = devices;
    v.last_commands = last.to_vec();
}

#[cfg(feature = "gamepad")]
fn convert(ev: gilrs::Event, name: Option<String>) -> Vec<RawEvent> {
    use gilrs::{Axis as GA, Button as GB, EventType};
    let device = usize::from(ev.id) as u32;
    let axis = |a: GA| match a {
        GA::LeftStickX => Some(Axis::LeftX),
        GA::LeftStickY => Some(Axis::LeftY),
        GA::RightStickX => Some(Axis::RightX),
        GA::RightStickY => Some(Axis::RightY),
        GA::LeftZ => Some(Axis::LeftTrigger),
        GA::RightZ => Some(Axis::RightTrigger),
        _ => None,
    };
    let button = |b: GB| match b {
        GB::South => Some(Button::South),
        GB::East => Some(Button::East),
        GB::West => Some(Button::West),
        GB::North => Some(Button::North),
        GB::LeftTrigger => Some(Button::LeftBumper),
        GB::RightTrigger => Some(Button::RightBumper),
        GB::Select => Some(Button::Select),
        GB::Start => Some(Button::Start),
        GB::LeftThumb => Some(Button::LeftThumb),
        GB::RightThumb => Some(Button::RightThumb),
        GB::DPadUp => Some(Button::DPadUp),
        GB::DPadDown => Some(Button::DPadDown),
        GB::DPadLeft => Some(Button::DPadLeft),
        GB::DPadRight => Some(Button::DPadRight),
        _ => None,
    };
    match ev.event {
        EventType::Connected => vec![RawEvent::Connected {
            device,
            name: name.unwrap_or_else(|| "Gamepad".into()),
        }],
        EventType::Disconnected => vec![RawEvent::Disconnected { device }],
        EventType::AxisChanged(a, value, _) => axis(a)
            .map(|axis| {
                // Les gâchettes en axe Z peuvent aller de -1 à 1 : ramenées sur [0, 1].
                let value = if axis.is_trigger() && value < 0.0 {
                    (f64::from(value) + 1.0) / 2.0
                } else {
                    f64::from(value)
                };
                vec![RawEvent::Axis {
                    device,
                    axis,
                    value,
                }]
            })
            .unwrap_or_default(),
        EventType::ButtonChanged(GB::LeftTrigger2, v, _) => vec![RawEvent::Axis {
            device,
            axis: Axis::LeftTrigger,
            value: f64::from(v),
        }],
        EventType::ButtonChanged(GB::RightTrigger2, v, _) => vec![RawEvent::Axis {
            device,
            axis: Axis::RightTrigger,
            value: f64::from(v),
        }],
        EventType::ButtonPressed(b, _) => button(b)
            .map(|button| {
                vec![RawEvent::Button {
                    device,
                    button,
                    pressed: true,
                }]
            })
            .unwrap_or_default(),
        EventType::ButtonReleased(b, _) => button(b)
            .map(|button| {
                vec![RawEvent::Button {
                    device,
                    button,
                    pressed: false,
                }]
            })
            .unwrap_or_default(),
        _ => vec![],
    }
}

fn run(
    ctl: Controller,
    profile: Profile,
    profile_rx: std::sync::mpsc::Receiver<Profile>,
    view: Arc<RwLock<GamepadView>>,
    muted: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    let mut mapper = InputMapper::new(profile);
    let mut last: Vec<String> = vec![];
    let dispatch = |cmds: Vec<(nmeasim_sim::Command, nmeasim_sim::Source)>,
                    last: &mut Vec<String>| {
        for (c, s) in cmds {
            last.push(c.kind());
            if last.len() > 8 {
                last.remove(0);
            }
            if !muted.load(Ordering::Relaxed) {
                ctl.submit_async(c, s);
            }
        }
    };
    #[cfg(feature = "gamepad")]
    let mut gilrs = match gilrs::Gilrs::new() {
        Ok(g) => {
            view.write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .available = true;
            // Manettes déjà branchées.
            let present: Vec<(u32, String)> = g
                .gamepads()
                .map(|(id, gp)| (usize::from(id) as u32, gp.name().to_string()))
                .collect();
            for (device, name) in present {
                let cmds = mapper.process(&RawEvent::Connected { device, name });
                dispatch(cmds, &mut last);
            }
            Some(g)
        }
        Err(e) => {
            let mut v = view
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            v.available = false;
            v.error = Some(e.to_string());
            None
        }
    };
    #[cfg(not(feature = "gamepad"))]
    {
        view.write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .error = Some("compilé sans prise en charge de manette".into());
    }
    let mut last_tick = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        while let Ok(p) = profile_rx.try_recv() {
            mapper.set_profile(p);
        }
        #[cfg(feature = "gamepad")]
        if let Some(g) = gilrs.as_mut() {
            while let Some(ev) = g.next_event() {
                let name = g.connected_gamepad(ev.id).map(|gp| gp.name().to_string());
                // gilrs n'émet pas toujours `ButtonChanged` à l'appui d'une
                // gâchette : la valeur analogique est relue dans l'état.
                let trigger = match ev.event {
                    gilrs::EventType::ButtonPressed(
                        b @ (gilrs::Button::LeftTrigger2 | gilrs::Button::RightTrigger2),
                        _,
                    )
                    | gilrs::EventType::ButtonReleased(
                        b @ (gilrs::Button::LeftTrigger2 | gilrs::Button::RightTrigger2),
                        _,
                    ) => {
                        let pressed = matches!(ev.event, gilrs::EventType::ButtonPressed(..));
                        let v = g
                            .connected_gamepad(ev.id)
                            .and_then(|gp| gp.button_data(b).map(|d| f64::from(d.value())))
                            .unwrap_or(if pressed { 1.0 } else { 0.0 });
                        let axis = if b == gilrs::Button::LeftTrigger2 {
                            Axis::LeftTrigger
                        } else {
                            Axis::RightTrigger
                        };
                        Some(RawEvent::Axis {
                            device: usize::from(ev.id) as u32,
                            axis,
                            value: if pressed { v.max(0.5) } else { v.min(0.5) },
                        })
                    }
                    _ => None,
                };
                for raw in convert(ev, name).into_iter().chain(trigger) {
                    let cmds = mapper.process(&raw);
                    dispatch(cmds, &mut last);
                }
            }
        }
        let dt = last_tick.elapsed();
        if dt >= Duration::from_millis(20) {
            last_tick = Instant::now();
            let cmds = mapper.tick(dt.as_millis() as u32);
            dispatch(cmds, &mut last);
            publish(&mapper, &view, &last);
        }
        thread::sleep(Duration::from_millis(4));
    }
}
