//! Profil de compatibilité (décision D1).
//!
//! Le profil est résolu une fois en [`CompatProfile`], structure immuable dont
//! chaque champ choisit une stratégie. Le moteur est unique. Les corrections de
//! bogues (`docs/04`, statut `FIX`) ne figurent **jamais** ici.

use serde::{Deserialize, Serialize};

/// Profil global.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Comportements corrigés et modèle physique (défaut).
    #[default]
    Modern,
    /// Comportements observables du legacy 1.6.1.
    Legacy,
}

/// Choix d'un comportement individuel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Behavior {
    /// Stratégie moderne.
    Modern,
    /// Stratégie legacy.
    Legacy,
}

/// Surcharges par comportement ; `None` suit le profil.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CompatOverrides {
    /// D1 — barre instantanée.
    pub rudder: Option<Behavior>,
    /// D2 — arrêt net du virage à barre nulle.
    pub zero_rudder: Option<Behavior>,
    /// D3 — dérive relative `applySeed`.
    pub noise: Option<Behavior>,
    /// D5 — `VBW` brut.
    pub vbw: Option<Behavior>,
    /// D5 — `RPM` avec pas `10.5`.
    pub rpm: Option<Behavior>,
    /// Propulsion : vitesse commandée directement (legacy) ou dynamique.
    pub propulsion: Option<Behavior>,
    /// Jeu de phrases : les 24 phrases legacy ou le jeu moderne.
    pub sentences: Option<Behavior>,
}

/// Configuration de compatibilité, telle qu'écrite dans la config.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompatConfig {
    /// Profil de base.
    pub profile: Profile,
    /// Surcharges.
    pub overrides: CompatOverrides,
}

/// Profil résolu, consulté par le moteur et les encodeurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatProfile {
    /// Profil de base.
    pub profile: Profile,
    /// Barre instantanée (D1).
    pub instant_rudder: bool,
    /// Virage coupé net à barre nulle (D2).
    pub stop_turn_at_zero: bool,
    /// Bruit relatif legacy (D3).
    pub legacy_noise: bool,
    /// `VBW` legacy (D5).
    pub legacy_vbw: bool,
    /// `RPM` legacy (D5).
    pub legacy_rpm: bool,
    /// Vitesse commandée directement, sans dynamique de propulsion.
    pub direct_speed: bool,
    /// Jeu de 24 phrases legacy.
    pub legacy_sentences: bool,
}

impl CompatConfig {
    /// Résout le profil.
    pub fn resolve(&self) -> CompatProfile {
        let base = self.profile == Profile::Legacy;
        let pick = |o: Option<Behavior>| o.map_or(base, |b| b == Behavior::Legacy);
        let o = &self.overrides;
        CompatProfile {
            profile: self.profile,
            instant_rudder: pick(o.rudder),
            stop_turn_at_zero: pick(o.zero_rudder),
            legacy_noise: pick(o.noise),
            legacy_vbw: pick(o.vbw),
            legacy_rpm: pick(o.rpm),
            direct_speed: pick(o.propulsion),
            legacy_sentences: pick(o.sentences),
        }
    }
}

impl CompatProfile {
    /// Profil moderne complet.
    pub fn modern() -> Self {
        CompatConfig::default().resolve()
    }

    /// Profil legacy complet.
    pub fn legacy() -> Self {
        CompatConfig {
            profile: Profile::Legacy,
            ..Default::default()
        }
        .resolve()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_profiles_and_overrides() {
        let m = CompatProfile::modern();
        assert!(!m.instant_rudder && !m.legacy_noise && !m.direct_speed);
        let l = CompatProfile::legacy();
        assert!(l.instant_rudder && l.stop_turn_at_zero && l.legacy_sentences);
        let mixed = CompatConfig {
            profile: Profile::Modern,
            overrides: CompatOverrides {
                rudder: Some(Behavior::Legacy),
                ..Default::default()
            },
        }
        .resolve();
        assert!(mixed.instant_rudder && !mixed.stop_turn_at_zero);
    }

    #[test]
    fn json_shape() {
        let c: CompatConfig =
            serde_json::from_str(r#"{"profile":"legacy","overrides":{"zeroRudder":"modern"}}"#)
                .unwrap();
        let p = c.resolve();
        assert!(p.instant_rudder && !p.stop_turn_at_zero);
    }
}
