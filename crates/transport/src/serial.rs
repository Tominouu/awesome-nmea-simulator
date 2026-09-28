//! Liaison série : écriture par thread dédié, lecture ligne à ligne vers la
//! boîte de réception, reprise automatique après débranchement.

use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, SyncSender, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::{
    FlowControl, Frame, Inbox, Parity, QUEUE_DEPTH, Shared, Transport, TransportError,
    TransportSpec, TransportState, TransportStatus,
};

/// Transport série.
pub struct SerialTransport {
    spec: TransportSpec,
    inbox: Inbox,
    shared: Shared,
    stop: Arc<AtomicBool>,
    tx: Option<SyncSender<Vec<u8>>>,
    worker: Option<JoinHandle<()>>,
}

impl SerialTransport {
    /// Nouveau transport, non démarré.
    pub fn new(id: &str, spec: &TransportSpec, inbox: Inbox) -> Self {
        Self {
            spec: spec.clone(),
            inbox,
            shared: Shared::new(id, spec),
            stop: Arc::new(AtomicBool::new(false)),
            tx: None,
            worker: None,
        }
    }
}

fn open(spec: &TransportSpec) -> Result<Box<dyn serialport::SerialPort>, String> {
    let TransportSpec::Serial {
        port,
        baud_rate,
        data_bits,
        stop_bits,
        parity,
        flow_control,
    } = spec
    else {
        return Err("spécification non série".into());
    };
    let db = match data_bits {
        5 => serialport::DataBits::Five,
        6 => serialport::DataBits::Six,
        7 => serialport::DataBits::Seven,
        8 => serialport::DataBits::Eight,
        n => return Err(format!("bits de données invalides : {n}")),
    };
    let sb = match stop_bits {
        1 => serialport::StopBits::One,
        2 => serialport::StopBits::Two,
        n => return Err(format!("bits d'arrêt invalides : {n}")),
    };
    let pa = match parity {
        Parity::None => serialport::Parity::None,
        Parity::Even => serialport::Parity::Even,
        Parity::Odd => serialport::Parity::Odd,
    };
    let fc = match flow_control {
        FlowControl::None => serialport::FlowControl::None,
        FlowControl::Software => serialport::FlowControl::Software,
        FlowControl::Hardware => serialport::FlowControl::Hardware,
    };
    serialport::new(port, *baud_rate)
        .data_bits(db)
        .stop_bits(sb)
        .parity(pa)
        .flow_control(fc)
        .timeout(Duration::from_millis(50))
        .open()
        .map_err(|e| format!("{port} : {e}"))
}

/// Liste des ports série détectés.
pub fn ports() -> Vec<String> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.port_name)
        .collect()
}

impl Transport for SerialTransport {
    fn start(&mut self) -> Result<(), TransportError> {
        if self.worker.is_some() {
            return Ok(());
        }
        self.shared.set_state(TransportState::Starting);
        // Première ouverture synchrone : l'échec est rendu à l'appelant.
        let first = open(&self.spec).map_err(|e| {
            self.shared.error(&e);
            self.shared.set_state(TransportState::Failed);
            TransportError(e)
        })?;
        self.stop.store(false, Ordering::Relaxed);
        let (tx, rx) = sync_channel::<Vec<u8>>(QUEUE_DEPTH);
        self.tx = Some(tx);
        let (stop, shared, inbox, spec) = (
            self.stop.clone(),
            self.shared.clone(),
            self.inbox.clone(),
            self.spec.clone(),
        );
        shared.set_state(TransportState::Running);
        self.worker = Some(thread::spawn(move || {
            let mut port = Some(first);
            let mut line = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                let Some(p) = port.as_mut() else {
                    thread::sleep(Duration::from_secs(2));
                    match open(&spec) {
                        Ok(p) => {
                            port = Some(p);
                            shared.set_state(TransportState::Running);
                        }
                        Err(e) => shared.error(e),
                    }
                    continue;
                };
                let mut failed = false;
                match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok(buf) => match p.write_all(&buf).and_then(|()| p.flush()) {
                        Ok(()) => shared.sent(buf.len()),
                        Err(e) => {
                            shared.error(format!("écriture : {e}"));
                            failed = true;
                        }
                    },
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                let mut b = [0u8; 256];
                match p.read(&mut b) {
                    Ok(n) => {
                        for &c in &b[..n] {
                            if c == b'\n' {
                                let s = String::from_utf8_lossy(&line)
                                    .trim_end_matches('\r')
                                    .to_string();
                                inbox
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .push(s);
                                line.clear();
                            } else if line.len() < 4096 {
                                line.push(c);
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(e) => {
                        shared.error(format!("lecture : {e}"));
                        failed = true;
                    }
                }
                if failed {
                    port = None;
                    shared.set_state(TransportState::Reconnecting);
                }
            }
        }));
        Ok(())
    }

    fn send(&mut self, frame: &Frame) {
        if let Some(tx) = &self.tx {
            if tx.try_send(frame.wire_serial()).is_err() {
                self.shared.error("file pleine, trame abandonnée");
            }
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.tx = None;
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
        self.shared.set_state(TransportState::Stopped);
    }

    fn status(&self) -> TransportStatus {
        self.shared.get()
    }
}

impl Drop for SerialTransport {
    fn drop(&mut self) {
        self.stop();
    }
}
