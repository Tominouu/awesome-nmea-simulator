//! Serveur WebSocket : un message texte par trame, hello Signal K à la
//! connexion. Comme le legacy, une phrase NMEA est envoyée avec son CRLF.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TryRecvError, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tungstenite::{Message, WebSocket};

use crate::{
    Frame, HelloFn, QUEUE_DEPTH, Shared, Transport, TransportError, TransportSpec, TransportState,
    TransportStatus,
};

struct Client {
    tx: SyncSender<String>,
    alive: Arc<AtomicBool>,
}

/// Serveur WebSocket.
pub struct WebSocketServerTransport {
    bind: String,
    port: u16,
    hello: Option<HelloFn>,
    shared: Shared,
    stop: Arc<AtomicBool>,
    clients: Arc<Mutex<Vec<Client>>>,
    accept: Option<JoinHandle<()>>,
    local: Option<SocketAddr>,
}

impl WebSocketServerTransport {
    /// Nouveau serveur, non démarré.
    pub fn new(
        id: &str,
        spec: &TransportSpec,
        bind: &str,
        port: u16,
        hello: Option<HelloFn>,
    ) -> Self {
        Self {
            bind: bind.into(),
            port,
            hello,
            shared: Shared::new(id, spec),
            stop: Arc::new(AtomicBool::new(false)),
            clients: Arc::new(Mutex::new(Vec::new())),
            accept: None,
            local: None,
        }
    }

    /// Adresse liée.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local
    }
}

fn serve(
    stream: TcpStream,
    alive: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    shared: Shared,
    hello: Option<HelloFn>,
) -> Option<SyncSender<String>> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let mut ws: WebSocket<TcpStream> = tungstenite::accept(stream).ok()?;
    let _ = ws
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(20)));
    let (tx, rx) = sync_channel::<String>(QUEUE_DEPTH);
    thread::spawn(move || {
        if let Some(h) = hello {
            let m = h();
            if ws.send(Message::text(m.clone())).is_ok() {
                shared.sent(m.len());
            }
        }
        'outer: while alive.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
            loop {
                match rx.try_recv() {
                    Ok(m) => {
                        let n = m.len();
                        if ws.send(Message::text(m)).is_err() {
                            break 'outer;
                        }
                        shared.sent(n);
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break 'outer,
                }
            }
            match ws.read() {
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
        }
        let _ = ws.close(None);
        alive.store(false, Ordering::Relaxed);
    });
    Some(tx)
}

impl Transport for WebSocketServerTransport {
    fn start(&mut self) -> Result<(), TransportError> {
        if self.accept.is_some() {
            return Ok(());
        }
        self.shared.set_state(TransportState::Starting);
        let listener = TcpListener::bind((self.bind.as_str(), self.port)).map_err(|e| {
            self.shared.set_state(TransportState::Failed);
            self.shared.error(format!("bind : {e}"));
            TransportError(format!("WebSocket {}:{} : {e}", self.bind, self.port))
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|e| TransportError(e.to_string()))?;
        self.local = listener.local_addr().ok();
        self.stop.store(false, Ordering::Relaxed);
        let (stop, clients, shared, hello) = (
            self.stop.clone(),
            self.clients.clone(),
            self.shared.clone(),
            self.hello.clone(),
        );
        self.accept = Some(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let alive = Arc::new(AtomicBool::new(true));
                        if let Some(tx) = serve(
                            stream,
                            alive.clone(),
                            stop.clone(),
                            shared.clone(),
                            hello.clone(),
                        ) {
                            let mut c = clients
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            c.push(Client { tx, alive });
                            shared.set_clients(c.len());
                        }
                    }
                    Err(_) => {
                        let mut c = clients
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        c.retain(|cl| cl.alive.load(Ordering::Relaxed));
                        shared.set_clients(c.len());
                        drop(c);
                        thread::sleep(Duration::from_millis(20));
                    }
                }
            }
        }));
        self.shared.set_state(TransportState::Running);
        Ok(())
    }

    fn send(&mut self, frame: &Frame) {
        let text = String::from_utf8_lossy(&frame.wire()).into_owned();
        let mut c = self
            .clients
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for cl in c.iter() {
            match cl.tx.try_send(text.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    self.shared.error("client trop lent, déconnecté");
                    cl.alive.store(false, Ordering::Relaxed);
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
        }
        self.shared.set_clients(0);
        self.shared.set_state(TransportState::Stopped);
    }

    fn status(&self) -> TransportStatus {
        self.shared.get()
    }
}

impl Drop for WebSocketServerTransport {
    fn drop(&mut self) {
        self.stop();
    }
}
