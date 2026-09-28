//! UDP : diffusion, client unicast, multicast. Un datagramme par trame.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};

use socket2::{Domain, Protocol, Socket, Type};

use crate::{
    Frame, Shared, Transport, TransportError, TransportSpec, TransportState, TransportStatus,
};

/// Interfaces IPv4 : (nom, adresse, adresse de diffusion).
pub fn interfaces() -> Vec<(String, Ipv4Addr, Option<Ipv4Addr>)> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => Some((i.name.clone(), v4.ip, v4.broadcast)),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect()
}

/// Adresse de diffusion de l'interface nommée (dérivée du CIDR, comme le
/// legacy). `lo` n'a pas de diffusion : son adresse est utilisée.
pub fn broadcast_address(interface: &str) -> Option<Ipv4Addr> {
    if interface.is_empty() {
        return None;
    }
    interfaces()
        .into_iter()
        .find(|(n, _, _)| n == interface)
        .map(|(_, ip, bc)| bc.unwrap_or(ip))
}

/// Transport UDP.
pub struct UdpTransport {
    spec: TransportSpec,
    shared: Shared,
    socket: Option<UdpSocket>,
    dest: Option<SocketAddr>,
}

impl UdpTransport {
    /// Nouveau transport, non démarré.
    pub fn new(id: &str, spec: &TransportSpec) -> Self {
        Self {
            spec: spec.clone(),
            shared: Shared::new(id, spec),
            socket: None,
            dest: None,
        }
    }

    /// Destination résolue.
    pub fn destination(&self) -> Option<SocketAddr> {
        self.dest
    }

    fn open(&self) -> Result<(UdpSocket, SocketAddr), String> {
        match &self.spec {
            TransportSpec::UdpBroadcast { interface, port } => {
                let s = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
                s.set_broadcast(true).map_err(|e| e.to_string())?;
                let ip = broadcast_address(interface).unwrap_or(Ipv4Addr::BROADCAST);
                Ok((s, SocketAddr::V4(SocketAddrV4::new(ip, *port))))
            }
            TransportSpec::UdpClient { host, port } => {
                let dest = (host.as_str(), *port)
                    .to_socket_addrs()
                    .map_err(|e| e.to_string())?
                    .find(SocketAddr::is_ipv4)
                    .ok_or("adresse IPv4 introuvable")?;
                let s = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
                Ok((s, dest))
            }
            TransportSpec::UdpMulticast {
                group,
                port,
                interface,
                ttl,
                loopback,
            } => {
                let g: Ipv4Addr = group
                    .parse()
                    .map_err(|_| format!("groupe invalide : {group}"))?;
                if !g.is_multicast() {
                    return Err(format!("{g} n'est pas une adresse multicast"));
                }
                let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
                    .map_err(|e| e.to_string())?;
                sock.set_multicast_ttl_v4(*ttl).map_err(|e| e.to_string())?;
                sock.set_multicast_loop_v4(*loopback)
                    .map_err(|e| e.to_string())?;
                if !interface.is_empty() {
                    let ip: Ipv4Addr = match interface.parse() {
                        Ok(ip) => ip,
                        Err(_) => interfaces()
                            .into_iter()
                            .find(|(n, _, _)| n == interface)
                            .map(|(_, ip, _)| ip)
                            .ok_or(format!("interface inconnue : {interface}"))?,
                    };
                    sock.set_multicast_if_v4(&ip).map_err(|e| e.to_string())?;
                }
                // Pas d'adhésion au groupe côté émetteur (correction legacy).
                sock.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)).into())
                    .map_err(|e| e.to_string())?;
                Ok((sock.into(), SocketAddr::new(IpAddr::V4(g), *port)))
            }
            _ => Err("spécification non UDP".into()),
        }
    }
}

impl Transport for UdpTransport {
    fn start(&mut self) -> Result<(), TransportError> {
        if self.socket.is_some() {
            return Ok(());
        }
        self.shared.set_state(TransportState::Starting);
        match self.open() {
            Ok((s, d)) => {
                self.socket = Some(s);
                self.dest = Some(d);
                self.shared.set_state(TransportState::Running);
                Ok(())
            }
            Err(e) => {
                self.shared.error(&e);
                self.shared.set_state(TransportState::Failed);
                Err(TransportError(e))
            }
        }
    }

    fn send(&mut self, frame: &Frame) {
        if let (Some(s), Some(d)) = (&self.socket, self.dest) {
            let buf = frame.wire();
            match s.send_to(&buf, d) {
                Ok(n) => self.shared.sent(n),
                Err(e) => self.shared.error(e),
            }
        }
    }

    fn stop(&mut self) {
        self.socket = None;
        self.shared.set_state(TransportState::Stopped);
    }

    fn status(&self) -> TransportStatus {
        self.shared.get()
    }
}
