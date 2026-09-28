//! Construction de phrases NMEA 0183 : champs, checksum, formats numériques.
//! Une phrase est produite **sans** terminateur : le `\r\n` appartient au
//! transport (`docs/07` §1.1).

use chrono::{DateTime, Datelike, Timelike, Utc};

/// XOR des octets, en hexadécimal majuscule sur deux caractères.
pub fn checksum(body: &str) -> String {
    format!("{:02X}", body.bytes().fold(0u8, |a, b| a ^ b))
}

/// Constructeur de phrase.
#[derive(Debug, Clone)]
pub struct Sentence {
    body: String,
}

impl Sentence {
    /// `start` vaut `$` ou `!` ; `talker` deux lettres ; `id` le formateur.
    pub fn new(start: char, talker: &str, id: &str) -> Self {
        let mut body = String::with_capacity(82);
        body.push(start);
        body.push_str(talker);
        body.push_str(id);
        Self { body }
    }

    /// Ajoute un champ.
    pub fn field(mut self, f: impl AsRef<str>) -> Self {
        self.body.push(',');
        self.body.push_str(f.as_ref());
        self
    }

    /// Ajoute `n` champs vides.
    pub fn empty(mut self, n: usize) -> Self {
        for _ in 0..n {
            self.body.push(',');
        }
        self
    }

    /// Termine : `<corps>*<CS>`.
    pub fn finish(self) -> String {
        let cs = checksum(&self.body[1..]);
        format!("{}*{}", self.body, cs)
    }
}

/// Valeur à `n` décimales avec la sémantique de `Number.prototype.toFixed` :
/// en cas d'égalité exacte, l'entier le plus grand en valeur absolue est
/// retenu (`2.25` → `2.3`), là où le formatage Rust arrondit au pair. Jamais
/// `-0.0`.
pub fn fixed(v: f64, n: usize) -> String {
    if !v.is_finite() {
        return String::new();
    }
    // Développement décimal exact sur n + 25 chiffres, puis arrondi manuel.
    let exact = format!("{:.*}", n + 25, v.abs());
    let (int_part, frac_part) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut digits: Vec<u8> = int_part
        .bytes()
        .chain(frac_part.bytes().take(n))
        .map(|b| b - b'0')
        .collect();
    let round_up = frac_part.as_bytes().get(n).is_some_and(|&d| d >= b'5');
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let int_len = digits.len() - n;
    let mut s: String = digits[..int_len]
        .iter()
        .map(|d| char::from(b'0' + d))
        .collect();
    if n > 0 {
        s.push('.');
        s.extend(digits[int_len..].iter().map(|d| char::from(b'0' + d)));
    }
    if v < 0.0 && digits.iter().any(|&d| d != 0) {
        s.insert(0, '-');
    }
    s
}

/// Coordonnée NMEA moderne : `ddmm.mmmmmm` / `dddmm.mmmmmm`, 6 décimales
/// fixes, arrondi (correction `docs/04` §3.3). Rend `(champ, hémisphère)`.
pub fn coord(value: f64, is_lat: bool) -> (String, &'static str) {
    let hemi = match (is_lat, value < 0.0) {
        (true, false) => "N",
        (true, true) => "S",
        (false, false) => "E",
        (false, true) => "W",
    };
    let micro = (value.abs() * 60.0 * 1e6).round() as u64;
    let deg = micro / 60_000_000;
    let rem = micro % 60_000_000;
    let (min, frac) = (rem / 1_000_000, rem % 1_000_000);
    let s = if is_lat {
        format!("{deg:02}{min:02}.{frac:06}")
    } else {
        format!("{deg:03}{min:02}.{frac:06}")
    };
    (s, hemi)
}

/// Reproduction du formateur legacy (`mods_b/745.js:50`), pour la
/// classification des divergences dans les tests de compatibilité : fraction
/// des minutes tronquée à 6 chiffres, longueur variable.
pub fn legacy_coord(value: f64, is_lat: bool) -> String {
    let nt = value.abs();
    let o = nt.floor();
    let gt = if o == 0.0 { nt % 1.0 } else { nt % o };
    let minutes = 60.0 * gt;
    let zt = minutes.floor();
    let ce = if zt == 0.0 {
        minutes % 1.0
    } else {
        minutes % zt
    };
    let s = format!("{ce}");
    let digits = match s.split_once('.') {
        Some((_, d)) => d.chars().take(6).collect::<String>(),
        None => "0".to_string(),
    };
    let deg = o as u64;
    let degs = if is_lat {
        format!("{deg:02}")
    } else {
        format!("{deg:03}")
    };
    format!("{degs}{:02}.{digits}", zt as u64)
}

