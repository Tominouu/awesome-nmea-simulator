//! Analyse de phrases NMEA 0183 (moniteur, entrée série, tests de
//! compatibilité). Tolérante : une phrase invalide est signalée, jamais fatale.

use crate::sentence::checksum;

/// Phrase analysée.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// Préfixe 61162-450 éventuel, sans les antislashs.
    pub tag_block: Option<String>,
    /// `$` ou `!`.
    pub start: char,
    /// Talker (2 lettres, ou `P` pour les propriétaires).
    pub talker: String,
    /// Formateur (`RMC`…).
    pub formatter: String,
    /// Champs après l'adresse.
    pub fields: Vec<String>,
    /// Checksum présent.
    pub checksum: Option<String>,
    /// Checksum présent et exact.
    pub checksum_ok: bool,
}

impl Parsed {
    /// Champ `i` (0 = premier champ après l'adresse), vide si absent.
    pub fn f(&self, i: usize) -> &str {
        self.fields.get(i).map_or("", String::as_str)
    }

    /// Champ numérique.
    pub fn num(&self, i: usize) -> Option<f64> {
        self.f(i).parse().ok()
    }
}

/// Analyse une ligne (avec ou sans CRLF, avec ou sans préfixe).
pub fn parse(line: &str) -> Option<Parsed> {
    let mut line = line.trim_end_matches(['\r', '\n']);
    let mut tag_block = None;
    if let Some(rest) = line.strip_prefix('\\') {
        let end = rest.find('\\')?;
        tag_block = Some(rest[..end].to_string());
        line = &rest[end + 1..];
    }
    let start = line.chars().next()?;
    if start != '$' && start != '!' {
        return None;
    }
    let (body, cs) = match line[1..].rsplit_once('*') {
        Some((b, c)) => (b, Some(c.trim().to_string())),
        None => (&line[1..], None),
    };
    let mut parts = body.split(',');
    let addr = parts.next()?;
    if addr.len() < 3 {
        return None;
    }
    let (talker, formatter) = if let Some(rest) = addr.strip_prefix('P') {
        ("P".to_string(), rest.to_string())
    } else {
        (addr[..2].to_string(), addr[2..].to_string())
    };
    let checksum_ok = cs
        .as_deref()
        .is_some_and(|c| c.eq_ignore_ascii_case(&checksum(body)));
    Some(Parsed {
        tag_block,
        start,
        talker,
        formatter,
        fields: parts.map(str::to_string).collect(),
        checksum: cs,
        checksum_ok,
    })
}

/// Coordonnée NMEA (`ddmm.mmmm`, hémisphère) vers degrés décimaux.
pub fn coord(value: &str, hemi: &str) -> Option<f64> {
    let dot = value.find('.').unwrap_or(value.len());
    if dot < 2 {
        return None;
    }
    let deg: f64 = value[..dot - 2].parse().ok()?;
    let min: f64 = value[dot - 2..].parse().ok()?;
    let v = deg + min / 60.0;
    Some(if hemi == "S" || hemi == "W" { -v } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prefixed_and_plain() {
        let p = parse("\\c:1790584254,s:nmeasim*49\\$IIHDT,13.0,T*10\r\n").unwrap();
        assert_eq!(p.tag_block.as_deref(), Some("c:1790584254,s:nmeasim*49"));
        assert_eq!((p.talker.as_str(), p.formatter.as_str()), ("II", "HDT"));
        assert!(p.checksum_ok);
        assert_eq!(p.num(0), Some(13.0));
        let bad = parse("$IIHDT,13.0,T*11").unwrap();
        assert!(!bad.checksum_ok);
        let ais = parse("!AIVDO,2,2,9,A,@00000000000002,2*5D").unwrap();
        assert_eq!(ais.f(4), "@00000000000002");
        assert!(ais.checksum_ok);
        assert!(parse("hello").is_none());
        assert!(parse("$A").is_none());
        let prop = parse("$PGRME,1,M*00").unwrap();
        assert_eq!(prop.talker, "P");
    }

    #[test]
    fn coordinates() {
        assert!((coord("3459.978941", "S").unwrap() + 34.999_649_016_7).abs() < 1e-9);
        assert_eq!(coord("00000.0", "E"), Some(0.0));
        assert!(coord("1", "N").is_none());
    }
}
