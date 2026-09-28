//! Fond de carte en tuiles (OpenStreetMap par défaut), facultatif : désactivé
//! par défaut car il exige le réseau (NFR-10). Téléchargement sur deux threads,
//! cache disque dans `~/.cache/nmeasim-rs/tiles`, attribution affichée.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions, Vec2};

type Key = (u8, u32, u32);

enum Slot {
    Loading,
    Ready(TextureHandle),
    Failed(Instant),
}

/// Cache de tuiles.
pub struct TileCache {
    slots: HashMap<Key, Slot>,
    req: Sender<Key>,
    done: Receiver<(Key, Result<ColorImage, String>)>,
    pending: HashSet<Key>,
    /// Dernière erreur de téléchargement.
    pub last_error: Option<String>,
}

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("nmeasim-rs")
        .join("tiles")
}

fn fetch(url_tpl: &str, (z, x, y): Key) -> Result<ColorImage, String> {
    let path = cache_dir().join(format!("{z}/{x}/{y}.png"));
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            let url = url_tpl
                .replace("{z}", &z.to_string())
                .replace("{x}", &x.to_string())
                .replace("{y}", &y.to_string());
            let mut resp = ureq::get(&url)
                .header(
                    "User-Agent",
                    concat!(
                        "nmeasim-rs/",
                        env!("CARGO_PKG_VERSION"),
                        " (simulateur NMEA)"
                    ),
                )
                .call()
                .map_err(|e| e.to_string())?;
            let b = resp.body_mut().read_to_vec().map_err(|e| e.to_string())?;
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::fs::write(&path, &b);
            }
            b
        }
    };
    let img = image::load_from_memory(&bytes)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

impl TileCache {
    /// Démarre les threads de téléchargement.
    pub fn new(url_tpl: &str) -> Self {
        let (req, req_rx) = channel::<Key>();
        let (done_tx, done) = channel();
        let req_rx = Arc::new(Mutex::new(req_rx));
        for _ in 0..2 {
            let (rx, tx, tpl) = (req_rx.clone(), done_tx.clone(), url_tpl.to_string());
            thread::spawn(move || {
                loop {
                    let key = match rx.lock().map(|r| r.recv()) {
                        Ok(Ok(k)) => k,
                        _ => return,
                    };
                    if tx.send((key, fetch(&tpl, key))).is_err() {
                        return;
                    }
                }
            });
        }
        Self {
            slots: HashMap::new(),
            req,
            done,
            pending: HashSet::new(),
            last_error: None,
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((k, r)) = self.done.try_recv() {
            self.pending.remove(&k);
            let slot = match r {
                Ok(img) => Slot::Ready(ctx.load_texture(
                    format!("tile-{}-{}-{}", k.0, k.1, k.2),
                    img,
                    TextureOptions::LINEAR,
                )),
                Err(e) => {
                    self.last_error = Some(e);
                    Slot::Failed(Instant::now())
                }
            };
            self.slots.insert(k, slot);
        }
        if self.slots.len() > 600 {
            self.slots.retain(|_, s| matches!(s, Slot::Loading));
        }
    }

    /// Dessine les tuiles couvrant `rect`. `world` donne la position monde
    /// (pixels au zoom fractionnaire `zoom`) du centre de `rect`.
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: Rect,
        center_world: (f64, f64),
        zoom: f64,
    ) {
        self.poll(ctx);
        let z = zoom.floor().clamp(0.0, 19.0) as u8;
        let scale = 2f64.powf(zoom - f64::from(z));
        let tile_px = 256.0 * scale;
        // Coordonnées monde au zoom entier z.
        let (cx, cy) = (center_world.0 / scale, center_world.1 / scale);
        let half = Vec2::new(rect.width() / 2.0, rect.height() / 2.0);
        let x0 = ((cx - f64::from(half.x) / scale) / 256.0).floor() as i64;
        let x1 = ((cx + f64::from(half.x) / scale) / 256.0).floor() as i64;
        let y0 = ((cy - f64::from(half.y) / scale) / 256.0).floor() as i64;
        let y1 = ((cy + f64::from(half.y) / scale) / 256.0).floor() as i64;
        let n = 1i64 << z;
        for ty in y0.max(0)..=y1.min(n - 1) {
            for tx in x0..=x1 {
                let wx = tx.rem_euclid(n) as u32;
                let key = (z, wx, ty as u32);
                let min = rect.center()
                    + Vec2::new(
                        ((tx as f64 * 256.0 - cx) * scale) as f32,
                        ((ty as f64 * 256.0 - cy) * scale) as f32,
                    );
                let r = Rect::from_min_size(min, Vec2::splat(tile_px as f32));
                match self.slots.get(&key) {
                    Some(Slot::Ready(t)) => {
                        painter.image(
                            t.id(),
                            r,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    Some(Slot::Failed(at)) if at.elapsed() < Duration::from_secs(30) => {}
                    Some(Slot::Loading) => {}
                    _ => {
                        if self.pending.insert(key) {
                            self.slots.insert(key, Slot::Loading);
                            let _ = self.req.send(key);
                        }
                    }
                }
            }
        }
        if !self.pending.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
