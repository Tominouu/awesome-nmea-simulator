//! TCP serveur et client.

use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::{
    Frame, HelloFn, Inbox, QUEUE_DEPTH, Shared, Transport, TransportError, TransportSpec,
    TransportState, TransportStatus,
};

struct Client {
    tx: SyncSender<Vec<u8>>,
    stream: TcpStream,
    alive: Arc<AtomicBool>,
}

/// Serveur TCP : plusieurs clients, un thread d'écriture par client.
pub struct TcpServerTransport {
    bind: String,
    port: u16,
    hello: Option<HelloFn>,
    inbox: Inbox,
    shared: Shared,
    stop: Arc<AtomicBool>,
    clients: Arc<Mutex<Vec<Client>>>,
    accept: Option<JoinHandle<()>>,
    local: Option<SocketAddr>,
}

impl TcpServerTransport {
    /// Nouveau serveur, non démarré.
    pub fn new(
        id: &str,
        spec: &TransportSpec,
        bind: &str,
        port: u16,
        hello: Option<HelloFn>,
        inbox: Inbox,
    ) -> Self {
        Self {
            bind: bind.into(),
            port,
            hello,
            inbox,
            shared: Shared::new(id, spec),
            stop: Arc::new(AtomicBool::new(false)),
            clients: Arc::new(Mutex::new(Vec::new())),
            accept: None,
            local: None,
        }
    }

    /// Adresse effectivement liée (utile avec le port 0 en test).
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local
    }
}

/// Lance le thread d'écriture d'une connexion ; rend l'émetteur.
fn spawn_writer(
    mut stream: TcpStream,
    alive: Arc<AtomicBool>,
    shared: Shared,
) -> SyncSender<Vec<u8>> {
    let (tx, rx): (SyncSender<Vec<u8>>, Receiver<Vec<u8>>) = sync_channel(QUEUE_DEPTH);
    thread::spawn(move || {
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        for buf in rx {
            if !alive.load(Ordering::Relaxed) {
                break;
            }
            match stream.write_all(&buf) {
                Ok(()) => shared.sent(buf.len()),
                Err(e) => {
                    shared.error(format!("écriture : {e}"));
                    alive.store(false, Ordering::Relaxed);
                    let _ = stream.shutdown(Shutdown::Both);
                    break;
                }
            }
        }
    });
    tx
}

/// Lance un lecteur de lignes vers la boîte de réception.
fn spawn_reader(stream: TcpStream, alive: Arc<AtomicBool>, inbox: Inbox) {
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            match line {
                Ok(l) => inbox
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(l),
                Err(_) => break,
            }
        }
        alive.store(false, Ordering::Relaxed);
    });
}

impl Transport for TcpServerTransport {
    fn start(&mut self) -> Result<(), TransportError> {
        if self.accept.is_some() {
            return Ok(());
        }
        self.shared.set_state(TransportState::Starting);
        let listener = TcpListener::bind((self.bind.as_str(), self.port)).map_err(|e| {
            self.shared.set_state(TransportState::Failed);
            self.shared
                .error(format!("bind {}:{} : {e}", self.bind, self.port));
            TransportError(format!("TCP {}:{} : {e}", self.bind, self.port))
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|e| TransportError(e.to_string()))?;
        self.local = listener.local_addr().ok();
        self.stop.store(false, Ordering::Relaxed);
        let (stop, clients, shared, hello, inbox) = (
            self.stop.clone(),
            self.clients.clone(),
            self.shared.clone(),
            self.hello.clone(),
            self.inbox.clone(),
        );
        self.accept = Some(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_nodelay(true);
                        let alive = Arc::new(AtomicBool::new(true));
                        let Ok(w) = stream.try_clone() else { continue };
                        if let Ok(r) = stream.try_clone() {
                            spawn_reader(r, alive.clone(), inbox.clone());
                        }
                        let tx = spawn_writer(w, alive.clone(), shared.clone());
                        if let Some(h) = &hello {
                            let _ = tx.try_send(h().into_bytes());
                        }
                        let mut c = clients
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        c.push(Client { tx, stream, alive });
                        shared.set_clients(c.len());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        let mut c = clients
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        c.retain(|cl| cl.alive.load(Ordering::Relaxed));
                        shared.set_clients(c.len());
                        drop(c);
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(e) => {
                        shared.error(format!("accept : {e}"));
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        }));
        self.shared.set_state(TransportState::Running);
        Ok(())
    }

