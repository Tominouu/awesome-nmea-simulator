//! API distante HTTP (`docs/09` §7.5) : la même API de contrôle, sur
//! `127.0.0.1` par défaut, jeton facultatif, sans découverte.
//!
//! - `POST /api/v1/commands` : une [`Command`] JSON → `{"ok":true}` ou 422 ;
//! - `GET /api/v1/state` : instantané ;
//! - `GET /api/v1/status` : instantané, transports, producteur, débit ;
//! - `GET /api/v1/transports` : statuts ;
//! - `GET /api/v1/paths` : chemins écrivables et bornes ;
//! - `GET /api/v1/monitor?since=N` : lignes du moniteur postérieures à `N` ;
//! - `POST /api/v1/replay` : une commande de rejeu.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use nmeasim_sim::command::{Command, Source};
use nmeasim_source::replay::ReplayCommand;
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::config::ApiConfig;
use crate::runtime::{Controller, Published};

/// Serveur HTTP en tâche de fond.
pub struct ApiServer {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    /// Adresse effective.
    pub addr: String,
}

fn respond(req: Request, code: u16, body: &Value) {
    let h = Header::from_bytes("Content-Type", "application/json").expect("en-tête valide");
    let _ = req.respond(
        Response::from_string(body.to_string())
            .with_status_code(code)
            .with_header(h),
    );
}

impl ApiServer {
    /// Démarre le serveur.
    pub fn start(
        cfg: &ApiConfig,
        ctl: Controller,
        shared: Arc<RwLock<Published>>,
    ) -> Result<Self, String> {
        let server = Server::http((cfg.bind.as_str(), cfg.port))
            .map_err(|e| format!("API {}:{} : {e}", cfg.bind, cfg.port))?;
        let addr = server
            .server_addr()
            .to_ip()
            .map_or_else(|| format!("{}:{}", cfg.bind, cfg.port), |a| a.to_string());
        let stop = Arc::new(AtomicBool::new(false));
        let token = cfg.token.clone();
        let st = stop.clone();
        let worker = thread::spawn(move || {
            while !st.load(Ordering::Relaxed) {
                let Ok(Some(mut req)) = server.recv_timeout(Duration::from_millis(100)) else {
                    continue;
                };
                if let Some(t) = &token {
                    let ok = req.headers().iter().any(|h| {
                        h.field.equiv("Authorization") && h.value.as_str() == format!("Bearer {t}")
                    });
                    if !ok {
                        respond(req, 401, &json!({"error": "jeton manquant ou invalide"}));
                        continue;
                    }
                }
                let url = req.url().to_string();
                let (path, query) = url.split_once('?').unwrap_or((&url, ""));
                let read = || {
                    shared
                        .read()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                };
                match (req.method().clone(), path) {
                    (Method::Get, "/api/v1/state") => {
                        let v = serde_json::to_value(&read().snapshot).unwrap_or(Value::Null);
                        respond(req, 200, &v);
                    }
                    (Method::Get, "/api/v1/status") => {
                        let v = serde_json::to_value(&*read()).unwrap_or(Value::Null);
                        respond(req, 200, &v);
                    }
                    (Method::Get, "/api/v1/transports") => {
                        let v = serde_json::to_value(&read().transports).unwrap_or(Value::Null);
                        respond(req, 200, &v);
                    }
                    (Method::Get, "/api/v1/paths") => {
                        let v: Vec<Value> = nmeasim_sim::sim::PATHS
                            .iter()
                            .map(|(p, min, max)| json!({"path": p, "min": min, "max": max, "writable": true}))
                            .collect();
                        respond(req, 200, &Value::Array(v));
                    }
                    (Method::Get, "/api/v1/monitor") => {
                        let since: u64 = query
                            .strip_prefix("since=")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        let v: Vec<Value> = read()
                            .monitor
                            .iter()
                            .filter(|e| e.seq > since)
                            .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
                            .collect();
                        respond(req, 200, &Value::Array(v));
                    }
                    (Method::Post, "/api/v1/commands") => {
                        let mut body = String::new();
                        let _ = req.as_reader().take(1 << 20).read_to_string(&mut body);
                        match serde_json::from_str::<Command>(&body) {
                            Ok(c) => match ctl.submit(c, Source::Remote) {
                                Ok(()) => respond(req, 200, &json!({"ok": true})),
                                Err(r) => respond(
                                    req,
                                    422,
                                    &json!({"ok": false, "error": r, "message": r.to_string()}),
                                ),
                            },
                            Err(e) => respond(
                                req,
                                400,
                                &json!({"ok": false, "error": format!("commande invalide : {e}")}),
                            ),
                        }
                    }
                    (Method::Post, "/api/v1/replay") => {
                        let mut body = String::new();
                        let _ = req.as_reader().take(1 << 16).read_to_string(&mut body);
                        match serde_json::from_str::<ReplayCommand>(&body) {
                            Ok(c) => {
                                ctl.replay(c);
                                respond(req, 200, &json!({"ok": true}));
                            }
                            Err(e) => {
                                respond(req, 400, &json!({"ok": false, "error": e.to_string()}))
                            }
                        }
                    }
                    _ => respond(req, 404, &json!({"error": "route inconnue"})),
                }
            }
        });
        Ok(Self {
            stop,
            worker: Some(worker),
            addr,
        })
    }

    /// Arrête le serveur.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        self.stop();
    }
}
