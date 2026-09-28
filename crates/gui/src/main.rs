//! `nmeasim` : interface graphique par défaut, `--headless` pour un moteur
//! sans affichage (bancs d'essai, serveurs, CI).

mod app;
mod map;
mod panels;
mod tiles;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use nmeasim_app::config::{self, AppConfig};
use nmeasim_app::http::ApiServer;
use nmeasim_app::scenario;
use nmeasim_app::{APP_VERSION, Runtime};
use nmeasim_sim::command::{Command, Source};
use nmeasim_sim::compat::Profile;

const USAGE: &str = "nmeasim — simulateur NMEA 0183 / Signal K

USAGE
  nmeasim [OPTIONS]

OPTIONS
  --headless              moteur sans interface graphique
  --config <fichier>      configuration (défaut : ~/.config/nmeasim-rs/config.json)
  --scenario <fichier>    scénario à charger
  --legacy                profil de compatibilité legacy
  --modern                profil moderne (défaut)
  --seed <n>              graine aléatoire
  --interval <ms>         intervalle de sortie
  --start                 démarre la simulation au lancement
  --duration <s>          (headless) s'arrête après s secondes
  --print                 (headless) écrit les phrases émises sur la sortie standard
  --api [port]            active l'API HTTP (127.0.0.1, port 8375 par défaut)
  --import-legacy <f>     convertit une configuration NMEASimulator 1.6.1 et l'écrit
                          dans --config (ou l'affiche avec --stdout)
  --stdout                avec --import-legacy : affiche au lieu d'écrire
  --version               version
  --help                  cette aide
";

/// Options de la ligne de commande.
#[derive(Debug, Default)]
struct Cli {
    headless: bool,
    config: Option<PathBuf>,
    scenario: Option<PathBuf>,
    profile: Option<Profile>,
    seed: Option<u64>,
    interval: Option<u32>,
    start: bool,
    duration: Option<f64>,
    print: bool,
    api: Option<u16>,
    import_legacy: Option<PathBuf>,
    stdout: bool,
}

fn parse_cli(args: &[String]) -> Result<Cli, String> {
    let mut c = Cli::default();
    let mut it = args.iter().skip(1).peekable();
    let need = |it: &mut std::iter::Peekable<std::iter::Skip<std::slice::Iter<'_, String>>>,
                flag: &str| {
        it.next()
            .cloned()
            .ok_or_else(|| format!("{flag} : valeur attendue"))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--headless" => c.headless = true,
            "--config" => c.config = Some(need(&mut it, a)?.into()),
            "--scenario" => c.scenario = Some(need(&mut it, a)?.into()),
            "--legacy" => c.profile = Some(Profile::Legacy),
            "--modern" => c.profile = Some(Profile::Modern),
            "--seed" => {
                c.seed = Some(
                    need(&mut it, a)?
                        .parse()
                        .map_err(|_| "--seed : entier attendu")?,
                )
            }
            "--interval" => {
                c.interval = Some(
                    need(&mut it, a)?
                        .parse()
                        .map_err(|_| "--interval : entier attendu")?,
                )
            }
            "--start" => c.start = true,
            "--duration" => {
                c.duration = Some(
                    need(&mut it, a)?
                        .parse()
                        .map_err(|_| "--duration : nombre attendu")?,
                )
            }
            "--print" => c.print = true,
            "--api" => {
                c.api = Some(match it.peek() {
                    Some(p) if !p.starts_with("--") => need(&mut it, a)?
                        .parse()
                        .map_err(|_| "--api : port attendu")?,
                    _ => 8375,
                });
            }
            "--import-legacy" => c.import_legacy = Some(need(&mut it, a)?.into()),
            "--stdout" => c.stdout = true,
            "--version" | "-V" => {
                println!("nmeasim {APP_VERSION}");
                std::process::exit(0);
            }
            "--help" | "-h" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("option inconnue : {other}\n\n{USAGE}")),
        }
    }
    Ok(c)
}

/// Configuration effective : fichier, scénario, puis options.
fn effective_config(cli: &Cli) -> Result<(AppConfig, Vec<String>, PathBuf), String> {
    let path = cli.config.clone().unwrap_or_else(config::default_path);
    let loaded =
        config::load(&path).map_err(|e| format!("configuration {} : {e}", path.display()))?;
    let mut warnings = loaded.warnings;
    let mut cfg = loaded.config;
    if let Some(s) = &cli.scenario {
        let sc = scenario::load(s).map_err(|e| format!("scénario : {e}"))?;
        cfg = sc.apply_to(&cfg);
        cfg.last_scenario = Some(s.display().to_string());
        warnings.push(format!("scénario chargé : {}", sc.name));
    }
    if let Some(p) = cli.profile {
        cfg.simulation.compatibility.profile = p;
    }
    if let Some(s) = cli.seed {
        cfg.simulation.seed = Some(s);
    }
    if let Some(i) = cli.interval {
        cfg.output.interval_ms = i;
    }
    if let Some(p) = cli.api {
        cfg.api.enabled = true;
        cfg.api.port = p;
    }
    if cli.start {
        cfg.auto_start = true;
    }
    cfg.validate()
        .map_err(|e| format!("configuration invalide : {e}"))?;
    Ok((cfg, warnings, path))
}

