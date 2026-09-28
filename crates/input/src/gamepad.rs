//! Manette (`docs/09` §3) : profil, traitement des axes, traduction en
//! commandes. Aucune dépendance matérielle : le pilote (gilrs) vit dans
//! l'application et fournit des [`RawEvent`].

use std::collections::BTreeMap;

use nmeasim_sim::command::{Command, EngineSel, Source};
use serde::{Deserialize, Serialize};

/// Axe normalisé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    /// Stick gauche, horizontal.
    LeftX,
    /// Stick gauche, vertical (haut positif).
    LeftY,
    /// Stick droit, horizontal.
    RightX,
    /// Stick droit, vertical.
    RightY,
    /// Gâchette gauche, `[0, 1]`.
    LeftTrigger,
    /// Gâchette droite, `[0, 1]`.
    RightTrigger,
}

impl Axis {
    /// Tous les axes.
    pub const ALL: [Axis; 6] = [
        Axis::LeftX,
        Axis::LeftY,
        Axis::RightX,
        Axis::RightY,
        Axis::LeftTrigger,
        Axis::RightTrigger,
    ];

    /// Gâchette (plage `[0, 1]`).
    pub fn is_trigger(self) -> bool {
        matches!(self, Axis::LeftTrigger | Axis::RightTrigger)
    }

    /// Libellé court.
    pub fn label(self) -> &'static str {
        match self {
            Axis::LeftX => "Left X",
            Axis::LeftY => "Left Y",
            Axis::RightX => "Right X",
            Axis::RightY => "Right Y",
            Axis::LeftTrigger => "LT",
            Axis::RightTrigger => "RT",
        }
    }
}

/// Bouton (disposition Xbox ; A = sud).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Button {
    /// A (sud).
    South,
    /// B (est).
    East,
    /// X (ouest).
    West,
    /// Y (nord).
    North,
    /// Bumper gauche.
    LeftBumper,
    /// Bumper droit.
    RightBumper,
    /// Select / Back.
    Select,
    /// Start / Menu.
    Start,
    /// Clic stick gauche.
    LeftThumb,
    /// Clic stick droit.
    RightThumb,
    /// Croix haut.
    DPadUp,
    /// Croix bas.
    DPadDown,
    /// Croix gauche.
    DPadLeft,
    /// Croix droite.
    DPadRight,
}

impl Button {
    /// Tous les boutons.
    pub const ALL: [Button; 14] = [
        Button::South,
        Button::East,
        Button::West,
        Button::North,
        Button::LeftBumper,
        Button::RightBumper,
        Button::Select,
        Button::Start,
        Button::LeftThumb,
        Button::RightThumb,
        Button::DPadUp,
        Button::DPadDown,
        Button::DPadLeft,
        Button::DPadRight,
    ];

    /// Libellé court.
    pub fn label(self) -> &'static str {
        match self {
            Button::South => "A",
            Button::East => "B",
            Button::West => "X",
            Button::North => "Y",
            Button::LeftBumper => "LB",
            Button::RightBumper => "RB",
            Button::Select => "Back",
            Button::Start => "Start",
            Button::LeftThumb => "LS",
            Button::RightThumb => "RS",
            Button::DPadUp => "↑",
            Button::DPadDown => "↓",
            Button::DPadLeft => "←",
            Button::DPadRight => "→",
        }
    }
}

/// Événement brut d'un pilote de manette.
#[derive(Debug, Clone, PartialEq)]
pub enum RawEvent {
    /// Manette branchée.
    Connected {
        /// Identifiant de session.
        device: u32,
        /// Nom du périphérique.
        name: String,
    },
    /// Manette débranchée.
    Disconnected {
        /// Identifiant.
        device: u32,
    },
    /// Valeur brute d'un axe (`[-1, 1]`, gâchettes `[0, 1]`).
    Axis {
        /// Identifiant.
        device: u32,
        /// Axe.
        axis: Axis,
        /// Valeur brute.
        value: f64,
    },
    /// Bouton.
    Button {
        /// Identifiant.
        device: u32,
        /// Bouton.
        button: Button,
        /// Enfoncé.
        pressed: bool,
    },
}

