//! AIS : messages 1 et 5, armure 6 bits, fragmentation (`docs/07` §6, D6).

use serde::{Deserialize, Serialize};

use crate::sentence::Sentence;

/// Identité AIS (D6). Défauts = valeurs du legacy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AisIdentity {
    /// MMSI, 9 chiffres.
    pub mmsi: u32,
    /// Indicatif, ≤ 7 caractères.
    pub callsign: String,
    /// Nom, ≤ 20 caractères.
    pub name: String,
    /// Type de navire.
    pub ship_type: u8,
    /// Numéro IMO.
    pub imo: u32,
    /// Destination, ≤ 20 caractères.
    pub destination: String,
    /// Identifiant de séquence multi-fragments, 0..9.
    pub sequential_id: u8,
    /// Distances antenne → étrave, poupe, bâbord, tribord, m.
    pub dimensions: [u16; 4],
    /// Tirant d'eau, m.
    pub draught_m: f64,
    /// ETA (ms epoch UTC) ; `None` = heure courante, comme le legacy.
    pub eta_ms: Option<i64>,
}

impl Default for AisIdentity {
    fn default() -> Self {
        Self {
            mmsi: 503_999_999,
            callsign: "SIM1234".into(),
            name: "NMEASIM".into(),
            ship_type: 37,
            imo: 0,
            destination: "SYDNEY".into(),
            sequential_id: 9,
            dimensions: [10, 10, 5, 5],
            draught_m: 1.0,
            eta_ms: None,
        }
    }
}

impl AisIdentity {
    /// Validation explicite (D6) : rend le champ fautif.
    pub fn validate(&self) -> Result<(), (String, String)> {
        let err = |f: &str, r: &str| Err((f.to_string(), r.to_string()));
        if !(100_000_000..=999_999_999).contains(&self.mmsi) {
            return err("ais.mmsi", "doit comporter 9 chiffres");
        }
        for (f, v, max) in [
            ("ais.callsign", &self.callsign, 7),
            ("ais.name", &self.name, 20),
            ("ais.destination", &self.destination, 20),
        ] {
            if v.chars().count() > max {
                return err(f, &format!("au plus {max} caractères"));
            }
            if v.chars().any(|c| sixbit_char(c).is_none()) {
                return err(f, "caractère hors jeu AIS 6 bits");
            }
        }
        if self.sequential_id > 9 {
            return err("ais.sequentialId", "doit être dans 0..=9");
        }
        if !(0.0..=25.5).contains(&self.draught_m) {
            return err("ais.draughtM", "doit être dans 0..=25.5");
        }
        Ok(())
    }
}

fn sixbit_char(c: char) -> Option<u8> {
    let c = c.to_ascii_uppercase() as u32;
    match c {
        64..=95 => Some((c - 64) as u8),
        32..=63 => Some(c as u8),
        _ => None,
    }
}

/// Accumulateur de bits.
#[derive(Debug, Default, Clone)]
pub struct Bits {
    bits: Vec<bool>,
}

impl Bits {
    /// Entier non signé sur `n` bits (tronqué aux `n` bits de poids faible).
    pub fn uint(&mut self, v: u64, n: usize) -> &mut Self {
        for i in (0..n).rev() {
            self.bits.push((v >> i) & 1 == 1);
        }
        self
    }

    /// Entier signé en complément à deux sur `n` bits.
    pub fn int(&mut self, v: i64, n: usize) -> &mut Self {
        self.uint(v as u64 & ((1u64 << n) - 1), n)
    }

    /// Chaîne 6 bits, complétée par `@`, tronquée à `n / 6` caractères.
    pub fn text(&mut self, s: &str, n: usize) -> &mut Self {
        let len = n / 6;
        let mut chars: Vec<u8> = s
            .chars()
            .take(len)
            .map(|c| sixbit_char(c).unwrap_or(0))
            .collect();
        chars.resize(len, 0);
        for c in chars {
            self.uint(u64::from(c), 6);
        }
        self
    }

    /// Nombre de bits.
    pub fn len(&self) -> usize {
        self.bits.len()
    }

