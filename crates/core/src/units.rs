//! Conversions entre le SI du noyau et les unités nautiques.
//!
//! Le noyau stocke tout en SI : mètres, secondes, m/s, radians, kelvins
//! (`06-simulation-model.md` §1). Les unités nautiques n'apparaissent qu'au
//! bord, dans l'API et les encodeurs, via ces fonctions. Les facteurs sont les
//! définitions exactes ; un encodeur qui doit reproduire un arrondi legacy
//! (par exemple les facteurs `3.28084` et `0.546807` de `DBT`, D4) le fait
//! lui-même, explicitement.
//!
//! Les valeurs `NaN` se propagent ; aucune fonction ne panique.

use std::f64::consts::{PI, TAU};

/// Mètres par mille marin international (définition exacte).
pub const METERS_PER_NAUTICAL_MILE: f64 = 1852.0;
/// Mètres par pied international (définition exacte).
pub const METERS_PER_FOOT: f64 = 0.3048;
/// Mètres par brasse (6 pieds, définition exacte).
pub const METERS_PER_FATHOM: f64 = 1.8288;
/// Secondes par heure.
pub const SECONDS_PER_HOUR: f64 = 3600.0;
/// Décalage entre degrés Celsius et kelvins.
pub const KELVIN_OFFSET: f64 = 273.15;
/// Rayon terrestre moyen utilisé par la géodésie du legacy (sphère, mètres).
pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// Nœuds vers mètres par seconde.
pub fn knots_to_mps(kn: f64) -> f64 {
    kn * METERS_PER_NAUTICAL_MILE / SECONDS_PER_HOUR
}

/// Mètres par seconde vers nœuds.
pub fn mps_to_knots(mps: f64) -> f64 {
    mps * SECONDS_PER_HOUR / METERS_PER_NAUTICAL_MILE
}

/// Kilomètres par heure vers mètres par seconde.
pub fn kmh_to_mps(kmh: f64) -> f64 {
    kmh * 1000.0 / SECONDS_PER_HOUR
}

/// Mètres par seconde vers kilomètres par heure.
pub fn mps_to_kmh(mps: f64) -> f64 {
    mps * SECONDS_PER_HOUR / 1000.0
}

/// Milles marins vers mètres.
pub fn nautical_miles_to_meters(nm: f64) -> f64 {
    nm * METERS_PER_NAUTICAL_MILE
}

/// Mètres vers milles marins.
pub fn meters_to_nautical_miles(m: f64) -> f64 {
    m / METERS_PER_NAUTICAL_MILE
}

/// Pieds vers mètres.
pub fn feet_to_meters(ft: f64) -> f64 {
    ft * METERS_PER_FOOT
}

/// Mètres vers pieds.
pub fn meters_to_feet(m: f64) -> f64 {
    m / METERS_PER_FOOT
}

/// Brasses vers mètres.
pub fn fathoms_to_meters(fm: f64) -> f64 {
    fm * METERS_PER_FATHOM
}

/// Mètres vers brasses.
pub fn meters_to_fathoms(m: f64) -> f64 {
    m / METERS_PER_FATHOM
}

/// Degrés Celsius vers kelvins.
pub fn celsius_to_kelvin(c: f64) -> f64 {
    c + KELVIN_OFFSET
}

/// Kelvins vers degrés Celsius.
pub fn kelvin_to_celsius(k: f64) -> f64 {
    k - KELVIN_OFFSET
}

/// Degrés vers radians.
pub fn deg_to_rad(deg: f64) -> f64 {
    deg.to_radians()
}

/// Radians vers degrés.
pub fn rad_to_deg(rad: f64) -> f64 {
    rad.to_degrees()
}

/// Ramène un angle en degrés dans `[0, 360)`.
///
/// `rem_euclid` peut rendre exactement `360.0` pour une très petite valeur
/// négative (arrondi) ; ce cas est replié sur `0.0`.
pub fn normalize_deg(deg: f64) -> f64 {
    wrap(deg, 360.0)
}

/// Ramène un angle en radians dans `[0, 2π)`.
pub fn normalize_rad(rad: f64) -> f64 {
    wrap(rad, TAU)
}

/// Écart angulaire signé le plus court de `from` vers `to`, en degrés,
/// dans `(-180, 180]`.
pub fn angle_diff_deg(from: f64, to: f64) -> f64 {
    let d = normalize_deg(to - from);
    if d > 180.0 { d - 360.0 } else { d }
}