/// Courbe de réponse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Curve {
    /// Linéaire.
    Linear,
    /// `(1 − k)·x + k·x³`.
    Expo {
        /// Facteur `[0, 1]`.
        k: f64,
    },
    /// `sign(x)·|x|^p`.
    Power {
        /// Exposant `> 0`.
        p: f64,
    },
}

impl Curve {
    /// Applique la courbe à `x ∈ [-1, 1]`.
    pub fn apply(&self, x: f64) -> f64 {
        match self {
            Curve::Linear => x,
            Curve::Expo { k } => (1.0 - k) * x + k * x * x * x,
            Curve::Power { p } => x.signum() * x.abs().powf(*p),
        }
    }
}

/// Calibration mesurée.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Calibration {
    /// Minimum brut.
    pub min: f64,
    /// Centre brut au repos.
    pub center: f64,
    /// Maximum brut.
    pub max: f64,
}

impl Calibration {
    /// Identité pour un stick.
    pub const STICK: Calibration = Calibration {
        min: -1.0,
        center: 0.0,
        max: 1.0,
    };
    /// Identité pour une gâchette.
    pub const TRIGGER: Calibration = Calibration {
        min: 0.0,
        center: 0.0,
        max: 1.0,
    };

    /// Ramène une valeur brute sur `[-1, 1]` (`[0, 1]` pour une gâchette).
    pub fn apply(&self, raw: f64) -> f64 {
        let v = if raw >= self.center {
            let span = self.max - self.center;
            if span.abs() < 1e-9 {
                0.0
            } else {
                (raw - self.center) / span
            }
        } else {
            let span = self.center - self.min;
            if span.abs() < 1e-9 {
                0.0
            } else {
                (raw - self.center) / span
            }
        };
        v.clamp(-1.0, 1.0)
    }
}

/// Mode d'un axe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AxisMode {
    /// La position de l'axe est la consigne.
    Position,
    /// La position de l'axe est une vitesse de variation de la consigne.
    Rate,
}

/// Action d'un axe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AxisAction {
    /// Aucune.
    None,
    /// Barre (consigne en degrés = valeur × échelle).
    Rudder,
    /// Propulsion `[-1, 1]` des deux moteurs.
    Throttle,
    /// Propulsion avant (gâchette).
    ThrottleForward,
    /// Propulsion arrière (gâchette).
    ThrottleReverse,
    /// Cap du pilote (mode taux, °/s = valeur × échelle).
    Heading,
}

/// Réglage d'un axe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AxisConfig {
    /// Action.
    pub action: AxisAction,
    /// Mode.
    pub mode: AxisMode,
    /// Calibration.
    pub calibration: Calibration,
    /// Inversion.
    pub invert: bool,
    /// Zone morte `[0, 0.5)`.
    pub deadzone: f64,
    /// Courbe.
    pub curve: Curve,
    /// Sensibilité `(0, 2]`.
    pub sensitivity: f64,
    /// Échelle : degrés de barre à fond, ou unité/s en mode taux.
    pub scale: f64,
}

impl Default for AxisConfig {
    fn default() -> Self {
        Self {
            action: AxisAction::None,
            mode: AxisMode::Position,
            calibration: Calibration::STICK,
            invert: false,
            deadzone: 0.08,
            curve: Curve::Linear,
            sensitivity: 1.0,
            scale: 1.0,
        }
    }
}

impl AxisConfig {
    /// Chaîne complète : calibration → inversion → zone morte (continue) →
    /// courbe → sensibilité (`docs/09` §3.3).
    pub fn normalize(&self, raw: f64) -> f64 {
        let mut x = self.calibration.apply(raw);
        if self.invert {
            x = -x;
        }
        let d = self.deadzone.clamp(0.0, 0.49);
        if x.abs() < d {
            return 0.0;
        }
        x = x.signum() * (x.abs() - d) / (1.0 - d);
        x = self.curve.apply(x);
        (x * self.sensitivity.clamp(0.01, 2.0)).clamp(-1.0, 1.0)
    }
}