    /// Vide ?
    pub fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }

    /// Armure 6 bits. `legacy_padding` reproduit le legacy : le dernier groupe
    /// incomplet est lu aligné à droite au lieu d'être complété par des zéros
    /// à droite. Rend `(charge utile, bits de bourrage)`.
    pub fn armor(&self, legacy_padding: bool) -> (String, usize) {
        let mut out = String::new();
        let fill = (6 - self.bits.len() % 6) % 6;
        for chunk in self.bits.chunks(6) {
            let mut v = chunk.iter().fold(0u8, |a, &b| (a << 1) | u8::from(b));
            if chunk.len() < 6 && !legacy_padding {
                v <<= 6 - chunk.len();
            }
            out.push(char::from(if v < 40 { v + 48 } else { v + 56 }));
        }
        (out, fill)
    }
}

/// Paramètres dynamiques du message 1.
#[derive(Debug, Clone, Copy)]
pub struct PositionReport {
    /// Vitesse fond, nœuds.
    pub sog_kn: f64,
    /// Route fond, degrés.
    pub cog_deg: f64,
    /// Cap vrai, degrés.
    pub heading_deg: f64,
    /// Latitude, degrés.
    pub lat: f64,
    /// Longitude, degrés.
    pub lon: f64,
    /// Taux de giration, °/min (`None` = indisponible, 128).
    pub rot_deg_min: Option<f64>,
    /// Seconde UTC du point.
    pub utc_second: u32,
    /// Statut de navigation (0 = en route au moteur, 1 = au mouillage).
    pub nav_status: u8,
}

/// Message 1. `legacy` reproduit les troncatures du legacy (`parseInt`).
pub fn message1(mmsi: u32, p: &PositionReport, legacy: bool) -> Bits {
    let conv = |x: f64| if legacy { x.trunc() } else { x.round() };
    let rot = match (legacy, p.rot_deg_min) {
        (true, _) | (false, None) => -128,
        (false, Some(r)) => {
            let v = (4.733 * r.abs().sqrt()).round().min(126.0) * r.signum();
            v as i64
        }
    };
    let mut b = Bits::default();
    b.uint(1, 6)
        .uint(0, 2)
        .uint(u64::from(mmsi), 30)
        .uint(u64::from(p.nav_status), 4)
        .int(rot, 8)
        .uint(conv(10.0 * p.sog_kn).clamp(0.0, 1022.0) as u64, 10)
        .uint(1, 1)
        .int(conv(600_000.0 * p.lon) as i64, 28)
        .int(conv(600_000.0 * p.lat) as i64, 27)
        .uint(conv(10.0 * p.cog_deg) as u64 % 3600, 12)
        .uint(conv(p.heading_deg) as u64 % 360, 9)
        .uint(u64::from(p.utc_second % 60), 6)
        .uint(0, 2)
        .uint(0, 3)
        .uint(0, 1)
        .uint(0, 19);
    b
}

/// ETA (mois, jour, heure, minute).
pub type Eta = (u32, u32, u32, u32);

/// Message 5.
pub fn message5(id: &AisIdentity, eta: Eta) -> Bits {
    let [bow, stern, port, stbd] = id.dimensions;
    let mut b = Bits::default();
    b.uint(5, 6)
        .uint(0, 2)
        .uint(u64::from(id.mmsi), 30)
        .uint(0, 2)
        .uint(u64::from(id.imo), 30)
        .text(&id.callsign, 42)
        .text(&id.name, 120)
        .uint(u64::from(id.ship_type), 8)
        .uint(u64::from(bow), 9)
        .uint(u64::from(stern), 9)
        .uint(u64::from(port), 6)
        .uint(u64::from(stbd), 6)
        .uint(0, 4)
        .uint(u64::from(eta.0), 4)
        .uint(u64::from(eta.1), 5)
        .uint(u64::from(eta.2), 5)
        .uint(u64::from(eta.3), 6)
        .uint((id.draught_m * 10.0).round() as u64, 8)
        .text(&id.destination, 120)
        .uint(1, 1)
        .uint(0, 1);
    b
}

