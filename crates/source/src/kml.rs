//! KML (`docs/10` §2). Deux modes :
//!
//! - `Legacy` : structures lues par le legacy 1.6.1 (vérifié en P2) —
//!   `Placemark` sous `Document` et sous le premier niveau de `Folder`,
//!   géométries `Track`, `MultiTrack/Track`, `MultiGeometry/Track` ;
//! - `Modern` : récursion complète, plus `LineString`, `MultiGeometry/LineString`
//!   et `Point` (waypoints).

use nmeasim_core::geo::LatLon;
use roxmltree::{Document, Node};

use crate::gpx::parse_time;
use crate::track::{NamedPoint, SourceError, Track, TrackFile, TrackPoint};

/// Mode de lecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KmlMode {
    /// Limites du legacy.
    Legacy,
    /// Support complet.
    Modern,
}

fn is(n: &Node<'_, '_>, name: &str) -> bool {
    n.is_element() && n.tag_name().name() == name
}

fn children<'a>(n: Node<'a, 'a>, name: &'a str) -> impl Iterator<Item = Node<'a, 'a>> + 'a {
    n.children().filter(move |c| is(c, name))
}

fn text(n: Node<'_, '_>, name: &str) -> Option<String> {
    children(n, name)
        .next()
        .and_then(|c| c.text())
        .map(|t| t.trim().to_string())
}

/// `gx:Track` / `Track` : `<when>` et `<gx:coord>lon lat alt</gx:coord>`.
fn gx_track(t: Node<'_, '_>) -> Vec<TrackPoint> {
    let whens: Vec<Option<i64>> = children(t, "when")
        .map(|w| w.text().and_then(parse_time))
        .collect();
    children(t, "coord")
        .enumerate()
        .filter_map(|(i, c)| {
            let v: Vec<f64> = c
                .text()?
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            if v.len() < 2 {
                return None;
            }
            let position = LatLon::new(v[1], v[0]);
            position.is_valid().then(|| TrackPoint {
                position,
                time_ms: whens.get(i).copied().flatten(),
                elevation: v.get(2).copied(),
                course_deg: None,
                speed_mps: None,
            })
        })
        .collect()
}

/// `LineString/coordinates` : `lon,lat[,alt]` séparés par des blancs.
fn line_string(l: Node<'_, '_>) -> Vec<TrackPoint> {
    let Some(coords) = text(l, "coordinates") else {
        return vec![];
    };
    coords
        .split_whitespace()
        .filter_map(|tuple| {
            let v: Vec<f64> = tuple
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            if v.len() < 2 {
                return None;
            }
            let position = LatLon::new(v[1], v[0]);
            position.is_valid().then(|| TrackPoint {
                elevation: v.get(2).copied(),
                ..TrackPoint::at(position)
            })
        })
        .collect()
}

fn placemark(p: Node<'_, '_>, index: usize, mode: KmlMode, out: &mut TrackFile) {
    let name = text(p, "name").unwrap_or_else(|| format!("KML Track #{index}"));
    let mut pts = Vec::new();
    for mt in children(p, "MultiTrack") {
        for t in children(mt, "Track") {
            pts.extend(gx_track(t));
        }
    }
    for mg in children(p, "MultiGeometry") {
        for t in children(mg, "Track") {
            pts.extend(gx_track(t));
        }
        if mode == KmlMode::Modern {
            for l in children(mg, "LineString") {
                pts.extend(line_string(l));
            }
        }
    }
    for t in children(p, "Track") {
        pts.extend(gx_track(t));
    }
    if mode == KmlMode::Modern {
        for l in children(p, "LineString") {
            pts.extend(line_string(l));
        }
        if pts.is_empty() {
            if let Some(pt) = children(p, "Point").next() {
                if let Some(first) = line_string(pt).first() {
                    out.waypoints.push(NamedPoint {
                        name: name.clone(),
                        position: first.position,
                    });
                }
            }
        }
    }
    if !pts.is_empty() {
        out.tracks.push(Track { name, points: pts });
    }
}

fn placemarks(container: Node<'_, '_>, mode: KmlMode, out: &mut TrackFile) {
    for (i, p) in children(container, "Placemark").enumerate() {
        placemark(p, i + 1, mode, out);
    }
}

fn recurse(container: Node<'_, '_>, out: &mut TrackFile) {
    placemarks(container, KmlMode::Modern, out);
    for c in container
        .children()
        .filter(|c| is(c, "Folder") || is(c, "Document"))
    {
        recurse(c, out);
    }
}

/// Lit un document KML.
pub fn parse(xml: &str, mode: KmlMode) -> Result<TrackFile, SourceError> {
    let doc = Document::parse(xml).map_err(|e| SourceError(format!("XML invalide : {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "kml" {
        return Err(SourceError(
            "le fichier ne contient pas de données KML".into(),
        ));
    }
    let mut out = TrackFile::default();
    match mode {
        KmlMode::Legacy => {
            // `mods_b/3309.js:768` : Document → Placemark ; puis Folder de
            // premier niveau sous Document (ou sous kml sans Document).
            let base = children(root, "Document").next();
            if let Some(d) = base {
                placemarks(d, mode, &mut out);
            }
            let rt = base.unwrap_or(root);
            for f in children(rt, "Folder") {
                placemarks(f, mode, &mut out);
            }
        }
        KmlMode::Modern => recurse(root, &mut out),
    }
    if out.tracks.is_empty() && out.waypoints.is_empty() {
        return Err(SourceError("le fichier ne contient aucune trace".into()));
    }
    Ok(out)
}
