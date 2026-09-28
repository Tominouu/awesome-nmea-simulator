//! Tests de bout en bout : moteur réel, sockets réels, fichiers réels.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use nmeasim_app::config::{self, AppConfig, OutputFormat, TransportConfig};
use nmeasim_app::http::ApiServer;
use nmeasim_app::runtime::{AppEvent, Producer};
use nmeasim_app::scenario::{self, Scenario};
use nmeasim_app::{APP_VERSION, Runtime};
use nmeasim_encode::parse;
use nmeasim_sim::command::{Command, EngineSel, RejectReason, Source};
use nmeasim_source::replay::ReplayCommand;
use nmeasim_transport::TransportSpec;

fn free_port() -> u16 {
    // Hors de la plage éphémère (32768+) : le système ne réattribue pas ce
    // port à un autre test qui lie le port 0 en parallèle.
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let base = 20_000 + (std::process::id() % 500) as u16 * 16;
    loop {
        let p = base + NEXT.fetch_add(1, Ordering::Relaxed) % 8000;
        if TcpListener::bind(("127.0.0.1", p)).is_ok() && TcpListener::bind(("0.0.0.0", p)).is_ok()
        {
            return p;
        }
    }
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nmeasim-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name)
}

fn base(tcp: u16) -> AppConfig {
    let mut c = AppConfig::default();
    c.simulation.seed = Some(42);
    c.output.interval_ms = 200;
    c.transports = vec![TransportConfig {
        id: "tcp".into(),
        enabled: true,
        format: OutputFormat::Nmea,
        spec: TransportSpec::TcpServer {
            bind: "127.0.0.1".into(),
            port: tcp,
        },
    }];
    c
}

