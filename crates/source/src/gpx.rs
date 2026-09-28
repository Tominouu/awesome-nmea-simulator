//! GPX 1.0 / 1.1 : `trk/trkseg/trkpt`, `rte/rtept`, `wpt` (`docs/10` §1).

use chrono::DateTime;
use nmeasim_core::geo::LatLon;
use roxmltree::{Document, Node};

use crate::track::{NamedPoint, SourceError, Track, TrackFile, TrackPoint};

fn child<'a>(n: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn text(n: Node<'_, '_>, name: &str) -> Option<String> {
    child(n, name)
        .and_then(|c| c.text())
        .map(|t| t.trim().to_string())
}

/// Recherche en profondeur (extensions GPX : `<speed>`, `<course>`).
fn deep(n: Node<'_, '_>, name: &str) -> Option<f64> {
    n.descendants()
        .find(|c| c.is_element() && c.tag_name().name() == name)
        .and_then(|c| c.text())
        .and_then(|t| t.trim().parse().ok())
}

/// Horodatage ISO 8601 vers ms epoch.
pub fn parse_time(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|d| d.timestamp_millis())
}

fn point(n: Node<'_, '_>) -> Option<TrackPoint> {
    let lat: f64 = n.attribute("lat")?.trim().parse().ok()?;
    let lon: f64 = n.attribute("lon")?.trim().parse().ok()?;
    let position = LatLon::new(lat, lon);
    if !position.is_valid() {
        return None;
    }
    Some(TrackPoint {
        position,
        time_ms: text(n, "time").and_then(|t| parse_time(&t)),
        elevation: text(n, "ele").and_then(|t| t.parse().ok()),
        course_deg: deep(n, "course"),
        speed_mps: deep(n, "speed"),
    })
}

/// Lit un document GPX.
pub fn parse(xml: &str) -> Result<TrackFile, SourceError> {
    let doc = Document::parse(xml).map_err(|e| SourceError(format!("XML invalide : {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "gpx" {
        return Err(SourceError(
            "le fichier ne contient pas de données GPX".into(),
        ));
    }
    let mut out = TrackFile::default();
    for (k, trk) in root
        .children()
        .filter(|c| c.tag_name().name() == "trk")
        .enumerate()
    {
        let name = text(trk, "name").unwrap_or_else(|| format!("Track #{}", k + 1));
        let points: Vec<TrackPoint> = trk
            .children()
            .filter(|c| c.tag_name().name() == "trkseg")
            .flat_map(|seg| {
                seg.children()
                    .filter(|c| c.tag_name().name() == "trkpt")
                    .filter_map(point)
                    .collect::<Vec<_>>()
            })
            .collect();
        if !points.is_empty() {
            out.tracks.push(Track { name, points });
        }
    }
    for (k, rte) in root
        .children()
        .filter(|c| c.tag_name().name() == "rte")
        .enumerate()
    {
        let name = text(rte, "name").unwrap_or_else(|| format!("Route #{}", k + 1));
        let points: Vec<TrackPoint> = rte
            .children()
            .filter(|c| c.tag_name().name() == "rtept")
            .filter_map(point)
            .collect();
        if !points.is_empty() {
            out.tracks.push(Track { name, points });
        }
    }
    for (k, w) in root
        .children()
        .filter(|c| c.tag_name().name() == "wpt")
        .enumerate()
    {
        if let Some(p) = point(w) {
            out.waypoints.push(NamedPoint {
                name: text(w, "name").unwrap_or_else(|| format!("WPT{:03}", k + 1)),
                position: p.position,
            });
        }
    }
    if out.tracks.is_empty() && out.waypoints.is_empty() {
        return Err(SourceError("le fichier ne contient aucune trace".into()));
    }
    Ok(out)
}

/// Exporte une trace en GPX 1.1.
pub fn export(track: &Track) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<gpx version=\"1.1\" creator=\"nmeasim-rs\" xmlns=\"http://www.topografix.com/GPX/1/1\">\r\n",
    );
    s.push_str(&format!(
        "\t<trk>\r\n\t\t<name>{}</name>\r\n\t\t<trkseg>\r\n",
        xml_escape(&track.name)
    ));
    for p in &track.points {
        s.push_str(&format!(
            "\t\t\t<trkpt lat=\"{:.7}\" lon=\"{:.7}\">",
            p.position.lat, p.position.lon
        ));
        if let Some(e) = p.elevation {
            s.push_str(&format!("<ele>{e}</ele>"));
        }
        if let Some(t) = p.time_ms.and_then(chrono::DateTime::from_timestamp_millis) {
            s.push_str(&format!(
                "<time>{}</time>",
                t.format("%Y-%m-%dT%H:%M:%S%.3fZ")
            ));
        }
        s.push_str("</trkpt>\r\n");
    }
    s.push_str("\t\t</trkseg>\r\n\t</trk>\r\n</gpx>\r\n");
    s
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
  <wpt lat="-35.1" lon="138.4"><name>BUOY</name></wpt>
  <trk><name>T1</name>
    <trkseg>
      <trkpt lat="-35.0" lon="138.5"><ele>2</ele><time>2026-09-28T10:00:00Z</time></trkpt>
      <trkpt lat="-35.0" lon="138.501"><time>2026-09-28T10:00:10Z</time><extensions><speed>4.5</speed><course>91</course></extensions></trkpt>
    </trkseg>
    <trkseg><trkpt lat="-35.0" lon="138.502"/></trkseg>
  </trk>
  <rte><rtept lat="-35.2" lon="138.6"/><rtept lat="-35.3" lon="138.7"/></rte>
</gpx>"#;

    #[test]
    fn tracks_segments_routes_waypoints() {
        let f = parse(SAMPLE).unwrap();
        assert_eq!(f.tracks.len(), 2);
        assert_eq!(f.tracks[0].name, "T1");
        assert_eq!(f.tracks[0].points.len(), 3, "segments concaténés");
        assert_eq!(f.tracks[0].points[0].elevation, Some(2.0));
        assert_eq!(f.tracks[0].points[1].speed_mps, Some(4.5));
        assert_eq!(f.tracks[0].points[1].course_deg, Some(91.0));
        assert_eq!(f.tracks[0].points[1].time_ms, Some(1_790_589_610_000));
        assert_eq!(f.tracks[1].name, "Route #1");
        assert_eq!(f.waypoints[0].name, "BUOY");
    }

    #[test]
    fn errors_and_export_roundtrip() {
        assert!(parse("<kml/>").is_err());
        assert!(parse("pas du xml").is_err());
        assert!(parse("<gpx></gpx>").is_err());
        let f = parse(SAMPLE).unwrap();
        let back = parse(&export(&f.tracks[0])).unwrap();
        assert_eq!(back.tracks[0].points.len(), 3);
        assert_eq!(back.tracks[0].points[1].time_ms, Some(1_790_589_610_000));
    }
}