/// Heure UTC `hhmmss.sss` (`formatTimeString`).
pub fn time_field(ms: i64) -> String {
    let t = utc(ms);
    format!(
        "{:02}{:02}{:02}.{:03}",
        t.hour(),
        t.minute(),
        t.second(),
        t.timestamp_subsec_millis()
    )
}

/// Date UTC `ddmmyy` (`formatDateString`).
pub fn date_field(ms: i64) -> String {
    let t = utc(ms);
    format!(
        "{:02}{:02}{:02}",
        t.day(),
        t.month(),
        t.year().rem_euclid(100)
    )
}

/// Instant UTC depuis des ms epoch.
pub fn utc(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap_or_default()
}

/// Horodatage ISO 8601 à la milliseconde (`toISOString`).
pub fn iso(ms: i64) -> String {
    utc(ms).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Préfixe propriétaire 61162-450 : `\c:<s>,s:<source>*CS\`.
pub fn prefix(ms: i64, source: &str) -> String {
    let body = format!("c:{},s:{}", ms.div_euclid(1000), source);
    format!("\\{}*{}\\", body, checksum(&body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_golden() {
        // legacy/cap-tcp.log
        assert_eq!(
            Sentence::new('$', "SD", "DBT")
                .field("26.2")
                .field("f")
                .field("8.0")
                .field("M")
                .field("4.4")
                .field("F")
                .finish(),
            "$SDDBT,26.2,f,8.0,M,4.4,F*38"
        );
        assert_eq!(
            Sentence::new('$', "II", "HDT")
                .field("0.0")
                .field("T")
                .finish(),
            "$IIHDT,0.0,T*22"
        );
        assert_eq!(
            Sentence::new('$', "GP", "RMB")
                .field("V")
                .empty(1)
                .field("R")
                .empty(10)
                .field("N")
                .finish(),
            "$GPRMB,V,,R,,,,,,,,,,,N*00"
        );
    }

    #[test]
    fn coordinates() {
        assert_eq!(coord(-35.0, true), ("3500.000000".into(), "S"));
        assert_eq!(coord(0.0, false), ("00000.000000".into(), "E"));
        assert_eq!(coord(138.503, false), ("13830.180000".into(), "E"));
        assert_eq!(coord(-34.99966098512356, true).0, "3459.979659");
        // Retenue : 59,9999999' arrondi à 60' passe au degré suivant.
        assert_eq!(coord(10.999_999_999_9, true).0, "1100.000000");
        assert_eq!(coord(-0.5, false), ("00030.000000".into(), "W"));
    }

    #[test]
    fn legacy_coordinates_reproduce_defect() {
        assert_eq!(legacy_coord(-35.0, true), "3500.0");
        assert_eq!(legacy_coord(0.0, false), "00000.0");
        assert_eq!(legacy_coord(138.503, false), "13830.179999");
    }

    #[test]
    fn times_and_prefix() {
        // 2026-09-28T08:30:54.938Z
        let ms = 1_790_584_254_938;
        assert_eq!(time_field(ms), "083054.938");
        assert_eq!(date_field(ms), "280926");
        assert_eq!(iso(ms), "2026-09-28T08:30:54.938Z");
        // legacy/cap-prefix.log : \c:1790584254,s:nmeasim*49\
        assert_eq!(prefix(ms, "nmeasim"), "\\c:1790584254,s:nmeasim*49\\");
        assert_eq!(fixed(-0.04, 1), "0.0");
        assert_eq!(fixed(2.25, 1), "2.3");
        assert_eq!(fixed(-2.25, 1), "-2.3");
        assert_eq!(fixed(9.96, 1), "10.0");
        assert_eq!(fixed(0.35, 1), "0.3"); // 0.35 vaut 0.34999… en binaire, comme en JS
        assert_eq!(fixed(1.0005, 3), "1.000"); // idem
        assert_eq!(fixed(7.0, 0), "7");
        assert_eq!(fixed(f64::NAN, 1), "");
    }
}
