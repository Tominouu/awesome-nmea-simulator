//! Sonde manette : moteur + pilote gilrs sans interface. Affiche les
//! manettes vues, les dernières commandes et l'effet sur le navire.
//! usage : cargo run -p nmeasim-app --example gamepad_probe -- [secondes]

use std::time::{Duration, Instant};

use nmeasim_app::Runtime;
use nmeasim_app::config::AppConfig;
use nmeasim_app::gamepad::GamepadDriver;
use nmeasim_sim::command::{Command, Source};

fn main() {
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(12);
    let mut cfg = AppConfig::default();
    cfg.transports.clear();
    cfg.simulation.initial.speed_kn = 0.0;
    let rt = Runtime::spawn(cfg.clone()).expect("moteur");
    rt.submit(Command::Start, Source::Script).expect("start");
    let pad = GamepadDriver::spawn(rt.controller(), cfg.gamepad.active_profile());
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_millis(500));
        let v = pad.view();
        let s = rt.snapshot();
        let devs: Vec<String> = v
            .devices
            .iter()
            .map(|d| {
                let axes: Vec<String> = d
                    .normalized
                    .iter()
                    .filter(|(_, x)| x.abs() > 0.01)
                    .map(|(a, x)| format!("{}={x:+.2}", a.label()))
                    .collect();
                let btn: Vec<&str> = d
                    .pressed
                    .iter()
                    .filter(|(_, p)| **p)
                    .map(|(b, _)| b.label())
                    .collect();
                format!("{} [{}] [{}]", d.name, axes.join(" "), btn.join(" "))
            })
            .collect();
        println!(
            "t={:4.1}s dispo={} manettes={:?} cmds={:?} | moteurs={} thr={:+.2} barre={:+.1} pilote={:?} mouillage={}",
            t0.elapsed().as_secs_f64(),
            v.available,
            devs,
            v.last_commands,
            s.engines[0].running,
            s.engines[0].throttle,
            s.vessel.rudder_command,
            s.autopilot.mode,
            s.vessel.anchored,
        );
    }
}