    fn send(&mut self, frame: &Frame) {
        let buf = frame.wire();
        let mut c = self
            .clients
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for cl in c.iter() {
            match cl.tx.try_send(buf.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    // Client trop lent : déconnecté plutôt que de bloquer.
                    self.shared.error("client trop lent, déconnecté");
                    cl.alive.store(false, Ordering::Relaxed);
                    let _ = cl.stream.shutdown(Shutdown::Both);
                }
                Err(TrySendError::Disconnected(_)) => cl.alive.store(false, Ordering::Relaxed),
            }
        }
        c.retain(|cl| cl.alive.load(Ordering::Relaxed));
        self.shared.set_clients(c.len());
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
        let mut c = self
            .clients
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for cl in c.drain(..) {
            cl.alive.store(false, Ordering::Relaxed);
            let _ = cl.stream.shutdown(Shutdown::Both);
        }
        self.shared.set_clients(0);
        self.shared.set_state(TransportState::Stopped);
    }

    fn status(&self) -> TransportStatus {
        self.shared.get()
    }
}

impl Drop for TcpServerTransport {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Client TCP avec reconnexion exponentielle bornée (correction legacy).
pub struct TcpClientTransport {
    host: String,
    port: u16,
    hello: Option<HelloFn>,
    inbox: Inbox,
    shared: Shared,
    stop: Arc<AtomicBool>,
    tx: Option<SyncSender<Vec<u8>>>,
    worker: Option<JoinHandle<()>>,
}

impl TcpClientTransport {
    /// Nouveau client, non démarré.
    pub fn new(
        id: &str,
        spec: &TransportSpec,
        host: &str,
        port: u16,
        hello: Option<HelloFn>,
        inbox: Inbox,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            hello,
            inbox,
            shared: Shared::new(id, spec),
            stop: Arc::new(AtomicBool::new(false)),
            tx: None,
            worker: None,
        }
    }
}

/// Délai de reprise : 0,5 s doublé à chaque échec, plafonné à 10 s.
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_millis((500u64 << attempt.min(5)).min(10_000))
}

impl Transport for TcpClientTransport {
    fn start(&mut self) -> Result<(), TransportError> {
        if self.worker.is_some() {
            return Ok(());
        }
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| TransportError(format!("adresse {}:{} : {e}", self.host, self.port)))?
            .next()
            .ok_or_else(|| {
                TransportError(format!("adresse {}:{} introuvable", self.host, self.port))
            })?;
        self.stop.store(false, Ordering::Relaxed);
        self.shared.set_state(TransportState::Starting);
        let (tx, rx) = sync_channel::<Vec<u8>>(QUEUE_DEPTH);
        self.tx = Some(tx);
        let (stop, shared, hello, inbox) = (
            self.stop.clone(),
            self.shared.clone(),
            self.hello.clone(),
            self.inbox.clone(),
        );
        self.worker = Some(thread::spawn(move || {
            let mut attempt = 0u32;
            while !stop.load(Ordering::Relaxed) {
                match TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
                    Ok(mut stream) => {
                        attempt = 0;
                        let _ = stream.set_nodelay(true);
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                        // Vider les trames accumulées pendant la déconnexion
                        // AVANT d'annoncer l'état : une trame envoyée juste
                        // après le passage à Running ne doit pas être jetée.
                        while rx.try_recv().is_ok() {}
                        shared.set_state(TransportState::Running);
                        shared.set_clients(1);
                        let alive = Arc::new(AtomicBool::new(true));
                        if let Ok(r) = stream.try_clone() {
                            spawn_reader(r, alive.clone(), inbox.clone());
                        }
                        if let Some(h) = &hello {
                            let hb = h().into_bytes();
                            if stream.write_all(&hb).is_ok() {
                                shared.sent(hb.len());
                            }
                        }
                        while alive.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
                            match rx.recv_timeout(Duration::from_millis(100)) {
                                Ok(buf) => match stream.write_all(&buf) {
                                    Ok(()) => shared.sent(buf.len()),
                                    Err(e) => {
                                        shared.error(format!("écriture : {e}"));
                                        break;
                                    }
                                },
                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                                Err(_) => return,
                            }
                        }
                        let _ = stream.shutdown(Shutdown::Both);
                        shared.set_clients(0);
                        if !stop.load(Ordering::Relaxed) {
                            shared.set_state(TransportState::Reconnecting);
                        }
                    }
                    Err(e) => {
                        shared.error(format!("connexion {addr} : {e}"));
                        shared.set_state(TransportState::Reconnecting);
                        let wait = backoff(attempt);
                        attempt += 1;
                        let mut waited = Duration::ZERO;
                        while waited < wait && !stop.load(Ordering::Relaxed) {
                            thread::sleep(Duration::from_millis(50));
                            waited += Duration::from_millis(50);
                        }
                    }
                }
            }
        }));
        Ok(())
    }

    fn send(&mut self, frame: &Frame) {
        if self.shared.state() != TransportState::Running {
            return;
        }
        if let Some(tx) = &self.tx {
            if tx.try_send(frame.wire()).is_err() {
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
        self.shared.set_clients(0);
        self.shared.set_state(TransportState::Stopped);
    }

    fn status(&self) -> TransportStatus {
        self.shared.get()
    }
}

impl Drop for TcpClientTransport {
    fn drop(&mut self) {
        self.stop();
    }
}