fn import_legacy(cli: &Cli, src: &PathBuf) -> Result<(), String> {
    let text = std::fs::read_to_string(src).map_err(|e| format!("{} : {e}", src.display()))?;
    let loaded = config::from_json(&text).map_err(|e| e.to_string())?;
    for w in &loaded.warnings {
        eprintln!("avertissement : {w}");
    }
    if cli.stdout {
        println!(
            "{}",
            serde_json::to_string_pretty(&loaded.config).map_err(|e| e.to_string())?
        );
    } else {
        let dst = cli.config.clone().unwrap_or_else(config::default_path);
        config::save(&dst, &loaded.config).map_err(|e| e.to_string())?;
        eprintln!("configuration écrite : {}", dst.display());
    }
    Ok(())
}

fn headless(cli: &Cli) -> Result<(), String> {
    let (cfg, warnings, _) = effective_config(cli)?;
    for w in &warnings {
        eprintln!("[nmeasim] {w}");
    }
    let scenario_cmds = match &cli.scenario {
        Some(s) => scenario::load(s).map_err(|e| e.to_string())?.commands(),
        None => vec![],
    };
    let api_cfg = cfg.api.clone();
    let rt = Runtime::spawn(cfg).map_err(|e| e.to_string())?;
    for c in scenario_cmds {
        rt.submit(c, Source::Script).map_err(|e| e.to_string())?;
    }
    let _api = if api_cfg.enabled {
        let s = ApiServer::start(&api_cfg, rt.controller(), rt.shared())?;
        eprintln!("[nmeasim] API HTTP sur http://{}/api/v1", s.addr);
        Some(s)
    } else {
        None
    };
    let events = rt.subscribe();
    if !rt.snapshot().running {
        rt.submit(Command::Start, Source::Script)
            .map_err(|e| e.to_string())?;
    }
    eprintln!("[nmeasim] {} démarré (headless)", APP_VERSION);
    for t in &rt.published().transports {
        eprintln!(
            "[nmeasim] transport {} ({}) {} : {:?}",
            t.id, t.kind, t.endpoint, t.state
        );
    }
    let started = Instant::now();
    let mut seq = 0;
    loop {
        std::thread::sleep(Duration::from_millis(50));
        for e in events.try_iter() {
            eprintln!(
                "[nmeasim] {}",
                serde_json::to_string(&e).unwrap_or_default()
            );
        }
        if cli.print {
            let fresh: Vec<(u64, String)> = rt
                .published()
                .monitor
                .iter()
                .filter(|m| m.seq > seq)
                .map(|m| (m.seq, m.text.clone()))
                .collect();
            for (s, text) in fresh {
                println!("{text}");
                seq = s;
            }
        }
        if cli
            .duration
            .is_some_and(|d| started.elapsed().as_secs_f64() >= d)
        {
            break;
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cli = match parse_cli(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let result = if let Some(src) = cli.import_legacy.clone() {
        import_legacy(&cli, &src)
    } else if cli.headless {
        headless(&cli)
    } else {
        match effective_config(&cli) {
            Ok((cfg, warnings, path)) => {
                app::run(cfg, warnings, path, cli.scenario.clone()).map_err(|e| e.to_string())
            }
            Err(e) => Err(e),
        }
    };
    if let Err(e) = result {
        eprintln!("nmeasim : {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        std::iter::once("nmeasim".to_string())
            .chain(s.split_whitespace().map(str::to_string))
            .collect()
    }

    #[test]
    fn cli_parsing() {
        let c = parse_cli(&args(
            "--headless --legacy --seed 7 --interval 500 --api --duration 2",
        ))
        .unwrap();
        assert!(c.headless);
        assert_eq!(c.profile, Some(Profile::Legacy));
        assert_eq!(c.seed, Some(7));
        assert_eq!(c.interval, Some(500));
        assert_eq!(c.api, Some(8375));
        assert_eq!(c.duration, Some(2.0));
        assert_eq!(parse_cli(&args("--api 9000")).unwrap().api, Some(9000));
        assert!(parse_cli(&args("--bogus")).is_err());
        assert!(parse_cli(&args("--seed x")).is_err());
    }
}
