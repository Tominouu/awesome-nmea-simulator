//! Tests d'intégration des transports sur des sockets locaux réels (J4).

use std::io::{BufRead, BufReader, Read};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use nmeasim_transport::*;

fn inbox() -> Inbox {
    Arc::new(Mutex::new(Vec::new()))
}

fn wait_until(ms: u64, mut f: impl FnMut() -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        if f() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

fn free_port() -> u16 {
    // Hors de la plage éphémère (32768+) : le système ne réattribue pas ce
    // port à un autre test qui lie le port 0 en parallèle.
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let base = 20_000 + (std::process::id() % 500) as u16 * 16;
    loop {
        let p = base + NEXT.fetch_add(1, Ordering::Relaxed) % 8000;
        if TcpListener::bind(("127.0.0.1", p)).is_ok() && TcpListener::bind(("0.0.0.0", p)).is_ok()
        {
            return p;
        }
    }
}

fn read_line(r: &mut BufReader<TcpStream>) -> String {
    let mut s = String::new();
    r.read_line(&mut s).unwrap();
    s
}

#[test]
fn tcp_server_multi_clients_hello_and_framing() {
    let spec = TransportSpec::TcpServer {
        bind: "127.0.0.1".into(),
        port: 0,
    };
    let hello: HelloFn = Arc::new(|| "{\"hello\":1}\r\n".to_string());
    let mut t = TcpServerTransport::new("tcp", &spec, "127.0.0.1", 0, Some(hello), inbox());
    t.start().unwrap();
    assert_eq!(t.status().state, TransportState::Running);
    let addr = t.local_addr().unwrap();
    let conn = |a| {
        let s = TcpStream::connect(a).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        BufReader::new(s)
    };
    let mut a = conn(addr);
    let mut b = conn(addr);
    assert!(wait_until(2000, || t.status().clients == 2));
    assert_eq!(read_line(&mut a), "{\"hello\":1}\r\n");
    assert_eq!(read_line(&mut b), "{\"hello\":1}\r\n");
    t.send(&Frame::sentence("$IIHDT,13.0,T*10"));
    t.send(&Frame::json("{\"x\":1}\n"));
    assert_eq!(read_line(&mut a), "$IIHDT,13.0,T*10\r\n");
    assert_eq!(read_line(&mut b), "$IIHDT,13.0,T*10\r\n");
    assert_eq!(read_line(&mut a), "{\"x\":1}\n");
    drop(a);
    for _ in 0..50 {
        t.send(&Frame::sentence("$X"));
        thread::sleep(Duration::from_millis(5));
    }
    assert!(wait_until(2000, || t.status().clients == 1));
    t.stop();
    assert_eq!(t.status().state, TransportState::Stopped);
    let mut buf = [0u8; 64];
    assert!(wait_until(2000, || matches!(
        b.get_mut().read(&mut buf),
        Ok(0) | Err(_)
    )));
}

#[test]
fn tcp_server_bind_failure_is_reported() {
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let spec = TransportSpec::TcpServer {
        bind: "127.0.0.1".into(),
        port,
    };
    let mut t = TcpServerTransport::new("tcp", &spec, "127.0.0.1", port, None, inbox());
    assert!(t.start().is_err());
    assert_eq!(t.status().state, TransportState::Failed);
    assert_eq!(t.status().errors, 1);
}

#[test]
fn tcp_server_receives_input_lines() {
    let spec = TransportSpec::TcpServer {
        bind: "127.0.0.1".into(),
        port: 0,
    };
    let ib = inbox();
    let mut t = TcpServerTransport::new("tcp", &spec, "127.0.0.1", 0, None, ib.clone());
    t.start().unwrap();
    let mut c = TcpStream::connect(t.local_addr().unwrap()).unwrap();
    std::io::Write::write_all(&mut c, b"$GPTXT,01,01,02,HELLO*00\r\n").unwrap();
    assert!(wait_until(2000, || ib
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.starts_with("$GPTXT"))));
}

