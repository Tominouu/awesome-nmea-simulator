//! Bruit des grandeurs dérivantes (décision D3).

use nmeasim_core::random::Rng;
use nmeasim_core::units::{angle_diff_deg, normalize_deg};

use crate::config::DriftSpec;

/// Pas moderne : rappel exponentiel vers `nominal`, bruit additif absolu
/// proportionnel à `√dt`, bornes dures.
pub fn modern_step(x: f64, nominal: f64, spec: &DriftSpec, dt_s: f64, rng: &mut dyn Rng) -> f64 {
    if !spec.enabled {
        return x;
    }
    let pull = 1.0 - 0.5f64.powf(dt_s * 1000.0 / spec.half_life_ms.max(1.0));
    let noisy = x + (nominal - x) * pull + rng.next_symmetric() * spec.amplitude * dt_s.sqrt();
    noisy.clamp(spec.min, spec.max)
}

/// Variante circulaire pour les angles en degrés.
pub fn modern_step_deg(
    x: f64,
    nominal: f64,
    spec: &DriftSpec,
    dt_s: f64,
    rng: &mut dyn Rng,
) -> f64 {
    if !spec.enabled {
        return x;
    }
    let pull = 1.0 - 0.5f64.powf(dt_s * 1000.0 / spec.half_life_ms.max(1.0));
    let d = angle_diff_deg(x, nominal);
    normalize_deg(x + d * pull + rng.next_symmetric() * spec.amplitude * dt_s.sqrt())
}

/// Formule legacy `applySeed` exacte (`mods_b/7264.js`), sur le `Rng`
/// injecté : reproductible sans changer la loi.
pub fn legacy_step(x: f64, spec: &DriftSpec, rng: &mut dyn Rng) -> f64 {
    if !spec.enabled {
        return x;
    }
    let up = ((10.0 * rng.next_f64()).floor() as i64) % 2 == 0;
    let v = if x == 0.0 {
        if up { x + spec.plus } else { x - spec.minus }
    } else if up {
        x * (1.0 + spec.plus)
    } else {
        x / (1.0 + spec.minus)
    };
    v.min(spec.max).max(spec.min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmeasim_core::random::Xoshiro256StarStar;

    fn spec(min: f64, max: f64, amp: f64) -> DriftSpec {
        DriftSpec {
            min,
            max,
            amplitude: amp,
            half_life_ms: 10_000.0,
            plus: 0.02,
            minus: 0.02,
            enabled: true,
        }
    }

    #[test]
    fn modern_bounded_zero_mean() {
        let s = spec(0.0, 20.0, 0.5);
        let mut rng = Xoshiro256StarStar::seed_from_u64(1);
        let mut x = 10.0;
        let mut sum = 0.0;
        let n = 200_000;
        for _ in 0..n {
            x = modern_step(x, 10.0, &s, 0.1, &mut rng);
            assert!((0.0..=20.0).contains(&x));
            sum += x;
        }
        assert!((sum / n as f64 - 10.0).abs() < 0.2, "{}", sum / n as f64);
    }

    #[test]
    fn modern_heading_wrap() {
        let s = spec(0.0, 360.0, 0.3);
        let mut rng = Xoshiro256StarStar::seed_from_u64(2);
        let mut x = 359.0;
        for _ in 0..10_000 {
            let y = modern_step_deg(x, 359.5, &s, 0.1, &mut rng);
            assert!(angle_diff_deg(x, y).abs() < 1.0);
            x = y;
        }
    }

    #[test]
    fn legacy_two_state_floor() {
        // docs/04 §3.14 : 600 ↔ 612 rpm.
        let s = spec(600.0, 6000.0, 0.0);
        let mut rng = Xoshiro256StarStar::seed_from_u64(3);
        let mut seen = std::collections::BTreeSet::new();
        let mut x = 600.0;
        for _ in 0..200 {
            x = legacy_step(x, &s, &mut rng);
            if x < 613.0 {
                seen.insert((x * 1000.0).round() as i64);
            }
        }
        assert!(seen.contains(&600_000) && seen.contains(&612_000));
    }

    #[test]
    fn legacy_upward_bias_and_zero_branch() {
        let s = spec(-10.0, 1e9, 0.0);
        let mut rng = Xoshiro256StarStar::seed_from_u64(4);
        let mut x = 100.0;
        for _ in 0..5_000 {
            x = legacy_step(x, &s, &mut rng);
        }
        assert!(x > 100.0, "dérive ascendante attendue : {x}");
        let z = legacy_step(0.0, &s, &mut rng);
        assert!(z == 0.02 || z == -0.02);
    }

    #[test]
    fn disabled_is_identity() {
        let mut s = spec(0.0, 1.0, 1.0);
        s.enabled = false;
        let mut rng = Xoshiro256StarStar::seed_from_u64(5);
        assert_eq!(modern_step(0.5, 0.1, &s, 1.0, &mut rng), 0.5);
        assert_eq!(modern_step_deg(0.5, 0.1, &s, 1.0, &mut rng), 0.5);
        assert_eq!(legacy_step(0.5, &s, &mut rng), 0.5);
    }
}
