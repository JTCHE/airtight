//! Find AirPlay receivers: one mDNS PTR query out of every local IPv4
//! interface, then collect the replies for a while.

use airtight_wire::dns::{self, AIRPLAY, Record, Service};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

const GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 251), 5353);

/// This computer's name, as the receiver shows it.
pub fn host_name() -> String {
    #[cfg(windows)]
    let name = std::env::var("COMPUTERNAME").ok();
    #[cfg(target_os = "linux")]
    let name = std::fs::read_to_string("/proc/sys/kernel/hostname").ok().map(|n| n.trim().to_string());
    name.filter(|n| !n.is_empty()).unwrap_or_else(|| "Airtight".into())
}

/// This PC's IPv4 addresses, without loopback and link-local ones.
#[cfg(windows)]
pub fn local_addresses() -> Vec<Ipv4Addr> {
    use std::net::{SocketAddr, ToSocketAddrs};
    let mut out: Vec<Ipv4Addr> = (host_name().as_str(), 0)
        .to_socket_addrs()
        .map(|found| found.collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| match a {
            SocketAddr::V4(a) if !a.ip().is_loopback() && !a.ip().is_link_local() => Some(*a.ip()),
            _ => None,
        })
        .collect();
    out.dedup();
    out
}

/// The address of the interface that multicast leaves by.
// ponytail: one interface on Linux; read getifaddrs if multi-homed hosts matter.
#[cfg(target_os = "linux")]
pub fn local_addresses() -> Vec<Ipv4Addr> {
    crate::session::local_address(GROUP).into_iter().collect()
}

fn sender(interface: Ipv4Addr) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind(SocketAddrV4::new(interface, 0))?;
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        use windows::Win32::Networking::WinSock::{IP_MULTICAST_IF, IPPROTO_IP, SOCKET, setsockopt};
        let address = u32::from(interface).to_be_bytes();
        let raw = SOCKET(socket.as_raw_socket() as usize);
        // The bound address alone does not pick the outgoing interface for
        // multicast; without this, every query leaves by the default route.
        unsafe { setsockopt(raw, IPPROTO_IP.0, IP_MULTICAST_IF, Some(&address)) };
    }
    socket.set_multicast_ttl_v4(255)?;
    socket.set_read_timeout(Some(Duration::from_millis(50)))?;
    Ok(socket)
}

/// Every receiver that answers within `timeout`, by name.
pub fn browse(timeout: Duration) -> Vec<Service> {
    let sockets: Vec<UdpSocket> = local_addresses().into_iter().filter_map(|ip| sender(ip).ok()).collect();
    let query = dns::query(AIRPLAY);
    let mut records: Vec<Record> = Vec::new();
    let start = Instant::now();
    let mut next_query = start;
    let mut buffer = [0u8; 9000];
    while start.elapsed() < timeout {
        if Instant::now() >= next_query {
            for socket in &sockets {
                let _ = socket.send_to(&query, GROUP);
            }
            next_query += Duration::from_secs(1);
        }
        for socket in &sockets {
            while let Ok((count, _)) = socket.recv_from(&mut buffer) {
                records.extend(dns::parse(&buffer[..count]).unwrap_or_default());
            }
        }
    }
    let mut found = dns::services(&records, AIRPLAY);
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found
}
