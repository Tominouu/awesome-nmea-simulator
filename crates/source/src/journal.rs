//! Journal `.nmeasim` (`docs/10` §3, décision D7).
//!
//! Format : `nmeasim,<version>\r\n` [`meta:{json}\r\n`] `~` puis un bloc par
//! tick (phrases terminées par `\r\n`) suivi de `~`. La ligne `meta:` est dans
//! le segment d'en-tête : le legacy l'ignore.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde_json::Value;

use crate::track::SourceError;

/// Version du format `meta`.
pub const META_FORMAT: u64 = 1;

/// En-tête d'un journal.
pub fn header(version: &str, meta: Option<&Value>) -> String {
    let mut h = format!("nmeasim,{version}\r\n");
    if let Some(m) = meta {
        // Ni `~` ni saut de ligne dans la ligne meta (D7).
        let json = m.to_string().replace('~', "\\u007e");
        h.push_str(&format!("meta:{json}\r\n"));
    }
    h.push('~');
    h
}

/// Bloc d'un tick, prêt à écrire.
pub fn block(lines: &[String]) -> String {
    let mut s = String::new();
    for l in lines {
        s.push_str(l);
        s.push_str("\r\n");
    }
    s.push('~');
    s
}

/// Journal lu.
#[derive(Debug, Clone, PartialEq)]
pub struct Journal {
    /// Version dans l'en-tête.
    pub version: String,
    /// Métadonnées D7, si présentes.
    pub meta: Option<Value>,
    /// Blocs, chacun avec ses `\r\n`.
    pub blocks: Vec<String>,
    /// Avertissement (format `meta` inconnu…).
    pub warning: Option<String>,
}

impl Journal {
    /// Phrases d'un bloc, sans terminateurs.
    pub fn lines(&self, i: usize) -> Vec<String> {
        self.blocks
            .get(i)
            .map(|b| {
                b.split("\r\n")
                    .map(|l| l.trim_end_matches(['\r', '\n']))
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Lit un journal (legacy ou étendu).
pub fn parse(data: &str) -> Result<Journal, SourceError> {
    let mut seg: Vec<&str> = data.split('~').collect();
    if seg.first().is_none_or(|h| !h.starts_with("nmeasim")) {
        return Err(SourceError(
            "fichier journal invalide : en-tête `nmeasim` absent".into(),
        ));
    }
    let head = seg.remove(0);
    if seg.last().is_some_and(|l| l.trim().is_empty()) {
        seg.pop();
    }
    let mut lines = head.split("\r\n").filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or("nmeasim");
    let version = first
        .split_once(',')
        .map_or("", |(_, v)| v)
        .trim()
        .to_string();
    let mut meta = None;
    let mut warning = None;
    for (n, l) in lines.enumerate() {
        if let Some(j) = l.strip_prefix("meta:") {
            match serde_json::from_str::<Value>(j) {
                Ok(v) => {
                    if v.get("format")
                        .and_then(Value::as_u64)
                        .is_some_and(|f| f > META_FORMAT)
                    {
                        warning = Some("format meta plus récent : rejeu en mode chaîne".into());
                    }
                    meta = Some(v);
                }
                Err(e) => {
                    return Err(SourceError(format!(
                        "ligne {} : meta illisible : {e}",
                        n + 2
                    )));
                }
            }
        }
    }
    if seg.is_empty() {
        return Err(SourceError("le journal ne contient aucune donnée".into()));
    }
    Ok(Journal {
        version,
        meta,
        blocks: seg.into_iter().map(str::to_string).collect(),
        warning,
    })
}

/// Lit un fichier journal.
pub fn read(path: &Path) -> Result<Journal, SourceError> {
    let data = std::fs::read_to_string(path)
        .map_err(|e| SourceError(format!("{} : {e}", path.display())))?;
    parse(&data)
}

/// Écrivain asynchrone : la simulation n'attend jamais le disque. Si la file
/// est pleine ou le disque en erreur, les blocs sont abandonnés et comptés.
pub struct JournalWriter {
    tx: Option<SyncSender<String>>,
    worker: Option<JoinHandle<()>>,
    dropped: Arc<AtomicU64>,
    written: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    path: PathBuf,
}

impl JournalWriter {
    /// Ouvre le fichier ; l'extension `.nmeasim` est ajoutée si absente.
    pub fn create(path: &Path, version: &str, meta: Option<&Value>) -> Result<Self, SourceError> {
        let mut path = path.to_path_buf();
        if path.extension().is_none() {
            path.set_extension("nmeasim");
        }
        let file =
            File::create(&path).map_err(|e| SourceError(format!("{} : {e}", path.display())))?;
        let mut w = BufWriter::new(file);
        w.write_all(header(version, meta).as_bytes())
            .map_err(|e| SourceError(e.to_string()))?;
        let (tx, rx) = sync_channel::<String>(4096);
        let (dropped, written, failed, error) = (
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(Mutex::new(None)),
        );
        let (wr, fl, er) = (written.clone(), failed.clone(), error.clone());
        let worker = thread::spawn(move || {
            for b in rx {
                if fl.load(Ordering::Relaxed) {
                    continue;
                }
                match w.write_all(b.as_bytes()).and_then(|()| w.flush()) {
                    Ok(()) => {
                        wr.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        fl.store(true, Ordering::Relaxed);
                        *er.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some(e.to_string());
                    }
                }
            }
            let _ = w.flush();
        });
        Ok(Self {
            tx: Some(tx),
            worker: Some(worker),
            dropped,
            written,
            failed,
            error,
            path,
        })
    }

    /// Chemin effectif.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Ajoute un bloc ; ne bloque jamais.
    pub fn push(&self, lines: &[String]) {
        if let Some(tx) = &self.tx {
            match tx.try_send(block(lines)) {
                Ok(()) => {}
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Blocs écrits, blocs abandonnés, erreur disque éventuelle.
    pub fn stats(&self) -> (u64, u64, Option<String>) {
        (
            self.written.load(Ordering::Relaxed),
            self.dropped.load(Ordering::Relaxed),
            self.error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        )
    }

    /// Écriture désactivée après une erreur disque.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// Ferme et vide le fichier.
    pub fn close(mut self) -> (u64, u64, Option<String>) {
        self.tx = None;
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
        self.stats()
    }
}

impl Drop for JournalWriter {
    fn drop(&mut self) {
        self.tx = None;
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}