/// Écart angulaire signé le plus court de `from` vers `to`, en radians,
/// dans `(-π, π]`.
pub fn angle_diff_rad(from: f64, to: f64) -> f64 {
    let d = normalize_rad(to - from);
    if d > PI { d - TAU } else { d }
}

fn wrap(x: f64, period: f64) -> f64 {
    let r = x.rem_euclid(period);
    if r >= period { 0.0 } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-12;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= EPS * b.abs().max(1.0)
    }

    #[test]
    fn knots_and_mps() {
        assert!(close(knots_to_mps(1.0), 1852.0 / 3600.0));
        assert!(close(mps_to_knots(knots_to_mps(6.3)), 6.3));
        // docs/02 §3.4 : 10 kn pendant 1 s = 5.144 m.
        assert!((knots_to_mps(10.0) - 5.1444).abs() < 1e-4);
    }

    #[test]
    fn kmh_and_mps() {
        assert!(close(kmh_to_mps(3.6), 1.0));
        assert!(close(mps_to_kmh(1.0), 3.6));
        // VTG legacy : 10.0 kn affiché 18.5 km/h.
        assert!((mps_to_kmh(knots_to_mps(10.0)) - 18.52).abs() < 1e-9);
    }

    #[test]
    fn distances() {
        assert!(close(nautical_miles_to_meters(1.0), 1852.0));
        assert!(close(meters_to_nautical_miles(1852.0), 1.0));
        assert!(close(feet_to_meters(1.0), 0.3048));
        assert!(close(meters_to_feet(0.3048), 1.0));
        assert!(close(fathoms_to_meters(1.0), 1.8288));
        assert!(close(meters_to_fathoms(1.8288), 1.0));
    }

    #[test]
    fn dbt_capture_values() {
        // legacy/cap-tcp.log : $SDDBT,26.2,f,8.0,M,4.4,F pour 8,0 m (D4).
        assert_eq!(format!("{:.1}", meters_to_feet(8.0)), "26.2");
        assert_eq!(format!("{:.1}", meters_to_fathoms(8.0)), "4.4");
        assert_eq!(format!("{:.1}", meters_to_feet(10.2)), "33.5");
        assert_eq!(format!("{:.1}", meters_to_fathoms(10.2)), "5.6");
    }

    #[test]
    fn temperatures() {
        assert!(close(celsius_to_kelvin(85.0), 358.15));
        assert!(close(kelvin_to_celsius(273.15), 0.0));
        assert!(close(kelvin_to_celsius(celsius_to_kelvin(7.5)), 7.5));
    }

    #[test]
    fn angles() {
        assert!(close(deg_to_rad(180.0), PI));
        assert!(close(rad_to_deg(PI / 2.0), 90.0));
    }

    #[test]
    fn normalize_degrees() {
        assert_eq!(normalize_deg(0.0), 0.0);
        assert_eq!(normalize_deg(360.0), 0.0);
        assert_eq!(normalize_deg(-90.0), 270.0);
        assert_eq!(normalize_deg(725.0), 5.0);
        // Cas d'arrondi : rem_euclid rendrait 360.0.
        let r = normalize_deg(-1e-17);
        assert!((0.0..360.0).contains(&r), "{r}");
        assert!(normalize_deg(f64::NAN).is_nan());
    }

    #[test]
    fn normalize_radians() {
        assert!(close(normalize_rad(-PI / 2.0), 1.5 * PI));
        assert!(close(normalize_rad(TAU + 1.0), 1.0));
        let r = normalize_rad(-1e-17);
        assert!((0.0..TAU).contains(&r), "{r}");
    }

    #[test]
    fn signed_differences_degrees() {
        assert_eq!(angle_diff_deg(350.0, 10.0), 20.0);
        assert_eq!(angle_diff_deg(10.0, 350.0), -20.0);
        assert_eq!(angle_diff_deg(0.0, 180.0), 180.0);
        assert_eq!(angle_diff_deg(180.0, 0.0), 180.0);
        assert_eq!(angle_diff_deg(15.0, 15.0), 0.0);
    }

    #[test]
    fn signed_differences_radians() {
        assert!(close(angle_diff_rad(0.1, TAU - 0.1), -0.2));
        assert!(close(angle_diff_rad(TAU - 0.1, 0.1), 0.2));
        assert!(close(angle_diff_rad(0.0, PI), PI));
    }
}
