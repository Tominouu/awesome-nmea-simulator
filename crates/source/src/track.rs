//! Traces : modèle commun GPX/KML et lecteur (`docs/10` §1-2).

use nmeasim_core::geo::LatLon;
use nmeasim_core::units::{mps_to_knots, rad_to_deg};
use serde::{Deserialize, Serialize};

/// Point de trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPoint {
    /// Position.
    pub position: LatLon,
    /// Heure, ms epoch.
    pub time_ms: Option<i64>,
    /// Altitude, m.
    pub elevation: Option<f64>,
    /// Cap, degrés (`<course>` GPX).
    pub course_deg: Option<f64>,
    /// Vitesse, m/s (`<speed>` GPX).
    pub speed_mps: Option<f64>,
}

impl TrackPoint {
    /// Point réduit à sa position.
    pub fn at(position: LatLon) -> Self {
        Self {
            position,
            time_ms: None,
            elevation: None,
            course_deg: None,
            speed_mps: None,
        }
    }
}

/// Trace nommée.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Nom.
    pub name: String,
    /// Points.
    pub points: Vec<TrackPoint>,
}

impl Track {
    /// Longueur totale, m.
    pub fn length_m(&self) -> f64 {
        self.points
            .windows(2)
            .map(|w| w[0].position.distance_to(w[1].position))
            .sum()
    }
}

/// Waypoint nommé (GPX `wpt`, KML `Point`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedPoint {
    /// Nom.
    pub name: String,
    /// Position.
    pub position: LatLon,
}

/// Contenu d'un fichier de trace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackFile {
    /// Traces et routes.
    pub tracks: Vec<Track>,
    /// Waypoints.
    pub waypoints: Vec<NamedPoint>,
}

/// Erreur de lecture explicite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError(pub String);

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SourceError {}

/// Mode de lecture d'une trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlayMode {
    /// Un point par tick de sortie (legacy).
    #[default]
    FixedRate,
    /// Temps réel : interpolation selon les horodatages.
    RealTime,
}

/// Sortie du lecteur pour un tick.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerOutput {
    /// Position.
    pub position: LatLon,
    /// Vitesse, nœuds, si connue.
    pub sog_kn: Option<f64>,
    /// Cap, degrés.
    pub cog_deg: Option<f64>,
    /// Altitude.
    pub altitude: Option<f64>,
    /// Heure du point.
    pub time_ms: Option<i64>,
    /// Index du point courant.
    pub index: usize,
    /// Dernier point atteint.
    pub ended: bool,
}

/// Lecteur de trace. Correction legacy : tous les points sont émis, y compris
/// le premier et le dernier (`docs/04` §5).
#[derive(Debug, Clone)]
pub struct TrackPlayer {
    track: Track,
    mode: PlayMode,
    repeat: bool,
    index: usize,
    /// Temps de trace écoulé en mode temps réel, ms.
    elapsed_ms: i64,
    ended: bool,
}

impl TrackPlayer {
    /// Nouveau lecteur ; une trace doit avoir au moins deux points (legacy).
    pub fn new(track: Track, mode: PlayMode, repeat: bool) -> Result<Self, SourceError> {
        if track.points.len() < 2 {
            return Err(SourceError(
                "la trace doit contenir au moins deux points".into(),
            ));
        }
        Ok(Self {
            track,
            mode,
            repeat,
            index: 0,
            elapsed_ms: 0,
            ended: false,
        })
    }

    /// Trace lue.
    pub fn track(&self) -> &Track {
        &self.track
    }

    /// Index courant.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Fin atteinte.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Revient au début.
    pub fn rewind(&mut self) {
        self.index = 0;
        self.elapsed_ms = 0;
        self.ended = false;
    }

    fn speed_course(&self, i: usize) -> (Option<f64>, Option<f64>) {
        let pts = &self.track.points;
        let p = &pts[i];
        let speed = p.speed_mps.map(mps_to_knots).or_else(|| {
            let (a, b) = if i + 1 < pts.len() {
                (&pts[i], &pts[i + 1])
            } else {
                (&pts[i - 1], &pts[i])
            };
            match (a.time_ms, b.time_ms) {
                (Some(ta), Some(tb)) if tb > ta => Some(mps_to_knots(
                    a.position.distance_to(b.position) / ((tb - ta) as f64 / 1000.0),
                )),
                _ => None,
            }
        });
        let course = p.course_deg.or_else(|| {
            let (a, b) = if i + 1 < pts.len() {
                (&pts[i], &pts[i + 1])
            } else {
                (&pts[i - 1], &pts[i])
            };
            Some(rad_to_deg(a.position.bearing_to(b.position)))
        });
        (speed, course)
    }