#[test]
fn tcp_client_reconnects_and_reports_state() {
    let port = free_port();
    let spec = TransportSpec::TcpClient {
        host: "127.0.0.1".into(),
        port,
    };
    let mut t = TcpClientTransport::new("cli", &spec, "127.0.0.1", port, None, inbox());
    t.start().unwrap();
    // Cible absente : jamais « running » (correction legacy).
    assert!(wait_until(1500, || t.status().state == TransportState::Reconnecting));
    assert!(t.status().errors >= 1);
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
    let (s1, _) = listener.accept().unwrap();
    s1.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    assert!(wait_until(3000, || t.status().state == TransportState::Running));
    t.send(&Frame::sentence("$A*41"));
    let mut r = BufReader::new(s1);
    assert_eq!(read_line(&mut r), "$A*41\r\n");
    // Le serveur ferme : le client se reconnecte.
    drop(r);
    t.send(&Frame::sentence("$B"));
    let (s2, _) = listener.accept().unwrap();
    s2.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    assert!(wait_until(3000, || t.status().state == TransportState::Running));
    t.send(&Frame::sentence("$C"));
    let mut r2 = BufReader::new(s2);
    assert_eq!(read_line(&mut r2), "$C\r\n");
    t.stop();
    assert_eq!(t.status().state, TransportState::Stopped);
}

#[test]
fn udp_client_one_datagram_per_frame() {
    let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
    rx.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let port = rx.local_addr().unwrap().port();
    let spec = TransportSpec::UdpClient {
        host: "127.0.0.1".into(),
        port,
    };
    let mut t = UdpTransport::new("u", &spec);
    t.start().unwrap();
    t.send(&Frame::sentence("$GPRMC,1"));
    t.send(&Frame::sentence("$GPRMC,2"));
    let mut buf = [0u8; 256];
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"$GPRMC,1\r\n");
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"$GPRMC,2\r\n");
    assert_eq!(t.status().messages, 2);
}

#[test]
// Vérifié sur Linux ; la diffusion et le multicast sur boucle locale varient
// selon les systèmes (non vérifié sur Windows et macOS).
#[cfg_attr(not(target_os = "linux"), ignore)]
fn udp_broadcast_on_loopback() {
    let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
    rx.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let port = rx.local_addr().unwrap().port();
    let lo = interfaces()
        .into_iter()
        .find(|(_, ip, _)| ip.is_loopback())
        .map(|(n, _, _)| n)
        .unwrap();
    let spec = TransportSpec::UdpBroadcast {
        interface: lo,
        port,
    };
    let mut t = UdpTransport::new("b", &spec);
    t.start().unwrap();
    assert_eq!(
        t.destination().unwrap().ip(),
        std::net::IpAddr::V4(Ipv4Addr::LOCALHOST)
    );
    t.send(&Frame::sentence("$X"));
    let mut buf = [0u8; 64];
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"$X\r\n");
}

#[test]
// Vérifié sur Linux ; la diffusion et le multicast sur boucle locale varient
// selon les systèmes (non vérifié sur Windows et macOS).
#[cfg_attr(not(target_os = "linux"), ignore)]
fn udp_multicast_two_receivers() {
    let group = Ipv4Addr::new(239, 255, 42, 98);
    let port = UdpSocket::bind("0.0.0.0:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mk = || {
        let s = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).unwrap();
        s.set_reuse_address(true).unwrap();
        s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())
            .unwrap();
        s.join_multicast_v4(&group, &Ipv4Addr::LOCALHOST).unwrap();
        let u: UdpSocket = s.into();
        u.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        u
    };
    let (r1, r2) = (mk(), mk());
    let spec = TransportSpec::UdpMulticast {
        group: group.to_string(),
        port,
        interface: "127.0.0.1".into(),
        ttl: 1,
        loopback: true,
    };
    let mut t = UdpTransport::new("m", &spec);
    t.start().unwrap();
    t.send(&Frame::sentence("$M"));
    let mut buf = [0u8; 64];
    for r in [&r1, &r2] {
        let (n, _) = r.recv_from(&mut buf).expect("réception multicast");
        assert_eq!(&buf[..n], b"$M\r\n");
    }
    let bad = TransportSpec::UdpMulticast {
        group: "10.0.0.1".into(),
        port,
        interface: String::new(),
        ttl: 1,
        loopback: true,
    };
    assert!(UdpTransport::new("x", &bad).start().is_err());
}

