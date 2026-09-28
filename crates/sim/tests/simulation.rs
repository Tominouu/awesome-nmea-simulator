//! Tests de simulation (J1) : déterminisme, dynamique, commandes.

use nmeasim_core::geo::LatLon;
use nmeasim_core::units::{knots_to_mps, mps_to_knots, rad_to_deg};
use nmeasim_sim::command::{Command, EngineSel, RejectReason, Source};
use nmeasim_sim::compat::{CompatConfig, Profile};
use nmeasim_sim::state::AutopilotMode;
use nmeasim_sim::{SimConfig, Simulator};

const UI: Source = Source::Ui;

fn cfg(profile: Profile) -> SimConfig {
    SimConfig {
        seed: Some(1234),
        start_time_ms: Some(1_790_000_000_000),
        compatibility: CompatConfig {
            profile,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn quiet(mut c: SimConfig) -> SimConfig {
    for d in [
        &mut c.drift.speed,
        &mut c.drift.heading,
        &mut c.drift.hdop,
        &mut c.drift.vdop,
        &mut c.drift.pdop,
        &mut c.drift.wind_direction,
        &mut c.drift.wind_speed,
        &mut c.drift.depth,
        &mut c.drift.water_temperature,
        &mut c.drift.rpm,
        &mut c.drift.engine_temperature,
    ] {
        d.enabled = false;
    }
    c
}

fn run(sim: &mut Simulator, seconds: f64) {
    let steps = (seconds * 1000.0 / f64::from(sim.config().step_ms)).round() as usize;
    let dt = sim.config().step_ms;
    for i in 0..steps {
        sim.step(dt);
        if (i + 1) % (1000 / dt as usize) == 0 {
            sim.publish_tick();
        }
    }
}

fn started(c: SimConfig) -> Simulator {
    let mut s = Simulator::new(c).unwrap();
    s.submit(Command::Start, &UI).unwrap();
    s
}

fn script(sim: &mut Simulator) {
    run(sim, 5.0);
    sim.submit(
        Command::SetThrottle {
            engine: EngineSel::All,
            value: 0.8,
        },
        &UI,
    )
    .unwrap();
    run(sim, 20.0);
    sim.submit(Command::SetRudder { deg: 15.0 }, &UI).unwrap();
    run(sim, 30.0);
    sim.submit(Command::CenterRudder, &UI).unwrap();
    run(sim, 10.0);
}

#[test]
fn determinism_same_seed_same_state() {
    for p in [Profile::Modern, Profile::Legacy] {
        let mut a = started(cfg(p));
        let mut b = started(cfg(p));
        script(&mut a);
        script(&mut b);
        assert_eq!(a.state(), b.state(), "profil {p:?}");
        assert_eq!(
            serde_json::to_string(a.state()).unwrap(),
            serde_json::to_string(b.state()).unwrap()
        );
    }
}

#[test]
fn different_seed_different_noise() {
    let mut a = started(cfg(Profile::Modern));
    let mut c2 = cfg(Profile::Modern);
    c2.seed = Some(99);
    let mut b = started(c2);
    run(&mut a, 30.0);
    run(&mut b, 30.0);
    assert_ne!(a.state().wind, b.state().wind);
}

#[test]
fn legacy_turn_law_golden() {
    // legacy/cap-turn.log : barre 2 à 6,3 kn → 19,45 °/min (0,324 °/s).
    let mut c = quiet(cfg(Profile::Legacy));
    c.initial.speed_kn = 6.3;
    let mut s = started(c);
    s.submit(Command::SetRudder { deg: 2.0 }, &UI).unwrap();
    let h0 = rad_to_deg(s.state().vessel.heading);
    run(&mut s, 60.0);
    let turned = rad_to_deg(s.state().vessel.heading) - h0;
    assert!((turned - 19.45).abs() < 0.2, "{turned}");
    // Barre nulle : arrêt net.
    s.submit(Command::SetRudder { deg: 0.0 }, &UI).unwrap();
    let h1 = s.state().vessel.heading;
    run(&mut s, 10.0);
    assert_eq!(s.state().vessel.heading, h1);
}

#[test]
fn legacy_negative_rudder_uses_same_law() {
    // Correction : le legacy appliquait 600 m à toute barre négative.
    let mut c = quiet(cfg(Profile::Legacy));
    c.initial.speed_kn = 6.0;
    let mut a = started(c.clone());
    let mut b = started(c);
    a.submit(Command::SetRudder { deg: 25.0 }, &UI).unwrap();
    b.submit(Command::SetRudder { deg: -25.0 }, &UI).unwrap();
    run(&mut a, 10.0);
    run(&mut b, 10.0);
    let da = rad_to_deg(a.state().vessel.heading) - 15.0;
    let db = 15.0 - rad_to_deg(b.state().vessel.heading);
    assert!((da - db).abs() < 1e-6, "{da} {db}");
}

#[test]
fn modern_acceleration_inertia_and_steady_speed() {
    let mut c = quiet(cfg(Profile::Modern));
    c.initial.speed_kn = 0.0;
    let mut s = started(c);
    assert_eq!(s.state().vessel.stw, 0.0);
    s.submit(
        Command::SetEngineState {
            engine: EngineSel::All,
            running: true,
        },
        &UI,
    )
    .unwrap();
    s.submit(
        Command::SetThrottle {
            engine: EngineSel::All,
            value: 0.5,
        },
        &UI,
    )
    .unwrap();
    run(&mut s, 5.0);
    let v5 = mps_to_knots(s.state().vessel.stw);
    assert!(v5 > 0.5 && v5 < 5.0, "accélération progressive : {v5}");
    assert!(s.state().vessel.acceleration > 0.0);
    run(&mut s, 120.0);
    let vss = mps_to_knots(s.state().vessel.stw);
    assert!((vss - 6.0).abs() < 0.1, "équilibre = 0,5 × 12 kn : {vss}");
    // Point mort : décélération lente (inertie).
    s.submit(
        Command::SetThrottle {
            engine: EngineSel::All,
            value: 0.0,
        },
        &UI,
    )
    .unwrap();
    run(&mut s, 10.0);
    let v = mps_to_knots(s.state().vessel.stw);
    assert!(v > 3.0 && v < 6.0, "inertie : {v}");
    // Marche arrière.
    s.submit(
        Command::SetThrottle {
            engine: EngineSel::All,
            value: -1.0,
        },
        &UI,
    )
    .unwrap();
    run(&mut s, 200.0);
    let va = mps_to_knots(s.state().vessel.stw);
    assert!((va + 4.0).abs() < 0.2, "arrière : {va}");
    assert!(s.state().engines[0].pitch_percent < 0.0);
}

#[test]
fn modern_rudder_rate_and_turn_decay() {
    let mut s = started(quiet(cfg(Profile::Modern)));
    s.submit(Command::SetRudder { deg: 20.0 }, &UI).unwrap();
    s.step(1000);
    assert!((s.state().vessel.rudder_angle - 8.0).abs() < 1e-9);
    s.step(1000);
    s.step(1000);
    assert!((s.state().vessel.rudder_angle - 20.0).abs() < 1e-9);
    run(&mut s, 20.0);
    let r = s.state().vessel.rate_of_turn;
    assert!(r > 0.0);
    s.submit(Command::CenterRudder, &UI).unwrap();
    run(&mut s, 5.0);
    let r2 = s.state().vessel.rate_of_turn;
    assert!(r2 > 0.0 && r2 < r, "le virage continue en décroissant");
    run(&mut s, 60.0);
    let h = s.state().vessel.heading;
    run(&mut s, 60.0);
    assert!(
        (s.state().vessel.heading - h).abs() < 1e-6,
        "dérive nulle à barre zéro"
    );
}

#[test]
fn autopilot_heading_converges() {
    let mut s = started(quiet(cfg(Profile::Modern)));
    s.submit(Command::SetHeading { deg: 120.0 }, &UI).unwrap();
    assert_eq!(s.state().autopilot.mode, AutopilotMode::Heading);
    let mut max_over: f64 = 0.0;
    for _ in 0..240 {
        run(&mut s, 1.0);
        let h = rad_to_deg(s.state().vessel.heading);
        max_over = max_over.max(h - 120.0);
    }
    let h = rad_to_deg(s.state().vessel.heading);
    assert!((h - 120.0).abs() < 1.0, "{h}");
    assert!(max_over < 10.0, "dépassement {max_over}");
    // Barre manuelle : désengage.
    s.submit(Command::SetRudder { deg: 5.0 }, &UI).unwrap();
    assert_eq!(s.state().autopilot.mode, AutopilotMode::Off);
}

#[test]
fn route_following_and_advance() {
    let mut s = started(quiet(cfg(Profile::Modern)));
    let p = s.state().vessel.position;
    let a = p.destination(0.0, 800.0);
    let b = a.destination(std::f64::consts::FRAC_PI_2, 800.0);
    s.submit(Command::SetRoute { points: vec![a, b] }, &UI)
        .unwrap();
    s.submit(
        Command::SetAutopilot {
            mode: AutopilotMode::Route,
        },
        &UI,
    )
    .unwrap();
    run(&mut s, 900.0);
    let st = s.state();
    assert_eq!(st.route.active, 1);
    let g = st.route.geometry.as_ref().unwrap();
    assert!(g.arrived, "dtg {}", g.dtg);
}

#[test]
fn current_and_wind_affect_ground_track() {
    let mut c = quiet(cfg(Profile::Modern));
    c.initial.heading_deg = 0.0;
    c.environment.current_set_deg = 90.0;
    c.environment.current_drift_kn = 2.0;
    c.environment.wind_speed_kn = 0.0;
    let mut s = started(c);
    run(&mut s, 30.0);
    let v = &s.state().vessel;
    assert!(
        rad_to_deg(v.cog) > 15.0 && rad_to_deg(v.cog) < 30.0,
        "cog {}",
        rad_to_deg(v.cog)
    );
    assert!(v.sog > v.stw);
    // Vent de travers : dérive sous le vent.
    let mut c = quiet(cfg(Profile::Modern));
    c.initial.heading_deg = 0.0;
    c.environment.wind_direction_deg = 270.0; // vient de l'ouest, pousse vers l'est
    c.environment.wind_speed_kn = 30.0;
    let mut s = started(c);
    run(&mut s, 60.0);
    assert!(s.state().vessel.sway > 0.0);
}

#[test]
fn apparent_wind_head_on() {
    let mut c = quiet(cfg(Profile::Modern));
    c.initial.heading_deg = 0.0;
    c.environment.wind_direction_deg = 0.0;
    c.environment.wind_speed_kn = 10.0;
    let mut s = started(c);
    run(&mut s, 1.0);
    let w = &s.state().wind;
    assert!(w.apparent_angle.abs() < 1e-3);
    let expected = 10.0 + mps_to_knots(s.state().vessel.sog);
    assert!((mps_to_knots(w.apparent_speed) - expected).abs() < 0.05);
}

#[test]
fn anchor_holds_position() {
    let mut s = started(quiet(cfg(Profile::Modern)));
    s.submit(Command::ToggleAnchor, &UI).unwrap();
    run(&mut s, 30.0);
    let p = s.state().vessel.position;
    run(&mut s, 30.0);
    assert_eq!(s.state().vessel.position, p);
    assert_eq!(s.state().vessel.sog, 0.0);
}

#[test]
fn overrides_and_values() {
    let mut s = started(cfg(Profile::Modern));
    s.submit(
        Command::SetOverride {
            path: "env.water.depth".into(),
            value: 42.0,
        },
        &UI,
    )
    .unwrap();
    run(&mut s, 30.0);
    assert_eq!(s.state().water.depth, 42.0);
    s.submit(
        Command::ReleaseOverride {
            path: "env.water.depth".into(),
        },
        &UI,
    )
    .unwrap();
    s.submit(
        Command::SetValue {
            path: "env.wind.speedTrue".into(),
            value: 20.0,
        },
        &UI,
    )
    .unwrap();
    assert!((mps_to_knots(s.state().wind.true_speed) - 20.0).abs() < 1e-9);
    assert_eq!(
        s.submit(
            Command::SetValue {
                path: "nope".into(),
                value: 1.0
            },
            &UI
        ),
        Err(RejectReason::UnknownPath {
            path: "nope".into()
        })
    );
    assert_eq!(
        s.submit(
            Command::SetValue {
                path: "vessel.rateOfTurn".into(),
                value: 1.0
            },
            &UI
        ),
        Err(RejectReason::ReadOnly)
    );
    assert_eq!(
        s.submit(Command::SetRudder { deg: 55.0 }, &UI),
        Err(RejectReason::OutOfRange {
            min: -40.0,
            max: 40.0
        })
    );
}

#[test]
fn producer_lock_and_track_points() {
    let mut s = started(cfg(Profile::Modern));
    let p = LatLon::new(-35.1, 138.6);
    s.submit(
        Command::TrackPoint {
            position: p,
            sog_kn: Some(7.0),
            cog_deg: Some(90.0),
            altitude: None,
            time_ms: Some(1_000),
        },
        &Source::Track,
    )
    .unwrap();
    assert_eq!(
        s.submit(Command::SetRudder { deg: 3.0 }, &UI),
        Err(RejectReason::ProducerLocked)
    );
    assert!(s.submit(Command::Stop, &UI).is_ok());
    s.submit(Command::Start, &UI).unwrap();
    run(&mut s, 5.0);
    assert_eq!(
        s.state().vessel.position,
        p,
        "la source seule déplace le navire"
    );
    assert_eq!(s.state().sentence_time_ms(), 6_000);
    s.submit(Command::ReleaseSource, &Source::Track).unwrap();
    assert!(s.submit(Command::SetRudder { deg: 3.0 }, &UI).is_ok());
}

#[test]
fn pause_reset_and_lifecycle() {
    let mut s = Simulator::new(cfg(Profile::Modern)).unwrap();
    assert!(s.submit(Command::Pause, &UI).is_err());
    s.submit(Command::Start, &UI).unwrap();
    run(&mut s, 2.0);
    s.submit(Command::Pause, &UI).unwrap();
    let t = s.state().time_ms;
    run(&mut s, 2.0);
    assert_eq!(s.state().time_ms, t);
    s.submit(Command::TogglePause, &UI).unwrap();
    run(&mut s, 1.0);
    assert!(s.state().time_ms > t);
    s.submit(Command::Reset, &UI).unwrap();
    assert!(s.state().running);
    assert_eq!(s.state().elapsed_ms, 0);
    let evs = s.take_events();
    assert!(evs.len() >= 4);
}

#[test]
fn legacy_speed_commands_and_noise() {
    let mut s = started(cfg(Profile::Legacy));
    s.submit(Command::SetSpeed { kn: 10.0 }, &UI).unwrap();
    s.submit(Command::SetHeading { deg: 90.0 }, &UI).unwrap();
    assert!((rad_to_deg(s.state().vessel.heading) - 90.0).abs() < 1e-9);
    run(&mut s, 10.0);
    let sog = mps_to_knots(s.state().vessel.sog);
    assert!(sog > 9.0 && sog < 11.0, "{sog}");
    // pdop indépendant de hdop en legacy.
    assert_ne!(
        s.state().gnss.pdop,
        s.state().gnss.hdop.hypot(s.state().gnss.vdop)
    );
    assert_eq!(s.state().gnss.used, vec![8, 11, 15, 22]);
}

#[test]
fn invalid_config_is_explicit() {
    let mut c = SimConfig::default();
    c.vessel.max_rpm = 10.0;
    let e = Simulator::new(c).unwrap_err();
    assert_eq!(e.field, "vessel.maxRpm");
    let mut c = SimConfig::default();
    c.initial.position = LatLon::new(95.0, 0.0);
    assert!(Simulator::new(c).is_err());
}

#[test]
fn config_json_roundtrip_keeps_fields() {
    let c = cfg(Profile::Legacy);
    let j = serde_json::to_string(&c).unwrap();
    let back: SimConfig = serde_json::from_str(&j).unwrap();
    assert_eq!(c, back);
    let partial: SimConfig = serde_json::from_str(r#"{"vessel":{"lengthM":30}}"#).unwrap();
    assert_eq!(partial.vessel.length_m, 30.0);
    assert_eq!(partial.vessel.max_rpm, 3600.0);
    let _ = knots_to_mps(1.0);
}