/// Phrases `!AIVDx` pour une charge utile, fragments de 56 caractères.
pub fn sentences(
    bits: &Bits,
    talker: &str,
    formatter: &str,
    seq_id: u8,
    legacy_padding: bool,
) -> Vec<String> {
    let (payload, fill) = bits.armor(legacy_padding);
    let chunks: Vec<&str> = payload
        .as_bytes()
        .chunks(56)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    let total = chunks.len();
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let last = i + 1 == total;
            // Legacy : `6 / (len % 4)` ; coïncide avec le vrai bourrage pour les
            // longueurs émises (0 et 2), on émet le vrai bourrage.
            let f = if last { fill } else { 0 };
            Sentence::new('!', talker, formatter)
                .field(total.to_string())
                .field((i + 1).to_string())
                .field(if total > 1 {
                    seq_id.to_string()
                } else {
                    String::new()
                })
                .field("A")
                .field(*c)
                .field(f.to_string())
                .finish()
        })
        .collect()
}

/// Décodage minimal d'une charge utile (tests et moniteur) : rend les bits.
pub fn dearmor(payload: &str) -> Vec<bool> {
    let mut v = Vec::new();
    for ch in payload.bytes() {
        let mut x = ch - 48;
        if x > 40 {
            x -= 8;
        }
        for i in (0..6).rev() {
            v.push((x >> i) & 1 == 1);
        }
    }
    v
}

/// Lit un entier non signé dans des bits dé-armurés.
pub fn read_uint(bits: &[bool], start: usize, n: usize) -> u64 {
    bits[start..start + n]
        .iter()
        .fold(0, |a, &b| (a << 1) | u64::from(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message5_matches_golden_except_legacy_padding() {
        // legacy/cap-tcp.log : type 5 avec ETA 28/09 08:25.
        let id = AisIdentity::default();
        let b = message5(&id, (9, 28, 8, 25));
        assert_eq!(b.len(), 424);
        let legacy = sentences(&b, "AI", "VDO", 9, true);
        assert_eq!(
            legacy[0],
            "!AIVDO,2,1,9,A,57Paewh00001<To7;?@plD5<Tl0000000000000U1@:552N8I2TnA3QF,0*35"
        );
        assert_eq!(legacy[1], "!AIVDO,2,2,9,A,@00000000000002,2*5D");
        let modern = sentences(&b, "AI", "VDO", 9, false);
        assert_eq!(modern[1], "!AIVDO,2,2,9,A,@00000000000008,2*57");
    }

    #[test]
    fn message1_decodes_mmsi_and_position() {
        let p = PositionReport {
            sog_kn: 5.0,
            cog_deg: 14.3,
            heading_deg: 14.3,
            lat: -34.999_977_35,
            lon: 138.500_007_03,
            rot_deg_min: None,
            utc_second: 21,
            nav_status: 0,
        };
        let b = message1(503_999_999, &p, true);
        let s = sentences(&b, "AI", "VDO", 9, true);
        assert_eq!(s.len(), 1);
        assert!(s[0].starts_with("!AIVDO,1,1,,A,17Paewh"));
        let bits = dearmor(s[0].split(',').nth(5).unwrap());
        assert_eq!(read_uint(&bits, 8, 30), 503_999_999);
        assert_eq!(read_uint(&bits, 50, 10), 50);
        let m = message1(
            503_999_999,
            &PositionReport {
                rot_deg_min: Some(-30.0),
                ..p
            },
            false,
        );
        let bits = dearmor(&m.armor(false).0);
        assert_eq!(read_uint(&bits, 42, 8), (256 - 26) as u64); // -26 en complément à 2
    }

    #[test]
    fn identity_validation() {
        assert!(AisIdentity::default().validate().is_ok());
        let bad = AisIdentity {
            mmsi: 12345,
            ..Default::default()
        };
        assert_eq!(bad.validate().unwrap_err().0, "ais.mmsi");
        let bad = AisIdentity {
            name: "X".repeat(21),
            ..Default::default()
        };
        assert_eq!(bad.validate().unwrap_err().0, "ais.name");
        let bad = AisIdentity {
            callsign: "é".into(),
            ..Default::default()
        };
        assert!(bad.validate().is_err());
    }
}