#[test]
fn websocket_hello_and_messages() {
    let port = free_port();
    let spec = TransportSpec::WebsocketServer {
        bind: "127.0.0.1".into(),
        port,
    };
    let hello: HelloFn = Arc::new(|| "{\"name\":\"nmea-simulator\"}".into());
    let mut t = WebSocketServerTransport::new("ws", &spec, "127.0.0.1", port, Some(hello));
    t.start().unwrap();
    let (mut ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}")).unwrap();
    if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    }
    assert_eq!(
        ws.read().unwrap().into_text().unwrap().as_str(),
        "{\"name\":\"nmea-simulator\"}"
    );
    assert!(wait_until(2000, || t.status().clients == 1));
    t.send(&Frame::sentence("$IIHDT,1.0,T*3E"));
    assert_eq!(
        ws.read().unwrap().into_text().unwrap().as_str(),
        "$IIHDT,1.0,T*3E\r\n"
    );
    t.stop();
}

#[cfg(unix)]
#[test]
fn serial_over_pty_pair() {
    use nix::pty::openpty;
    use std::os::fd::AsRawFd;
    let pty = openpty(None, None).unwrap();
    let slave = nix::unistd::ttyname(&pty.slave).unwrap();
    let mut cfg = nix::sys::termios::tcgetattr(&pty.slave).unwrap();
    nix::sys::termios::cfmakeraw(&mut cfg);
    nix::sys::termios::tcsetattr(&pty.slave, nix::sys::termios::SetArg::TCSANOW, &cfg).unwrap();
    let spec = TransportSpec::Serial {
        port: slave.to_string_lossy().into(),
        baud_rate: 4800,
        data_bits: 8,
        stop_bits: 1,
        parity: Parity::None,
        flow_control: FlowControl::None,
    };
    let ib = inbox();
    let mut t = SerialTransport::new("s", &spec, ib.clone());
    t.start().unwrap();
    t.send(&Frame::sentence("$GPRMC,A"));
    t.send(&Frame::json("{}"));
    let mut master = std::fs::File::from(pty.master);
    let mut got = Vec::new();
    let mut buf = [0u8; 64];
    let deadline = Instant::now() + Duration::from_secs(3);
    while got.len() < 14 && Instant::now() < deadline {
        if let Ok(n) = master.read(&mut buf) {
            got.extend_from_slice(&buf[..n]);
        }
    }
    assert_eq!(&got[..], b"$GPRMC,A\r\n{}\r\n");
    std::io::Write::write_all(&mut master, b"$GPTXT,IN*00\r\n").unwrap();
    assert!(wait_until(3000, || ib
        .lock()
        .unwrap()
        .iter()
        .any(|l| l == "$GPTXT,IN*00")));
    t.stop();
    let _ = master.as_raw_fd();
}

#[test]
fn serial_missing_port_fails_explicitly() {
    let spec = TransportSpec::Serial {
        port: "/dev/does-not-exist-nmeasim".into(),
        baud_rate: 4800,
        data_bits: 8,
        stop_bits: 1,
        parity: Parity::None,
        flow_control: FlowControl::None,
    };
    let mut t = SerialTransport::new("s", &spec, inbox());
    assert!(t.start().is_err());
    assert_eq!(t.status().state, TransportState::Failed);
}