/// Action d'un bouton.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ButtonAction {
    /// Aucune.
    None,
    /// Marche / arrêt des moteurs.
    ToggleEngines,
    /// Pilote automatique (tenue du cap courant).
    ToggleAutopilot,
    /// Remise à l'état initial.
    Reset,
    /// Mouillage.
    ToggleAnchor,
    /// Pause / reprise.
    TogglePause,
    /// Démarrage / arrêt de la simulation.
    StartStop,
    /// Point de route suivant.
    NextWaypoint,
    /// Waypoint à la position du navire.
    AddWaypointAtVessel,
    /// Marque la position (`WPL`).
    MarkPosition,
    /// Barre au centre.
    CenterRudder,
    /// Barre ± degrés.
    RudderNudge {
        /// Degrés.
        deg: f64,
    },
    /// Propulsion ± delta.
    ThrottleNudge {
        /// Delta.
        delta: f64,
    },
    /// Cap du pilote ± degrés.
    HeadingNudge {
        /// Degrés.
        deg: f64,
    },
}

/// Réglage d'un bouton.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ButtonConfig {
    /// Action.
    pub action: ButtonAction,
    /// Répétition tant que le bouton est enfoncé, ms (0 = aucune).
    pub repeat_ms: u32,
}

impl Default for ButtonConfig {
    fn default() -> Self {
        Self {
            action: ButtonAction::None,
            repeat_ms: 0,
        }
    }
}

/// Comportement à la déconnexion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OnDisconnect {
    /// Consignes conservées.
    #[default]
    Hold,
    /// Barre au centre et propulsion au point mort.
    Center,
}

/// Profil de manette, exportable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    /// Version du schéma.
    pub schema: u32,
    /// Nom.
    pub name: String,
    /// Empreinte de périphérique reconnue (`*` = toutes).
    pub fingerprint: String,
    /// Déconnexion.
    pub on_disconnect: OnDisconnect,
    /// Axes.
    pub axes: BTreeMap<Axis, AxisConfig>,
    /// Boutons.
    pub buttons: BTreeMap<Button, ButtonConfig>,
    /// Seuil de variation avant réémission, fraction de la course.
    pub epsilon: f64,
}

impl Default for Profile {
    /// Mapping par défaut demandé : stick gauche Y → propulsion, stick droit
    /// X → barre, LT → arrière, RT → avant, A → moteurs, B → pilote, X →
    /// reset, Y → mouillage.
    fn default() -> Self {
        let stick = |action, scale| AxisConfig {
            action,
            scale,
            ..AxisConfig::default()
        };
        let trigger = |action| AxisConfig {
            action,
            calibration: Calibration::TRIGGER,
            deadzone: 0.05,
            ..AxisConfig::default()
        };
        let mut axes = BTreeMap::new();
        axes.insert(Axis::LeftY, stick(AxisAction::Throttle, 1.0));
        axes.insert(
            Axis::RightX,
            AxisConfig {
                curve: Curve::Expo { k: 0.3 },
                ..stick(AxisAction::Rudder, 40.0)
            },
        );
        axes.insert(Axis::LeftX, stick(AxisAction::None, 1.0));
        axes.insert(Axis::RightY, stick(AxisAction::None, 1.0));
        axes.insert(Axis::LeftTrigger, trigger(AxisAction::ThrottleReverse));
        axes.insert(Axis::RightTrigger, trigger(AxisAction::ThrottleForward));
        let b = |action| ButtonConfig {
            action,
            repeat_ms: 0,
        };
        let rep = |action| ButtonConfig {
            action,
            repeat_ms: 150,
        };
        let mut buttons = BTreeMap::new();
        buttons.insert(Button::South, b(ButtonAction::ToggleEngines));
        buttons.insert(Button::East, b(ButtonAction::ToggleAutopilot));
        buttons.insert(Button::West, b(ButtonAction::Reset));
        buttons.insert(Button::North, b(ButtonAction::ToggleAnchor));
        buttons.insert(Button::Start, b(ButtonAction::TogglePause));
        buttons.insert(Button::Select, b(ButtonAction::CenterRudder));
        buttons.insert(
            Button::LeftBumper,
            rep(ButtonAction::HeadingNudge { deg: -10.0 }),
        );
        buttons.insert(
            Button::RightBumper,
            rep(ButtonAction::HeadingNudge { deg: 10.0 }),
        );
        buttons.insert(
            Button::DPadLeft,
            rep(ButtonAction::RudderNudge { deg: -1.0 }),
        );
        buttons.insert(
            Button::DPadRight,
            rep(ButtonAction::RudderNudge { deg: 1.0 }),
        );
        buttons.insert(
            Button::DPadUp,
            rep(ButtonAction::ThrottleNudge { delta: 0.1 }),
        );
        buttons.insert(
            Button::DPadDown,
            rep(ButtonAction::ThrottleNudge { delta: -0.1 }),
        );
        buttons.insert(Button::LeftThumb, b(ButtonAction::MarkPosition));
        buttons.insert(Button::RightThumb, b(ButtonAction::NextWaypoint));
        Self {
            schema: 1,
            name: "default".into(),
            fingerprint: "*".into(),
            on_disconnect: OnDisconnect::Hold,
            axes,
            buttons,
            epsilon: 0.005,
        }
    }
}

