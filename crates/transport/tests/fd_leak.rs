//! Fuite de descripteurs : binaire de test séparé, pour que le comptage de
//! `/proc/self/fd` ne soit pas faussé par les autres tests en parallèle.

use std::sync::{Arc, Mutex};

use nmeasim_transport::*;

fn inbox() -> Inbox {
    Arc::new(Mutex::new(Vec::new()))
}

#[cfg(target_os = "linux")]
#[test]
fn start_stop_cycles_do_not_leak_descriptors() {
    let fds = || std::fs::read_dir("/proc/self/fd").unwrap().count();
    let spec = TransportSpec::TcpServer {
        bind: "127.0.0.1".into(),
        port: 0,
    };
    let mut warm = TcpServerTransport::new("w", &spec, "127.0.0.1", 0, None, inbox());
    warm.start().unwrap();
    warm.stop();
    let before = fds();
    for _ in 0..1000 {
        // Cycle réaliste : bind, connexion d'un client acceptée, trame, arrêt.
        let mut t = TcpServerTransport::new("t", &spec, "127.0.0.1", 0, None, inbox());
        t.start().unwrap();
        let client = std::net::TcpStream::connect(t.local_addr().unwrap()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while t.status().clients == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(t.status().clients, 1);
        t.send(&Frame::sentence("$X"));
        t.stop();
        drop(client);
        let mut u = build(
            "u",
            &TransportSpec::UdpClient {
                host: "127.0.0.1".into(),
                port: 9,
            },
            None,
            inbox(),
        );
        u.start().unwrap();
        u.send(&Frame::sentence("$X"));
        u.stop();
    }
    // Laisser les threads d'écriture et de lecture se terminer.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let after = fds();
    assert!(after <= before + 4, "fuite : {before} → {after}");
}