    /// Point émis au tick courant. `dt_ms` : durée du tick (mode temps réel).
    pub fn next(&mut self, dt_ms: i64) -> Option<PlayerOutput> {
        if self.ended {
            if !self.repeat {
                return None;
            }
            self.rewind();
        }
        let n = self.track.points.len();
        let out = match self.mode {
            PlayMode::FixedRate => {
                let i = self.index;
                let p = &self.track.points[i];
                let (sog, cog) = self.speed_course(i);
                self.index += 1;
                self.ended = self.index >= n;
                PlayerOutput {
                    position: p.position,
                    sog_kn: sog,
                    cog_deg: cog,
                    altitude: p.elevation,
                    time_ms: p.time_ms,
                    index: i,
                    ended: self.ended,
                }
            }
            PlayMode::RealTime => self.real_time(dt_ms),
        };
        Some(out)
    }

    fn real_time(&mut self, dt_ms: i64) -> PlayerOutput {
        let pts = &self.track.points;
        let t0 = pts[0].time_ms;
        let timed = t0.is_some() && pts.iter().all(|p| p.time_ms.is_some());
        if !timed {
            // Sans horodatage : repli sur la cadence fixe.
            self.mode = PlayMode::FixedRate;
            return self.next(dt_ms).expect("trace non vide");
        }
        let t0 = t0.unwrap_or(0);
        let now = t0 + self.elapsed_ms;
        self.elapsed_ms += dt_ms;
        let mut i = self.index;
        while i + 1 < pts.len() && pts[i + 1].time_ms.unwrap_or(0) <= now {
            i += 1;
        }
        self.index = i;
        if i + 1 >= pts.len() {
            self.ended = true;
            let (sog, cog) = self.speed_course(i);
            let p = &pts[i];
            return PlayerOutput {
                position: p.position,
                sog_kn: sog,
                cog_deg: cog,
                altitude: p.elevation,
                time_ms: p.time_ms,
                index: i,
                ended: true,
            };
        }
        let (a, b) = (&pts[i], &pts[i + 1]);
        let (ta, tb) = (a.time_ms.unwrap_or(0), b.time_ms.unwrap_or(0));
        let f = if tb > ta {
            (now - ta) as f64 / (tb - ta) as f64
        } else {
            0.0
        };
        // Interpolation sur l'arc de grand cercle.
        let d = a.position.distance_to(b.position);
        let brg = a.position.bearing_to(b.position);
        let pos = a.position.destination(brg, d * f.clamp(0.0, 1.0));
        let (sog, cog) = self.speed_course(i);
        let alt = match (a.elevation, b.elevation) {
            (Some(x), Some(y)) => Some(x + (y - x) * f),
            (x, _) => x,
        };
        PlayerOutput {
            position: pos,
            sog_kn: sog,
            cog_deg: cog,
            altitude: alt,
            time_ms: Some(now),
            index: i,
            ended: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize, timed: bool) -> Track {
        Track {
            name: "t".into(),
            points: (0..n)
                .map(|i| TrackPoint {
                    position: LatLon::new(-35.0, 138.5 + 0.001 * i as f64),
                    time_ms: timed.then_some(10_000 * i as i64),
                    elevation: None,
                    course_deg: None,
                    speed_mps: None,
                })
                .collect(),
        }
    }

    #[test]
    fn fixed_rate_emits_every_point_including_ends() {
        let mut p = TrackPlayer::new(line(5, true), PlayMode::FixedRate, false).unwrap();
        let outs: Vec<_> = std::iter::from_fn(|| p.next(1000)).collect();
        assert_eq!(outs.len(), 5, "legacy n'en émettait que 3");
        assert_eq!(outs[0].index, 0);
        assert!(outs[4].ended);
        // 0,001° à 35° S sur 10 s : 17,7 kn (valeur legacy observée).
        assert!((outs[0].sog_kn.unwrap() - 17.7).abs() < 0.05);
        assert!((outs[0].cog_deg.unwrap() - 90.0).abs() < 0.01);
        assert_eq!(outs[2].time_ms, Some(20_000));
    }

    #[test]
    fn real_time_interpolates_and_repeat() {
        let mut p = TrackPlayer::new(line(3, true), PlayMode::RealTime, true).unwrap();
        let a = p.next(5000).unwrap();
        let b = p.next(5000).unwrap();
        assert_eq!(a.index, 0);
        assert!((b.position.lon - 138.5005).abs() < 1e-6);
        for _ in 0..10 {
            p.next(5000);
        }
        assert!(!p.ended() || p.index() == 0);
    }

    #[test]
    fn untimed_real_time_falls_back() {
        let mut p = TrackPlayer::new(line(2, false), PlayMode::RealTime, false).unwrap();
        assert!(p.next(1000).unwrap().sog_kn.is_none());
        assert!(p.next(1000).unwrap().ended);
        assert!(p.next(1000).is_none());
    }

    #[test]
    fn one_point_rejected() {
        assert!(TrackPlayer::new(line(1, false), PlayMode::FixedRate, false).is_err());
    }
}
