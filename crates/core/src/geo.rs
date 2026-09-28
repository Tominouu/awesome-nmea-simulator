//! Géodésie sphérique, rayon moyen 6 371 km, comme le legacy
//! (`geodesy/latlon-spherical`). Positions en degrés décimaux, relèvements en
//! radians depuis le nord vrai, distances en mètres.

use crate::units::{EARTH_RADIUS_M, normalize_rad};

/// Position géographique en degrés décimaux (WGS84, sphère pour les calculs).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LatLon {
    /// Latitude, degrés, positive au nord.
    pub lat: f64,
    /// Longitude, degrés, positive à l'est.
    pub lon: f64,
}

impl LatLon {
    /// Construit une position.
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// Point atteint depuis `self` en suivant le relèvement initial
    /// `bearing` (rad) sur `distance` mètres (grand cercle).
    pub fn destination(self, bearing: f64, distance: f64) -> LatLon {
        let delta = distance / EARTH_RADIUS_M;
        let phi1 = self.lat.to_radians();
        let lambda1 = self.lon.to_radians();
        let sin_phi2 = phi1.sin() * delta.cos() + phi1.cos() * delta.sin() * bearing.cos();
        let phi2 = sin_phi2.clamp(-1.0, 1.0).asin();
        let y = bearing.sin() * delta.sin() * phi1.cos();
        let x = delta.cos() - phi1.sin() * sin_phi2;
        let lambda2 = lambda1 + y.atan2(x);
        LatLon::new(phi2.to_degrees(), normalize_lon(lambda2.to_degrees()))
    }

    /// Distance orthodromique en mètres (haversine).
    pub fn distance_to(self, other: LatLon) -> f64 {
        let phi1 = self.lat.to_radians();
        let phi2 = other.lat.to_radians();
        let dphi = phi2 - phi1;
        let dlambda = (other.lon - self.lon).to_radians();
        let a =
            (dphi / 2.0).sin().powi(2) + phi1.cos() * phi2.cos() * (dlambda / 2.0).sin().powi(2);
        2.0 * EARTH_RADIUS_M * a.sqrt().atan2((1.0 - a).sqrt())
    }

    /// Relèvement initial vers `other`, radians dans `[0, 2π)`.
    pub fn bearing_to(self, other: LatLon) -> f64 {
        let phi1 = self.lat.to_radians();
        let phi2 = other.lat.to_radians();
        let dlambda = (other.lon - self.lon).to_radians();
        let y = dlambda.sin() * phi2.cos();
        let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * dlambda.cos();
        normalize_rad(y.atan2(x))
    }

    /// Distance signée au grand cercle `start → end`, en mètres :
    /// négative à gauche de la route, positive à droite.
    pub fn cross_track_distance(self, start: LatLon, end: LatLon) -> f64 {
        let d13 = start.distance_to(self) / EARTH_RADIUS_M;
        let t13 = start.bearing_to(self);
        let t12 = start.bearing_to(end);
        (d13.sin() * (t13 - t12).sin()).clamp(-1.0, 1.0).asin() * EARTH_RADIUS_M
    }

    /// Distance le long de la route `start → end` jusqu'au pied de la
    /// perpendiculaire depuis `self`, en mètres (négative avant `start`).
    pub fn along_track_distance(self, start: LatLon, end: LatLon) -> f64 {
        let d13 = start.distance_to(self) / EARTH_RADIUS_M;
        let t13 = start.bearing_to(self);
        let t12 = start.bearing_to(end);
        let dxt = (d13.sin() * (t13 - t12).sin()).clamp(-1.0, 1.0).asin();
        let dat = (d13.cos() / dxt.cos()).clamp(-1.0, 1.0).acos();
        dat * (t12 - t13).cos().signum() * EARTH_RADIUS_M
    }

    /// Vrai si les deux coordonnées sont finies et dans leurs plages.
    pub fn is_valid(self) -> bool {
        self.lat.is_finite()
            && self.lon.is_finite()
            && (-90.0..=90.0).contains(&self.lat)
            && (-180.0..=180.0).contains(&self.lon)
    }
}

/// Ramène une longitude dans `[-180, 180)`.
pub fn normalize_lon(lon: f64) -> f64 {
    let r = (lon + 180.0).rem_euclid(360.0) - 180.0;
    if r >= 180.0 { -180.0 } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{deg_to_rad, knots_to_mps};

    #[test]
    fn legacy_step_10kn_one_second() {
        // docs/02 §3.4 : 5.144 m attendus à 10 kn pendant 1 s.
        let p = LatLon::new(-35.0, 138.5);
        let q = p.destination(0.0, knots_to_mps(10.0));
        assert!((p.distance_to(q) - 5.1444).abs() < 1e-3);
        assert!(q.lat > p.lat);
    }

    #[test]
    fn rmb_capture_distance_and_bearing() {
        // docs/04 §3.2 : destination réelle dans l'hémisphère sud.
        let ship = LatLon::new(-34.990_259_3, 138.508_152_53);
        let dest = LatLon::new(-(36.0 + 31.460_336 / 60.0), 136.0 + 21.430_572 / 60.0);
        let nm = ship.distance_to(dest) / 1852.0;
        assert!((nm - 139.5).abs() < 0.2, "{nm}");
        assert!((ship.bearing_to(dest).to_degrees() - 228.1).abs() < 0.1);
    }

    #[test]
    fn cross_track_sign() {
        let a = LatLon::new(0.0, 0.0);
        let b = LatLon::new(0.0, 1.0); // route plein est
        assert!(LatLon::new(0.01, 0.5).cross_track_distance(a, b) < 0.0); // nord = gauche
        assert!(LatLon::new(-0.01, 0.5).cross_track_distance(a, b) > 0.0);
        let at = LatLon::new(0.0, 0.5).along_track_distance(a, b);
        assert!((at - a.distance_to(LatLon::new(0.0, 0.5))).abs() < 1.0);
        assert!(LatLon::new(0.0, -0.5).along_track_distance(a, b) < 0.0);
    }

    #[test]
    fn destination_roundtrip() {
        let p = LatLon::new(48.0, -4.5);
        let q = p.destination(deg_to_rad(123.0), 10_000.0);
        assert!((p.distance_to(q) - 10_000.0).abs() < 1e-6);
        assert!((p.bearing_to(q).to_degrees() - 123.0).abs() < 1e-6);
    }

    #[test]
    fn longitude_wrap_and_validity() {
        assert_eq!(normalize_lon(190.0), -170.0);
        assert_eq!(normalize_lon(180.0), -180.0);
        let q = LatLon::new(0.0, 179.9999).destination(deg_to_rad(90.0), 1000.0);
        assert!(q.lon < -179.0 && q.is_valid());
        assert!(!LatLon::new(91.0, 0.0).is_valid());
        assert!(!LatLon::new(f64::NAN, 0.0).is_valid());
    }
}
