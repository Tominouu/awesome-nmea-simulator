//! Tests des sources sur les fixtures réelles (P1, P2).

use std::path::PathBuf;

use nmeasim_source::journal::{self, JournalWriter};
use nmeasim_source::kml::{self, KmlMode};
use nmeasim_source::replay::{ReplayCommand, ReplayPlayer};
use nmeasim_source::{PlayMode, TrackPlayer};
use serde_json::json;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn read(p: &str) -> String {
    std::fs::read_to_string(fixtures().join(p)).unwrap()
}

#[test]
fn kml_legacy_mode_matches_observed_legacy() {
    // legacy-p2 : gx:Track importé, LineString rejeté, Folder imbriqué ignoré.
    let g = kml::parse(&read("legacy-p2/kml/gxtrack.kml"), KmlMode::Legacy).unwrap();
    assert_eq!(g.tracks.len(), 1);
    assert_eq!(g.tracks[0].name, "GXTRACK-A");
    assert_eq!(g.tracks[0].points.len(), 5);
    assert_eq!(g.tracks[0].points[1].time_ms, Some(1_790_589_610_000));
    assert!(kml::parse(&read("legacy-p2/kml/linestring.kml"), KmlMode::Legacy).is_err());
    let f = kml::parse(&read("legacy-p2/kml/folder.kml"), KmlMode::Legacy).unwrap();
    let names: Vec<_> = f.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["FOLDER-TRACK-1"]);
}

#[test]
fn kml_modern_mode_reads_everything() {
    let l = kml::parse(&read("legacy-p2/kml/linestring.kml"), KmlMode::Modern).unwrap();
    assert_eq!(l.tracks[0].points.len(), 5);
    let f = kml::parse(&read("legacy-p2/kml/folder.kml"), KmlMode::Modern).unwrap();
    let names: Vec<_> = f.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        ["FOLDER-TRACK-1", "FOLDER-NESTED-TRACK", "FOLDER-LINESTRING"]
    );
    let point = r#"<kml><Document><Placemark><name>BUOY</name><Point><coordinates>138.5,-35.0,0</coordinates></Point></Placemark></Document></kml>"#;
    let p = kml::parse(point, KmlMode::Modern).unwrap();
    assert_eq!(p.waypoints[0].name, "BUOY");
    assert!(kml::parse("<gpx/>", KmlMode::Modern).is_err());
}

#[test]
fn gx_track_player_speed_matches_legacy_capture() {
    // legacy-p2/cap-kml-gxtrack.log : 17,7 kn, cap 90.0, heure = <when>.
    let g = kml::parse(&read("legacy-p2/kml/gxtrack.kml"), KmlMode::Modern).unwrap();
    let mut p = TrackPlayer::new(g.tracks[0].clone(), PlayMode::FixedRate, false).unwrap();
    let pts: Vec<_> = std::iter::from_fn(|| p.next(1000)).collect();
    assert_eq!(pts.len(), 5);
    assert!((pts[1].sog_kn.unwrap() - 17.7).abs() < 0.05);
    assert!((pts[1].cog_deg.unwrap() - 90.0).abs() < 0.01);
}

#[test]
fn legacy_journal_reads_1801_blocks() {
    let j = journal::parse(&read("legacy/sim.nmeasim")).unwrap();
    assert_eq!(j.version, "1.6.1");
    assert!(j.meta.is_none());
    assert_eq!(j.blocks.len(), 1801);
    assert!(j.blocks.iter().all(|b| b.matches("\r\n").count() == 24));
    assert_eq!(j.lines(0).len(), 24);
    assert!(j.lines(0)[0].starts_with("$GPRMC,083444.986"));
}

/// Réimplémentation fidèle du chargeur legacy (`mods_b/3309.js:1251`, `:930`).
fn legacy_loader(data: &str) -> Option<Vec<String>> {
    let mut seg: Vec<String> = data.split('~').map(str::to_string).collect();
    if !seg[0].starts_with("nmeasim") || seg.len() < 2 {
        return None;
    }
    seg.pop();
    seg.remove(0);
    Some(seg)
}