fn connect(port: u16) -> BufReader<TcpStream> {
    let t = Instant::now();
    loop {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            return BufReader::new(s);
        }
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "port {port} jamais ouvert"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait(ms: u64, mut f: impl FnMut() -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn nmea_over_tcp_end_to_end() {
    let port = free_port();
    let rt = Runtime::spawn(base(port)).unwrap();
    // Transports ouverts seulement au démarrage, comme le legacy.
    assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    rt.submit(Command::Start, Source::Ui).unwrap();
    let mut r = connect(port);
    let mut lines = vec![];
    while lines.len() < 60 {
        let mut s = String::new();
        r.read_line(&mut s).unwrap();
        assert!(s.ends_with("\r\n"), "CRLF : {s:?}");
        lines.push(s.trim_end().to_string());
    }
    let parsed: Vec<_> = lines
        .iter()
        .map(|l| parse::parse(l).expect("phrase"))
        .collect();
    assert!(parsed.iter().all(|p| p.checksum_ok), "checksums");
    let ids: std::collections::BTreeSet<String> =
        parsed.iter().map(|p| p.formatter.clone()).collect();
    for f in [
        "RMC", "VHW", "VTG", "HDT", "HDM", "GLL", "GGA", "GSA", "GSV", "ZDA", "VBW", "VDO", "VDM",
        "MWD", "MWV", "MTW", "DPT", "DBT", "RPM", "APB", "RMB",
    ] {
        assert!(ids.contains(f), "phrase {f} absente");
    }
    let rmc = parsed.iter().find(|p| p.formatter == "RMC").unwrap();
    assert_eq!(rmc.f(11), "A", "mode RMC (correction)");
    let status = rt.published().transports[0].clone();
    assert_eq!(status.clients, 1);
    assert!(status.messages > 0);
    rt.submit(Command::Stop, Source::Ui).unwrap();
    assert!(wait(2000, || TcpStream::connect(("127.0.0.1", port)).is_err()));
}

#[test]
fn signalk_websocket_hello_and_deltas() {
    let port = free_port();
    let mut c = base(free_port());
    c.transports.push(TransportConfig {
        id: "sk".into(),
        enabled: true,
        format: OutputFormat::Signalk,
        spec: TransportSpec::WebsocketServer {
            bind: "127.0.0.1".into(),
            port,
        },
    });
    let rt = Runtime::spawn(c).unwrap();
    rt.submit(Command::Start, Source::Ui).unwrap();
    let (mut ws, _) = loop {
        if let Ok(x) = tungstenite::connect(format!("ws://127.0.0.1:{port}")) {
            break x;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    }
    let hello: serde_json::Value =
        serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(hello["version"], APP_VERSION);
    assert_eq!(hello["name"], "nmea-simulator");
    let delta: serde_json::Value =
        serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
    let vals = delta["updates"][0]["values"].as_array().unwrap();
    assert!(vals.iter().any(|v| v["path"] == "navigation.position"));
    assert!(vals.iter().any(|v| v["path"] == "navigation.rateOfTurn"));
    assert_eq!(delta["updates"][0]["$source"], "nmea-simulator");
}

#[test]
fn commands_events_and_rejections() {
    let rt = Runtime::spawn(base(free_port())).unwrap();
    let events = rt.subscribe();
    rt.submit(Command::Start, Source::Ui).unwrap();
    rt.submit(
        Command::SetThrottle {
            engine: EngineSel::All,
            value: 1.0,
        },
        Source::Gamepad(1),
    )
    .unwrap();
    assert_eq!(
        rt.submit(Command::SetRudder { deg: 90.0 }, Source::Keyboard),
        Err(RejectReason::OutOfRange {
            min: -40.0,
            max: 40.0
        })
    );
    let v0 = rt.snapshot().vessel.stw;
    assert!(
        wait(3000, || rt.snapshot().vessel.stw > v0 + 0.3),
        "accélération"
    );
    let evs: Vec<AppEvent> = events.try_iter().collect();
    assert!(evs.iter().any(|e| matches!(
        e,
        AppEvent::CommandAccepted {
            source: Source::Gamepad(1),
            ..
        }
    )));
    assert!(evs.iter().any(|e| matches!(
        e,
        AppEvent::CommandRejected {
            source: Source::Keyboard,
            ..
        }
    )));
    assert!(
        evs.iter()
            .any(|e| matches!(e, AppEvent::TransportState { .. }))
    );
}

#[test]
fn recording_writes_meta_journal() {
    let rt = Runtime::spawn(base(free_port())).unwrap();
    rt.submit(Command::Start, Source::Ui).unwrap();
    let path = rt.start_recording(&tmp("rec")).unwrap();
    assert!(path.ends_with("rec.nmeasim"));
    assert!(wait(3000, || rt
        .published()
        .recording
        .as_ref()
        .is_some_and(|r| r.written >= 5)));
    rt.stop_recording();
    assert!(wait(2000, || rt.published().recording.is_none()));
    let j = nmeasim_source::journal::read(std::path::Path::new(&path)).unwrap();
    assert_eq!(j.version, APP_VERSION);
    assert_eq!(j.meta.as_ref().unwrap()["seed"], 42);
    assert_eq!(j.meta.as_ref().unwrap()["intervalMs"], 200);
    assert!(j.blocks.len() >= 5);
    assert!(j.lines(0)[0].starts_with("$GPRMC"));
}

#[test]
fn replay_legacy_journal_over_tcp() {
    let port = free_port();
    let mut c = base(port);
    c.output.interval_ms = 100;
    let rt = Runtime::spawn(c).unwrap();
    let n = rt
        .load_journal(&fixtures().join("legacy/sim.nmeasim"))
        .unwrap();
    assert_eq!(n, 1801);
    assert_eq!(rt.published().producer, Producer::Replay);
    // Jeton unique : la simulation live ne peut pas démarrer.
    assert_eq!(
        rt.submit(Command::Start, Source::Ui),
        Err(RejectReason::ProducerLocked)
    );
    let mut r = connect(port);
    rt.replay(ReplayCommand::Play);
    let mut rmc = 0;
    let mut lines = 0;
    while rmc < 3 {
        let mut s = String::new();
        r.read_line(&mut s).unwrap();
        lines += 1;
        if s.starts_with("$GPRMC,083444.986")
            || s.starts_with("$GPRMC,083445")
            || s.starts_with("$GPRMC,083446")
        {
            rmc += 1;
        }
    }
    assert!(lines >= 48);
    // Le rejeu pilote la position affichée.
    assert!(wait(2000, || (rt.snapshot().vessel.position.lat + 34.998)
        .abs()
        < 0.01));
    rt.close_replay();
    assert!(wait(2000, || rt.published().producer == Producer::Live));
}

#[test]
fn track_follow_and_end() {
    let rt = Runtime::spawn(base(free_port())).unwrap();
    let events = rt.subscribe();
    let name = rt
        .load_track(&fixtures().join("legacy-p2/kml/gxtrack.kml"), 0)
        .unwrap();
    assert_eq!(name, "GXTRACK-A");
    rt.submit(Command::Start, Source::Ui).unwrap();
    assert!(wait(3000, || rt.published().producer == Producer::Track));
    // Commandes de navigation refusées pendant le suivi.
    assert!(wait(2000, || rt
        .submit(Command::SetRudder { deg: 2.0 }, Source::Ui)
        == Err(RejectReason::ProducerLocked)));
    assert!(
        wait(5000, || !rt.snapshot().running),
        "arrêt en fin de trace"
    );
    let lon = rt.snapshot().vessel.position.lon;
    assert!((lon - 138.504).abs() < 1e-6, "dernier point émis : {lon}");
    assert!(events.try_iter().any(|e| e == AppEvent::TrackEnded));
}

#[test]
fn http_api_commands_and_state() {
    let rt = Runtime::spawn(base(free_port())).unwrap();
    let port = free_port();
    let cfg = nmeasim_app::config::ApiConfig {
        enabled: true,
        bind: "127.0.0.1".into(),
        port,
        token: Some("s3cret".into()),
    };
    let _api = ApiServer::start(&cfg, rt.controller(), rt.shared()).unwrap();
    let http = |method: &str, path: &str, body: &str, token: bool| -> (u16, String) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let auth = if token {
            "Authorization: Bearer s3cret\r\n"
        } else {
            ""
        };
        write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        let code: u16 = resp[9..12].parse().unwrap();
        (
            code,
            resp.split("\r\n\r\n").nth(1).unwrap_or("").to_string(),
        )
    };
    assert_eq!(http("GET", "/api/v1/state", "", false).0, 401);
    assert_eq!(
        http("POST", "/api/v1/commands", r#"{"kind":"sim.start"}"#, true).0,
        200
    );
    let (code, body) = http(
        "POST",
        "/api/v1/commands",
        r#"{"kind":"helm.rudder.set","deg":99}"#,
        true,
    );
    assert_eq!(code, 422);
    assert!(body.contains("outOfRange"), "{body}");
    assert_eq!(
        http("POST", "/api/v1/commands", r#"{"kind":"nope"}"#, true).0,
        400
    );
    assert_eq!(
        http(
            "POST",
            "/api/v1/commands",
            r#"{"kind":"route.destination.set","position":{"lat":-35.1,"lon":138.6}}"#,
            true
        )
        .0,
        200
    );
    let (code, body) = http("GET", "/api/v1/state", "", true);
    assert_eq!(code, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["running"], true);
    assert_eq!(v["route"]["waypoints"][0]["name"], "dest");
    assert!(
        http("GET", "/api/v1/paths", "", true)
            .1
            .contains("env.water.depth")
    );
    assert_eq!(http("GET", "/api/v1/nope", "", true).0, 404);
}

#[test]
fn legacy_config_migration_and_persistence() {
    // Configuration réellement injectée dans le legacy pendant P2.
    let text = std::fs::read_to_string(fixtures().join("legacy-p2/cfg-serial.json")).unwrap();
    let loaded = config::from_json(&text).unwrap();
    assert!(loaded.migrated);
    let t = &loaded.config.transports[0];
    assert!(
        matches!(&t.spec, TransportSpec::Serial { port, baud_rate: 4800, .. } if port == "/dev/pts/2")
    );
    let tcp = config::from_json(
        &std::fs::read_to_string(fixtures().join("legacy-p2/cfg-zda.json")).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        tcp.config.transports[0].spec,
        TransportSpec::TcpServer { port: 10110, .. }
    ));
    assert_eq!(tcp.config.simulation.initial.position.lat, -35.0);
    // Type en chaîne : erreur explicite (le legacy ne démarrait rien, en silence).
    let bad = text.replace("\"type\":3", "\"type\":\"udp\"");
    let e = config::from_json(&bad).unwrap_err();
    assert_eq!(e.field, "server.type");
    // Aucune clé perdue.
    let mut v: serde_json::Value = serde_json::to_value(AppConfig::default()).unwrap();
    v["futureFeature"] = serde_json::json!({"x": 1});
    let l = config::from_json(&v.to_string()).unwrap();
    assert_eq!(l.config.extra["futureFeature"]["x"], 1);
    assert!(!l.warnings.is_empty());
    let path = tmp("cfg.json");
    config::save(&path, &l.config).unwrap();
    let back = config::load(&path).unwrap();
    assert_eq!(back.config, l.config);
    // Invalide : champ nommé.
    let mut c = AppConfig::default();
    c.output.interval_ms = 0;
    assert_eq!(c.validate().unwrap_err().field, "output.intervalMs");
    c = AppConfig::default();
    c.transports[1].id = c.transports[0].id.clone();
    assert!(c.validate().unwrap_err().reason.contains("double"));
}

#[test]
fn scenario_capture_apply_roundtrip() {
    let rt = Runtime::spawn(base(free_port())).unwrap();
    rt.submit(Command::Start, Source::Ui).unwrap();
    rt.submit(
        Command::SetRoute {
            points: vec![nmeasim_core::geo::LatLon::new(-35.01, 138.51)],
        },
        Source::Ui,
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let snap = rt.snapshot();
    let cfg = rt.published().config.clone();
    let sc = Scenario::capture("Test", &cfg, &snap);
    let path = tmp("scenario.json");
    scenario::save(&path, &sc).unwrap();
    let back = scenario::load(&path).unwrap();
    assert_eq!(back, sc);
    assert_eq!(back.simulation.seed, Some(42));
    assert_eq!(back.route.len(), 1);
    let applied = back.apply_to(&AppConfig::default());
    assert_eq!(applied.simulation.initial.position, snap.vessel.position);
    rt.apply_config(applied).unwrap();
    for c in back.commands() {
        rt.submit(c, Source::Script).unwrap();
    }
    assert_eq!(rt.snapshot().route.waypoints.len(), 1);
}

#[test]
fn bundled_examples_load_and_run() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for f in [
        "port-approach.json",
        "legacy-bench.json",
        "manoeuvre-gamepad.json",
    ] {
        let sc =
            scenario::load(&root.join("scenarios").join(f)).unwrap_or_else(|e| panic!("{f} : {e}"));
        let mut cfg = sc.apply_to(&base(free_port()));
        // Ports propres au test.
        cfg.transports = base(free_port()).transports;
        let rt = Runtime::spawn(cfg).unwrap();
        for c in sc.commands() {
            rt.submit(c, Source::Script).unwrap();
        }
        assert!(wait(3000, || rt.snapshot().elapsed_ms > 500), "{f} tourne");
    }
    let t = nmeasim_source::read_track_file(
        &root.join("tracks/gulf-st-vincent.gpx"),
        nmeasim_source::kml::KmlMode::Modern,
    )
    .unwrap();
    assert_eq!(t.tracks[0].points.len(), 6);
}

#[test]
fn legacy_profile_emits_legacy_sentence_sequence_over_tcp() {
    // Séquence d'un bloc du golden file legacy/cap-tcp.log.
    let golden: String = std::fs::read_to_string(fixtures().join("legacy/cap-tcp.log"))
        .unwrap()
        .lines()
        .filter_map(|l| {
            l.find('"')
                .map(|i| serde_json::from_str::<String>(&l[i..]).unwrap())
        })
        .collect();
    let seq = |text: &str| -> Vec<String> {
        let lines: Vec<&str> = text.split("\r\n").filter(|l| !l.is_empty()).collect();
        let start = lines.iter().position(|l| l.starts_with("$GPRMC")).unwrap();
        let end = start
            + 1
            + lines[start + 1..]
                .iter()
                .position(|l| l.starts_with("$GPRMC"))
                .unwrap();
        lines[start..end]
            .iter()
            .map(|l| l.split(',').next().unwrap().to_string())
            .collect()
    };
    let expected = seq(&golden);
    assert_eq!(expected.len(), 24);
    let port = free_port();
    let mut c = base(port);
    c.simulation.compatibility.profile = nmeasim_sim::compat::Profile::Legacy;
    let rt = Runtime::spawn(c).unwrap();
    rt.submit(Command::Start, Source::Ui).unwrap();
    let mut r = connect(port);
    let mut text = String::new();
    while text.matches("$GPRMC").count() < 3 {
        let mut s = String::new();
        r.read_line(&mut s).unwrap();
        text.push_str(&s);
    }
    assert_eq!(seq(&text), expected, "séquence legacy");
}
