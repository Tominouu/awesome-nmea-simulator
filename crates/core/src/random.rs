//! Aléa injecté et graineable.
//!
//! Le legacy tire tout de `Math.random()`, global et non adressable : aucune
//! session n'est reproductible. Ici, chaque consommateur reçoit un [`Rng`]
//! (`06-simulation-model.md` §12, `docs/decisions/003-bruit.md`). À graine
//! égale et séquence de commandes égale, la simulation est identique bit à bit.
//!
//! Générateurs :
//!
//! - [`Xoshiro256StarStar`] : générateur principal (Blackman et Vigna), rapide,
//!   période 2^256 − 1, sans dépendance ;
//! - [`SplitMix64`] : sert à dériver l'état de xoshiro à partir d'une graine
//!   de 64 bits, comme le recommandent les auteurs.
//!
//! Ces générateurs ne sont **pas** cryptographiques.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

/// Source d'aléa uniforme, injectée dans tout composant qui en a besoin.
pub trait Rng {
    /// Prochain entier uniforme sur 64 bits.
    fn next_u64(&mut self) -> u64;

    /// Prochain entier uniforme sur 32 bits (bits de poids fort).
    fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Réel uniforme dans `[0, 1)`, 53 bits de mantisse.
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Réel uniforme dans `[-1, 1)`, pour les bruits centrés.
    fn next_symmetric(&mut self) -> f64 {
        2.0 * self.next_f64() - 1.0
    }

    /// Réel uniforme entre `lo` et `hi` inclus.
    ///
    /// # Panics
    ///
    /// Si une borne n'est pas finie, si `lo > hi`, ou si `hi − lo` déborde.
    fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        let span = hi - lo;
        assert!(
            lo.is_finite() && hi.is_finite() && lo <= hi && span.is_finite(),
            "range_f64: bornes invalides [{lo}, {hi}]"
        );
        (lo + span * self.next_f64()).min(hi)
    }

    /// Entier uniforme dans `[0, n)`, sans biais (méthode de Lemire).
    ///
    /// # Panics
    ///
    /// Si `n == 0`.
    fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below: n doit être strictement positif");
        let mut m = u128::from(self.next_u64()) * u128::from(n);
        if (m as u64) < n {
            let threshold = n.wrapping_neg() % n;
            while (m as u64) < threshold {
                m = u128::from(self.next_u64()) * u128::from(n);
            }
        }
        (m >> 64) as u64
    }

    /// Vrai avec la probabilité `p`, bornée à `[0, 1]`.
    fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }
}

/// Générateur SplitMix64 (Steele, Lea et Flood ; constantes de Vigna).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Crée un générateur à partir d'une graine quelconque, zéro compris.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl Rng for SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Générateur xoshiro256** 1.0 (Blackman et Vigna, 2018).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xoshiro256StarStar {
    s: [u64; 4],
}

impl Xoshiro256StarStar {
    /// Crée un générateur depuis une graine de 64 bits : l'état est rempli
    /// par quatre tirages de [`SplitMix64`]. C'est la forme à utiliser
    /// partout ; la graine est celle écrite dans la config et le journal (D7).
    pub fn seed_from_u64(seed: u64) -> Self {
        let mut sm = SplitMix64::new(seed);
        Self {
            s: [sm.next_u64(), sm.next_u64(), sm.next_u64(), sm.next_u64()],
        }
    }

    /// Crée un générateur depuis un état brut. Rend `None` pour l'état nul,
    /// qui est un point fixe du générateur.
    pub fn from_state(s: [u64; 4]) -> Option<Self> {
        (s != [0; 4]).then_some(Self { s })
    }

    /// État interne courant, pour sauvegarde et restauration (SIM-12).
    pub fn state(&self) -> [u64; 4] {
        self.s
    }

    /// Dérive un générateur enfant indépendant et déterministe.
    ///
    /// Sert à donner à chaque sous-système (bruit du vent, GNSS…) son propre
    /// flux : ajouter un consommateur ne décale pas les tirages des autres,
    /// tant que les enfants sont dérivés dans le même ordre.
    pub fn fork(&mut self) -> Self {
        Self::seed_from_u64(self.next_u64())
    }
}

impl Rng for Xoshiro256StarStar {
    fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }
}