#[test]
fn meta_journal_roundtrip_and_legacy_compat() {
    let dir = std::env::temp_dir().join(format!("nmeasim-journal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let meta = json!({"format": 1, "intervalMs": 1000, "seed": 1234, "note": "a~b"});
    let w = JournalWriter::create(&dir.join("test"), "2.0.0", Some(&meta)).unwrap();
    assert!(w.path().to_string_lossy().ends_with("test.nmeasim"));
    let path = w.path().to_path_buf();
    for i in 0..5 {
        w.push(&[format!("$GPRMC,{i}"), "$IIHDT,1.0,T".into()]);
    }
    let (written, dropped, err) = w.close();
    assert_eq!((written, dropped, err), (5, 0, None));
    let data = std::fs::read_to_string(&path).unwrap();
    assert!(data.ends_with('~'), "le chargeur legacy perdrait un bloc");
    assert!(data.contains("\\u007e"), "tilde échappé");
    let j = journal::parse(&data).unwrap();
    assert_eq!(j.meta.as_ref().unwrap()["note"], "a~b");
    assert_eq!(j.blocks.len(), 5);
    assert_eq!(j.lines(2), ["$GPRMC,2", "$IIHDT,1.0,T"]);
    let legacy = legacy_loader(&data).unwrap();
    assert_eq!(legacy.len(), 5);
    assert!(legacy.iter().all(|b| !b.contains("meta:")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn journal_errors_and_warnings() {
    assert!(journal::parse("hello~x~").is_err());
    assert!(journal::parse("nmeasim,1\r\n~").is_err());
    let e = journal::parse("nmeasim,2\r\nmeta:{oops\r\n~$A\r\n~").unwrap_err();
    assert!(e.0.contains("ligne 2"), "{}", e.0);
    let j = journal::parse("nmeasim,9\r\nmeta:{\"format\":99}\r\n~$A\r\n~").unwrap();
    assert!(j.warning.is_some());
    // Fichier sans `~` final : le dernier bloc est conservé par la cible.
    assert_eq!(
        journal::parse("nmeasim,1\r\n~$A\r\n~$B\r\n")
            .unwrap()
            .blocks
            .len(),
        2
    );
}

#[test]
fn replay_semantics_and_step_fix() {
    let j = journal::parse(&read("legacy/sim.nmeasim")).unwrap();
    let mut r = ReplayPlayer::new(j);
    assert!(r.tick(5000.0, 1000.0).is_empty(), "en pause au chargement");
    r.command(ReplayCommand::Play);
    let first: Vec<usize> = (0..9).flat_map(|_| r.tick(1000.0, 1000.0)).collect();
    assert_eq!(first, (0..9).collect::<Vec<_>>());
    r.command(ReplayCommand::Pause);
    // legacy-p2/cap-replay.log : le legacy sautait le bloc 10 (index 9).
    assert_eq!(r.command(ReplayCommand::StepForward), Some(9));
    assert_eq!(r.command(ReplayCommand::StepForward), Some(10));
    assert_eq!(r.command(ReplayCommand::StepBack), Some(9));
    r.command(ReplayCommand::Speed { factor: 2.0 });
    r.command(ReplayCommand::Play);
    assert_eq!(r.tick(1000.0, 1000.0), vec![10, 11]);
    r.command(ReplayCommand::Seek { index: 1799 });
    let end: Vec<usize> = (0..5).flat_map(|_| r.tick(1000.0, 1000.0)).collect();
    assert_eq!(end, vec![1799, 1800]);
    assert!(r.status().ended && !r.status().playing);
    r.command(ReplayCommand::Repeat { on: true });
    r.command(ReplayCommand::Play);
    assert_eq!(r.tick(500.0, 1000.0), vec![0]);
}

#[cfg(target_os = "linux")]
#[test]
fn disk_full_never_blocks_the_writer() {
    // /dev/full : toute écriture échoue avec ENOSPC. L'écrivain se désactive,
    // compte, et `push` ne bloque jamais.
    let w = match JournalWriter::create(std::path::Path::new("/dev/full"), "2.0.0", None) {
        Ok(w) => w,
        Err(_) => return, // l'en-tête bufferisé peut déjà échouer : aussi un échec explicite
    };
    let t = std::time::Instant::now();
    for i in 0..20_000 {
        w.push(&[format!("$GPRMC,{i}")]);
    }
    assert!(
        t.elapsed() < std::time::Duration::from_secs(1),
        "push non bloquant"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !w.failed() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(w.failed(), "erreur disque détectée");
    let (_, _, err) = w.close();
    assert!(err.is_some());
}
