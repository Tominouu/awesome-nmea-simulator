//! Tests déterministes J0 : reproductibilité du PRNG graineable, et
//! propriétés statistiques minimales, sur la seule API publique.

use nmeasim_core::random::{Rng, SplitMix64, Xoshiro256StarStar};
use nmeasim_core::units;

fn draw(seed: u64, n: usize) -> Vec<u64> {
    let mut g = Xoshiro256StarStar::seed_from_u64(seed);
    (0..n).map(|_| g.next_u64()).collect()
}

#[test]
fn same_seed_same_sequence() {
    assert_eq!(draw(1234, 10_000), draw(1234, 10_000));
}

#[test]
fn different_seeds_diverge_immediately() {
    let a = draw(1234, 16);
    let b = draw(1235, 16);
    assert!(a.iter().zip(&b).all(|(x, y)| x != y));
}

#[test]
fn state_roundtrip_resumes_sequence() {
    let mut g = Xoshiro256StarStar::seed_from_u64(5);
    for _ in 0..100 {
        g.next_u64();
    }
    let saved = g.state();
    let expected: Vec<u64> = (0..50).map(|_| g.next_u64()).collect();
    let mut restored = Xoshiro256StarStar::from_state(saved).unwrap();
    let got: Vec<u64> = (0..50).map(|_| restored.next_u64()).collect();
    assert_eq!(got, expected);
}

#[test]
fn injected_as_trait_object() {
    // Les consommateurs reçoivent `&mut dyn Rng` : les deux générateurs
    // sont interchangeables.
    fn noise(rng: &mut dyn Rng) -> f64 {
        rng.next_symmetric() * 0.05
    }
    let mut a = Xoshiro256StarStar::seed_from_u64(1);
    let mut b = SplitMix64::new(1);
    assert!(noise(&mut a).abs() <= 0.05);
    assert!(noise(&mut b).abs() <= 0.05);
}

#[test]
fn uniform_mean_and_bounds() {
    let mut g = Xoshiro256StarStar::seed_from_u64(2026);
    let n = 200_000;
    let mut sum = 0.0;
    for _ in 0..n {
        let x = g.next_f64();
        assert!((0.0..1.0).contains(&x));
        sum += x;
    }
    let mean = sum / n as f64;
    // Écart-type de la moyenne : 1/sqrt(12 n) ≈ 6.5e-4 ; tolérance 5 sigma.
    assert!((mean - 0.5).abs() < 3.3e-3, "moyenne {mean}");
}

#[test]
fn below_is_roughly_uniform() {
    let mut g = Xoshiro256StarStar::seed_from_u64(77);
    let mut counts = [0u32; 10];
    let n = 100_000;
    for _ in 0..n {
        counts[g.below(10) as usize] += 1;
    }
    // Khi-deux à 9 degrés de liberté ; seuil 27.9 pour p = 0.001.
    let expected = n as f64 / 10.0;
    let chi2: f64 = counts
        .iter()
        .map(|&c| (c as f64 - expected).powi(2) / expected)
        .sum();
    assert!(chi2 < 27.9, "khi-deux {chi2}, comptes {counts:?}");
}

#[test]
fn units_are_usable_from_outside() {
    assert_eq!(units::normalize_deg(-15.0), 345.0);
    assert!((units::mps_to_knots(units::knots_to_mps(5.0)) - 5.0).abs() < 1e-12);
}