/// Graine non déterministe, pour une session sans graine configurée
/// (`randomSeed: null`). La valeur tirée doit être affichée et journalisée
/// pour que la session reste rejouable.
pub fn entropy_seed() -> u64 {
    let mut h = RandomState::new().build_hasher();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    h.write_u128(nanos);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Générateur scripté, pour forcer des valeurs précises.
    struct Scripted(Vec<u64>, usize);

    impl Rng for Scripted {
        fn next_u64(&mut self) -> u64 {
            let v = self.0[self.1 % self.0.len()];
            self.1 += 1;
            v
        }
    }

    #[test]
    fn splitmix_reference_seed_zero() {
        let mut g = SplitMix64::new(0);
        assert_eq!(g.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(g.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(g.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn xoshiro_reference_state_1234() {
        // Vecteur de référence publié pour l'état [1, 2, 3, 4].
        let mut g = Xoshiro256StarStar::from_state([1, 2, 3, 4]).unwrap();
        let got: Vec<u64> = (0..6).map(|_| g.next_u64()).collect();
        assert_eq!(
            got,
            [
                11520,
                0,
                1_509_978_240,
                1_215_971_899_390_074_240,
                1_216_172_134_540_287_360,
                607_988_272_756_665_600
            ]
        );
    }

    #[test]
    fn seed_from_u64_matches_independent_implementation() {
        // Valeurs calculées par une implémentation Python indépendante.
        let g = Xoshiro256StarStar::seed_from_u64(42);
        assert_eq!(
            g.state(),
            [
                13_679_457_532_755_275_413,
                2_949_826_092_126_892_291,
                5_139_283_748_462_763_858,
                6_349_198_060_258_255_764
            ]
        );
        let mut g = g;
        assert_eq!(g.next_u64(), 1_546_998_764_402_558_742);
        assert_eq!(g.next_u64(), 6_990_951_692_964_543_102);
        let mut g = Xoshiro256StarStar::seed_from_u64(42);
        assert_eq!(g.next_f64(), 0.083_862_971_059_882_16);
    }

    #[test]
    fn zero_state_rejected() {
        assert!(Xoshiro256StarStar::from_state([0; 4]).is_none());
    }

    #[test]
    fn next_u32_takes_high_bits() {
        let mut g = Scripted(vec![0xDEAD_BEEF_0000_0001], 0);
        assert_eq!(g.next_u32(), 0xDEAD_BEEF);
    }

    #[test]
    fn next_f64_extremes() {
        let mut g = Scripted(vec![0, u64::MAX], 0);
        assert_eq!(g.next_f64(), 0.0);
        let top = g.next_f64();
        assert!(top < 1.0 && top > 0.999_999_999_999_999);
    }

    #[test]
    fn symmetric_bounds() {
        let mut g = Scripted(vec![0, u64::MAX], 0);
        assert_eq!(g.next_symmetric(), -1.0);
        assert!(g.next_symmetric() < 1.0);
    }

    #[test]
    fn range_bounds_inclusive() {
        let mut g = Scripted(vec![0, u64::MAX], 0);
        assert_eq!(g.range_f64(-2.0, 3.0), -2.0);
        assert!(g.range_f64(-2.0, 3.0) <= 3.0);
        assert_eq!(g.range_f64(5.0, 5.0), 5.0);
        // Le min(hi) évite un dépassement d'arrondi.
        let mut g = Scripted(vec![u64::MAX], 0);
        assert!(g.range_f64(1e16, 1e16 + 2.0) <= 1e16 + 2.0);
    }

    #[test]
    #[should_panic(expected = "bornes invalides")]
    fn range_rejects_reversed() {
        SplitMix64::new(1).range_f64(1.0, 0.0);
    }

    #[test]
    #[should_panic(expected = "bornes invalides")]
    fn range_rejects_nan() {
        SplitMix64::new(1).range_f64(f64::NAN, 1.0);
    }

    #[test]
    #[should_panic(expected = "bornes invalides")]
    fn range_rejects_infinite_upper() {
        SplitMix64::new(1).range_f64(0.0, f64::INFINITY);
    }

    #[test]
    #[should_panic(expected = "bornes invalides")]
    fn range_rejects_overflowing_span() {
        SplitMix64::new(1).range_f64(-f64::MAX, f64::MAX);
    }

    #[test]
    fn below_is_bounded_and_exercises_rejection() {
        // n = 2^63 + 1 : environ un tirage sur deux passe par le rejet.
        let n = (1u64 << 63) + 1;
        let mut g = Xoshiro256StarStar::seed_from_u64(7);
        for _ in 0..1000 {
            assert!(g.below(n) < n);
        }
        let mut g = Xoshiro256StarStar::seed_from_u64(7);
        for _ in 0..1000 {
            assert!(g.below(6) < 6);
        }
        assert_eq!(g.below(1), 0);
    }

    #[test]
    fn below_rejection_loop_with_scripted_values() {
        // n = 3 : seuil = (2^64 − 3) mod 3 = 1. Le tirage 0 donne m = 0,
        // dont la partie basse 0 < 1 : rejeté ; le tirage suivant est accepté.
        let mut g = Scripted(vec![0, u64::MAX], 0);
        assert_eq!(g.below(3), 2);
        assert_eq!(g.1, 2);
    }

    #[test]
    #[should_panic(expected = "strictement positif")]
    fn below_rejects_zero() {
        SplitMix64::new(1).below(0);
    }

    #[test]
    fn chance_extremes() {
        let mut g = Xoshiro256StarStar::seed_from_u64(3);
        assert!((0..100).all(|_| !g.chance(0.0)));
        assert!((0..100).all(|_| g.chance(1.0)));
    }

    #[test]
    fn fork_is_deterministic_and_distinct() {
        let mut a = Xoshiro256StarStar::seed_from_u64(99);
        let mut b = Xoshiro256StarStar::seed_from_u64(99);
        let mut ca = a.fork();
        let mut cb = b.fork();
        assert_eq!(ca, cb);
        assert_eq!(ca.next_u64(), cb.next_u64());
        assert_ne!(ca.next_u64(), a.next_u64());
    }

    #[test]
    fn entropy_seed_varies() {
        // Deux RandomState distincts : collision de probabilité 2^-64.
        assert_ne!(entropy_seed(), entropy_seed());
    }
}
