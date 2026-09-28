//! Rejeu d'un journal (`docs/10` §4). Correction legacy : le pas suivant après
//! une pause émet le bloc affiché au lieu de le sauter.

use crate::journal::Journal;

/// Commande de rejeu.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum ReplayCommand {
    /// Lecture.
    Play,
    /// Pause.
    Pause,
    /// Bloc suivant.
    StepForward,
    /// Bloc précédent.
    StepBack,
    /// Aller au bloc `index` (0-based) ; le prochain envoi l'émet.
    Seek {
        /// Index.
        index: usize,
    },
    /// Facteur de vitesse.
    Speed {
        /// Facteur (0.1 à 20).
        factor: f64,
    },
    /// Boucle en fin de journal.
    Repeat {
        /// Actif.
        on: bool,
    },
}

/// État observable.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayStatus {
    /// Index du prochain bloc à émettre.
    pub next: usize,
    /// Nombre de blocs.
    pub total: usize,
    /// En lecture.
    pub playing: bool,
    /// Facteur de vitesse.
    pub speed: f64,
    /// Boucle.
    pub repeat: bool,
    /// Fin atteinte.
    pub ended: bool,
}

/// Lecteur de rejeu, piloté par les ticks de l'application.
#[derive(Debug, Clone)]
pub struct ReplayPlayer {
    journal: Journal,
    next: usize,
    playing: bool,
    speed: f64,
    repeat: bool,
    ended: bool,
    acc_ms: f64,
}

impl ReplayPlayer {
    /// Nouveau lecteur, en pause au premier bloc.
    pub fn new(journal: Journal) -> Self {
        Self {
            journal,
            next: 0,
            playing: false,
            speed: 1.0,
            repeat: false,
            ended: false,
            acc_ms: 0.0,
        }
    }

    /// Journal.
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Statut.
    pub fn status(&self) -> ReplayStatus {
        ReplayStatus {
            next: self.next,
            total: self.journal.blocks.len(),
            playing: self.playing,
            speed: self.speed,
            repeat: self.repeat,
            ended: self.ended,
        }
    }

    /// Applique une commande ; rend le bloc à émettre immédiatement (pas à pas).
    pub fn command(&mut self, c: ReplayCommand) -> Option<usize> {
        let total = self.journal.blocks.len();
        match c {
            ReplayCommand::Play => {
                if self.ended {
                    self.next = 0;
                    self.ended = false;
                }
                self.playing = true;
                None
            }
            ReplayCommand::Pause => {
                self.playing = false;
                None
            }
            ReplayCommand::StepForward => {
                self.playing = false;
                if self.next < total {
                    let i = self.next;
                    self.next += 1;
                    self.ended = self.next >= total;
                    Some(i)
                } else {
                    None
                }
            }
            ReplayCommand::StepBack => {
                self.playing = false;
                self.ended = false;
                // Réémet le bloc précédant le dernier émis.
                let i = self.next.saturating_sub(2);
                self.next = i + 1;
                Some(i)
            }
            ReplayCommand::Seek { index } => {
                self.next = index.min(total.saturating_sub(1));
                self.ended = false;
                None
            }
            ReplayCommand::Speed { factor } => {
                self.speed = factor.clamp(0.1, 20.0);
                None
            }
            ReplayCommand::Repeat { on } => {
                self.repeat = on;
                None
            }
        }
    }

    /// Avance le temps de `dt_ms` à l'intervalle `interval_ms` ; rend les
    /// index des blocs à émettre.
    pub fn tick(&mut self, dt_ms: f64, interval_ms: f64) -> Vec<usize> {
        let mut out = Vec::new();
        if !self.playing {
            return out;
        }
        self.acc_ms += dt_ms * self.speed;
        let total = self.journal.blocks.len();
        while self.acc_ms >= interval_ms && self.playing {
            self.acc_ms -= interval_ms;
            if self.next >= total {
                if self.repeat {
                    self.next = 0;
                } else {
                    self.playing = false;
                    self.ended = true;
                    break;
                }
            }
            out.push(self.next);
            self.next += 1;
            if self.next >= total && !self.repeat {
                self.playing = false;
                self.ended = true;
            }
        }
        out
    }
}