impl Profile {
    /// Validation explicite.
    pub fn validate(&self) -> Result<(), String> {
        for (a, c) in &self.axes {
            if !(0.0..0.5).contains(&c.deadzone) {
                return Err(format!(
                    "gamepad.axes.{a:?}.deadzone : doit être dans [0, 0.5)"
                ));
            }
            if !(c.sensitivity > 0.0 && c.sensitivity <= 2.0) {
                return Err(format!(
                    "gamepad.axes.{a:?}.sensitivity : doit être dans (0, 2]"
                ));
            }
            if let Curve::Expo { k } = c.curve {
                if !(0.0..=1.0).contains(&k) {
                    return Err(format!(
                        "gamepad.axes.{a:?}.curve.k : doit être dans [0, 1]"
                    ));
                }
            }
            if let Curve::Power { p } = c.curve {
                if p <= 0.0 {
                    return Err(format!("gamepad.axes.{a:?}.curve.p : doit être > 0"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
struct DeviceState {
    name: String,
    raw: BTreeMap<Axis, f64>,
    last_sent: BTreeMap<AxisAction, f64>,
    triggers: (f64, f64),
    held: BTreeMap<Button, u32>,
    pressed: BTreeMap<Button, bool>,
    rate_acc: f64,
}

/// Traducteur pur événements → commandes.
#[derive(Debug, Clone, Default)]
pub struct InputMapper {
    profile: Profile,
    devices: BTreeMap<u32, DeviceState>,
}

impl InputMapper {
    /// Nouveau traducteur.
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            devices: BTreeMap::new(),
        }
    }

    /// Profil courant.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Remplace le profil (les états d'axes sont conservés).
    pub fn set_profile(&mut self, p: Profile) {
        self.profile = p;
    }

    /// Manettes connues : (id, nom).
    pub fn devices(&self) -> Vec<(u32, String)> {
        self.devices
            .iter()
            .map(|(k, d)| (*k, d.name.clone()))
            .collect()
    }

    /// Valeur brute d'un axe.
    pub fn raw(&self, device: u32, axis: Axis) -> f64 {
        self.devices
            .get(&device)
            .and_then(|d| d.raw.get(&axis))
            .copied()
            .unwrap_or(0.0)
    }

    /// Valeur normalisée d'un axe.
    pub fn normalized(&self, device: u32, axis: Axis) -> f64 {
        let cfg = self.profile.axes.get(&axis).cloned().unwrap_or_default();
        cfg.normalize(self.raw(device, axis))
    }

    /// Bouton enfoncé ?
    pub fn pressed(&self, device: u32, button: Button) -> bool {
        self.devices
            .get(&device)
            .and_then(|d| d.pressed.get(&button))
            .copied()
            .unwrap_or(false)
    }

    fn emit_axis(
        dev: &mut DeviceState,
        action: AxisAction,
        value: f64,
        eps: f64,
        out: &mut Vec<Command>,
    ) {
        let last = dev.last_sent.get(&action).copied().unwrap_or(0.0);
        let changed = (value - last).abs() > eps || (value == 0.0 && last != 0.0);
        if !changed {
            return;
        }
        dev.last_sent.insert(action, value);
        match action {
            AxisAction::Rudder => out.push(Command::SetRudder { deg: value }),
            AxisAction::Throttle | AxisAction::ThrottleForward | AxisAction::ThrottleReverse => out
                .push(Command::SetThrottle {
                    engine: EngineSel::All,
                    value: value.clamp(-1.0, 1.0),
                }),
            AxisAction::Heading | AxisAction::None => {}
        }
    }

    fn button_command(action: ButtonAction) -> Option<Command> {
        Some(match action {
            ButtonAction::None => return None,
            ButtonAction::ToggleEngines => Command::ToggleEngines,
            ButtonAction::ToggleAutopilot => Command::ToggleAutopilot,
            ButtonAction::Reset => Command::Reset,
            ButtonAction::ToggleAnchor => Command::ToggleAnchor,
            ButtonAction::TogglePause => Command::TogglePause,
            ButtonAction::StartStop => return Some(Command::Start),
            ButtonAction::NextWaypoint => Command::NextWaypoint,
            ButtonAction::AddWaypointAtVessel => Command::AddWaypointAtVessel,
            ButtonAction::MarkPosition => Command::MarkPosition,
            ButtonAction::CenterRudder => Command::CenterRudder,
            ButtonAction::RudderNudge { deg } => Command::NudgeRudder { deg },
            ButtonAction::ThrottleNudge { delta } => Command::NudgeThrottle {
                engine: EngineSel::All,
                delta,
            },
            ButtonAction::HeadingNudge { deg } => Command::NudgeHeading { deg },
        })
    }

    /// Traite un événement brut ; rend les commandes à soumettre avec leur source.
    pub fn process(&mut self, ev: &RawEvent) -> Vec<(Command, Source)> {
        let mut out = Vec::new();
        let dev_id = match ev {
            RawEvent::Connected { device, .. }
            | RawEvent::Disconnected { device }
            | RawEvent::Axis { device, .. }
            | RawEvent::Button { device, .. } => *device,
        };
        match ev {
            RawEvent::Connected { device, name } => {
                self.devices.insert(
                    *device,
                    DeviceState {
                        name: name.clone(),
                        ..Default::default()
                    },
                );
            }
            RawEvent::Disconnected { device } => {
                if self.devices.remove(device).is_some()
                    && self.profile.on_disconnect == OnDisconnect::Center
                {
                    out.push(Command::CenterRudder);
                    out.push(Command::SetThrottle {
                        engine: EngineSel::All,
                        value: 0.0,
                    });
                }
            }
            RawEvent::Axis {
                device,
                axis,
                value,
            } => {
                let eps = self.profile.epsilon;
                let cfg = self.profile.axes.get(axis).cloned().unwrap_or_default();
                let dev = self.devices.entry(*device).or_default();
                dev.raw.insert(*axis, *value);
                let v = cfg.normalize(*value);
                match (cfg.action, cfg.mode) {
                    (AxisAction::ThrottleForward | AxisAction::ThrottleReverse, _) => {
                        let mag = v.abs();
                        if cfg.action == AxisAction::ThrottleForward {
                            dev.triggers.1 = mag;
                        } else {
                            dev.triggers.0 = mag;
                        }
                        let combined = dev.triggers.1 - dev.triggers.0;
                        Self::emit_axis(dev, AxisAction::ThrottleForward, combined, eps, &mut out);
                    }
                    (AxisAction::Rudder, AxisMode::Position) => {
                        Self::emit_axis(
                            dev,
                            AxisAction::Rudder,
                            v * cfg.scale,
                            eps * cfg.scale.abs().max(1.0),
                            &mut out,
                        );
                    }
                    (AxisAction::Throttle, AxisMode::Position) => {
                        Self::emit_axis(dev, AxisAction::Throttle, v, eps, &mut out)
                    }
                    _ => {}
                }
            }
            RawEvent::Button {
                device,
                button,
                pressed,
            } => {
                let cfg = self
                    .profile
                    .buttons
                    .get(button)
                    .cloned()
                    .unwrap_or_default();
                let dev = self.devices.entry(*device).or_default();
                let was = dev.pressed.insert(*button, *pressed).unwrap_or(false);
                if *pressed && !was {
                    dev.held.insert(*button, 0);
                    if let Some(c) = Self::button_command(cfg.action) {
                        out.push(c);
                    }
                } else if !*pressed {
                    dev.held.remove(button);
                }
            }
        }
        out.into_iter()
            .map(|c| (c, Source::Gamepad(dev_id)))
            .collect()
    }

    /// Avance le temps : répétitions de boutons et axes en mode taux.
    pub fn tick(&mut self, dt_ms: u32) -> Vec<(Command, Source)> {
        let mut out = Vec::new();
        let profile = &self.profile;
        for (id, dev) in &mut self.devices {
            for (button, t) in &mut dev.held {
                let cfg = profile.buttons.get(button).cloned().unwrap_or_default();
                if cfg.repeat_ms == 0 {
                    continue;
                }
                *t += dt_ms;
                // Premier délai : 2 × la période, puis une par période.
                while *t >= 2 * cfg.repeat_ms {
                    *t -= cfg.repeat_ms;
                    if let Some(c) = Self::button_command(cfg.action) {
                        out.push((c, Source::Gamepad(*id)));
                    }
                }
            }
            for (axis, cfg) in &profile.axes {
                if cfg.mode != AxisMode::Rate {
                    continue;
                }
                let v = cfg.normalize(dev.raw.get(axis).copied().unwrap_or(0.0));
                if v == 0.0 {
                    continue;
                }
                dev.rate_acc += v * cfg.scale * f64::from(dt_ms) / 1000.0;
                if dev.rate_acc.abs() >= 1.0 {
                    let step = dev.rate_acc.trunc();
                    dev.rate_acc -= step;
                    let c = match cfg.action {
                        AxisAction::Rudder => Command::NudgeRudder { deg: step },
                        AxisAction::Heading => Command::NudgeHeading { deg: step },
                        AxisAction::Throttle => Command::NudgeThrottle {
                            engine: EngineSel::All,
                            delta: step / 100.0,
                        },
                        _ => continue,
                    };
                    out.push((c, Source::Gamepad(*id)));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis(device: u32, axis: Axis, value: f64) -> RawEvent {
        RawEvent::Axis {
            device,
            axis,
            value,
        }
    }

    #[test]
    fn deadzone_continuity_invert_curve_calibration() {
        let c = AxisConfig {
            deadzone: 0.1,
            ..AxisConfig::default()
        };
        assert_eq!(c.normalize(0.05), 0.0);
        assert!(c.normalize(0.100_001) < 1e-4, "pas de saut à la frontière");
        assert!((c.normalize(1.0) - 1.0).abs() < 1e-12);
        let inv = AxisConfig {
            invert: true,
            ..c.clone()
        };
        assert_eq!(inv.normalize(0.7), -c.normalize(0.7));
        let e = Curve::Expo { k: 0.3 };
        assert!((e.apply(0.5) - 0.3875).abs() < 1e-12);
        assert_eq!(e.apply(1.0), 1.0);
        assert!((Curve::Power { p: 2.0 }.apply(-0.5) + 0.25).abs() < 1e-12);
        let cal = Calibration {
            min: -0.8,
            center: 0.05,
            max: 0.9,
        };
        assert!((cal.apply(0.9) - 1.0).abs() < 1e-12);
        assert!((cal.apply(-0.8) + 1.0).abs() < 1e-12);
        assert_eq!(cal.apply(0.05), 0.0);
        let s = AxisConfig {
            sensitivity: 2.0,
            deadzone: 0.0,
            ..AxisConfig::default()
        };
        assert_eq!(s.normalize(0.75), 1.0);
    }

    #[test]
    fn default_mapping_and_no_spam_at_rest() {
        let mut m = InputMapper::new(Profile::default());
        m.process(&RawEvent::Connected {
            device: 1,
            name: "Xbox Controller".into(),
        });
        for _ in 0..1000 {
            assert!(
                m.process(&axis(1, Axis::LeftY, 0.02)).is_empty(),
                "stick au repos : aucune commande"
            );
        }
        let c = m.process(&axis(1, Axis::LeftY, 1.0));
        assert_eq!(
            c[0].0,
            Command::SetThrottle {
                engine: EngineSel::All,
                value: 1.0
            }
        );
        assert_eq!(c[0].1, Source::Gamepad(1));
        let back = m.process(&axis(1, Axis::LeftY, 0.0));
        assert_eq!(
            back[0].0,
            Command::SetThrottle {
                engine: EngineSel::All,
                value: 0.0
            }
        );
        assert!(m.process(&axis(1, Axis::LeftY, 0.0)).is_empty());
        let r = m.process(&axis(1, Axis::RightX, 1.0));
        assert_eq!(r[0].0, Command::SetRudder { deg: 40.0 });
        let rt = m.process(&axis(1, Axis::RightTrigger, 0.5));
        assert!(matches!(rt[0].0, Command::SetThrottle { value, .. } if value > 0.0));
        m.process(&axis(1, Axis::RightTrigger, 0.0));
        let lt = m.process(&axis(1, Axis::LeftTrigger, 1.0));
        assert_eq!(
            lt[0].0,
            Command::SetThrottle {
                engine: EngineSel::All,
                value: -1.0
            }
        );
        let btn = |b| RawEvent::Button {
            device: 1,
            button: b,
            pressed: true,
        };
        assert_eq!(m.process(&btn(Button::South))[0].0, Command::ToggleEngines);
        assert_eq!(m.process(&btn(Button::East))[0].0, Command::ToggleAutopilot);
        assert_eq!(m.process(&btn(Button::West))[0].0, Command::Reset);
        assert_eq!(m.process(&btn(Button::North))[0].0, Command::ToggleAnchor);
        assert!(
            m.process(&btn(Button::North)).is_empty(),
            "front montant seulement"
        );
        assert!(m.pressed(1, Button::North));
        assert_eq!(m.devices(), vec![(1, "Xbox Controller".to_string())]);
    }

    #[test]
    fn repeat_rate_mode_and_disconnect() {
        let mut p = Profile::default();
        p.axes.insert(
            Axis::LeftX,
            AxisConfig {
                action: AxisAction::Heading,
                mode: AxisMode::Rate,
                scale: 10.0,
                deadzone: 0.0,
                ..AxisConfig::default()
            },
        );
        p.on_disconnect = OnDisconnect::Center;
        let mut m = InputMapper::new(p);
        m.process(&RawEvent::Button {
            device: 2,
            button: Button::DPadRight,
            pressed: true,
        });
        let reps = m.tick(1000);
        assert!(reps.len() >= 4, "{}", reps.len());
        m.process(&RawEvent::Button {
            device: 2,
            button: Button::DPadRight,
            pressed: false,
        });
        assert!(m.tick(1000).is_empty());
        m.process(&axis(2, Axis::LeftX, 0.5));
        let rate = m.tick(1000);
        assert_eq!(rate[0].0, Command::NudgeHeading { deg: 5.0 });
        let d = m.process(&RawEvent::Disconnected { device: 2 });
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn profile_json_and_validation() {
        let p = Profile::default();
        let j = serde_json::to_string(&p).unwrap();
        let back: Profile = serde_json::from_str(&j).unwrap();
        assert_eq!(p, back);
        assert!(p.validate().is_ok());
        let mut bad = p.clone();
        bad.axes.get_mut(&Axis::LeftY).unwrap().deadzone = 0.7;
        assert!(bad.validate().unwrap_err().contains("deadzone"));
    }
}
