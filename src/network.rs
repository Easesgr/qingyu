//! Weather's macOS transport. Ignoring an HTTP proxy alone does not bypass a
//! packet tunnel: bind every socket to the physical uplink and resolve names
//! over that same path so a VPN's fake-IP DNS cannot reintroduce the tunnel.
use std::{io::{self, Read}, net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream}, time::{Duration, Instant}};

const DNS_BUDGET: Duration = Duration::from_secs(4);
const DNS_ATTEMPT: Duration = Duration::from_secs(2);
// Fixed bootstrap IPs avoid consulting a system resolver intercepted by TUN.
// TLS still verifies doh.pub; the weather connection verifies its own host.
const DNS_BOOTSTRAP: [Ipv4Addr; 2] = [Ipv4Addr::new(1,12,12,12), Ipv4Addr::new(120,53,53,53)];

pub(crate) fn weather_transport(builder: ureq::AgentBuilder) -> ureq::AgentBuilder {
    builder.resolver(resolve).tcp_connector(connect)
}

fn fake_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.octets()[0] == 198 && (ip.octets()[1] & 0xfe) == 18,
        IpAddr::V6(ip) => ip.segments()[..3] == [0x2001, 2, 0],
    }
}

fn connect(address: SocketAddr, timeout: Option<Duration>) -> io::Result<TcpStream> {
    if fake_ip(address.ip()) {return Err(io::Error::new(io::ErrorKind::InvalidInput, "weather cannot connect to a VPN fake IP"));}
    crate::mac_services::direct_connect(address, timeout)
}

fn resolve(netloc: &str) -> io::Result<Vec<SocketAddr>> {
    if let Ok(address) = netloc.parse::<SocketAddr>() {
        return if fake_ip(address.ip()) {Err(io::Error::new(io::ErrorKind::InvalidInput, "VPN fake IP in weather address"))} else {Ok(vec![address])};
    }
    let (host, port) = netloc.rsplit_once(':').ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput, "invalid weather host"))?;
    let port: u16 = port.parse().map_err(|_|io::Error::new(io::ErrorKind::InvalidInput, "invalid weather port"))?;
    // No DNS or uplink is needed for local services and offline test fixtures.
    if host.eq_ignore_ascii_case("localhost") {return Ok(vec![(Ipv4Addr::LOCALHOST,port).into()]);}
    let mut url = url::Url::parse("https://doh.pub/dns-query").expect("static DoH URL");
    url.query_pairs_mut().append_pair("name",host).append_pair("type","A");
    let deadline = Instant::now() + DNS_BUDGET;
    let mut last_error = io::Error::new(io::ErrorKind::TimedOut, "weather DNS timed out");
    for bootstrap in DNS_BOOTSTRAP {
        let remaining = deadline.saturating_duration_since(Instant::now()).min(DNS_ATTEMPT);
        if remaining.is_zero() {break;}
        // This agent only resolves the pinned DoH endpoint; it never recurses
        // into this resolver, uses system DNS, follows redirects, or uses proxies.
        let agent = ureq::AgentBuilder::new().try_proxy_from_env(false).https_only(true).redirects(0)
            .timeout_connect(remaining).timeout(remaining).tcp_connector(connect)
            .resolver(move |name: &str| {
                if name == "doh.pub:443" {Ok(vec![(bootstrap,443).into()])}
                else {Err(io::Error::new(io::ErrorKind::InvalidInput, "unexpected DoH endpoint"))}
            }).build();
        let result = (|| {
            let response = agent.get(url.as_str()).set("Accept","application/dns-json").call()
                .map_err(|error|io::Error::other(format!("weather direct DNS failed: {error}")))?;
            let value: serde_json::Value = serde_json::from_reader(response.into_reader().take(64 * 1024))
                .map_err(|_|io::Error::new(io::ErrorKind::InvalidData,"invalid weather DNS response"))?;
            dns_answers(&value, port)
        })();
        match result {Ok(addresses)=>return Ok(addresses),Err(error)=>last_error=error}
    }
    Err(last_error)
}

fn dns_answers(value: &serde_json::Value, port: u16) -> io::Result<Vec<SocketAddr>> {
    if value["Status"].as_u64() != Some(0) {return Err(io::Error::new(io::ErrorKind::NotFound,"weather DNS lookup failed"));}
    let mut addresses = Vec::new();
    for answer in value["Answer"].as_array().into_iter().flatten() {
        if answer["type"].as_u64() != Some(1) {continue;}
        let Some(ip) = answer["data"].as_str().and_then(|text|text.parse::<Ipv4Addr>().ok()) else {continue;};
        if ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() ||
            ip.is_multicast() || ip.is_broadcast() || ip.is_documentation() || ip.octets()[0] == 0 ||
            ip.octets()[0] >= 240 || fake_ip(ip.into()) {continue;}
        let address = (ip,port).into();
        if !addresses.contains(&address) {addresses.push(address);}
    }
    if addresses.is_empty() {return Err(io::Error::new(io::ErrorKind::NotFound,"no public IPv4 address for weather host"));}
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn weather_dns_rejects_tunnel_and_private_answers() {
        let answers=json!({"Status":0,"Answer":[
            {"type":5,"data":"alias.example"}, {"type":1,"data":"198.18.1.226"},
            {"type":1,"data":"198.19.255.254"}, {"type":1,"data":"10.1.2.3"},
            {"type":1,"data":"127.0.0.1"}, {"type":1,"data":"0.0.0.0"},
            {"type":1,"data":"172.66.175.107"}, {"type":1,"data":"172.66.175.107"}
        ]});
        assert_eq!(dns_answers(&answers,443).unwrap(),vec!["172.66.175.107:443".parse::<SocketAddr>().unwrap()]);
        assert!(dns_answers(&json!({"Status":3,"Answer":[{"type":1,"data":"172.66.175.107"}]}),443).is_err());
        assert!(dns_answers(&json!({"Status":0,"Answer":[{"type":1,"data":"198.18.1.226"}]}),443).is_err());
        for address in ["198.18.1.226:443","198.19.1.2:443","[2001:2::1ce]:443"] {
            assert!(resolve(address).is_err());
            assert!(connect(address.parse().unwrap(),Some(Duration::from_millis(10))).is_err());
        }
    }

    #[test]
    fn weather_loopback_resolution_needs_no_network() {
        for netloc in ["127.0.0.1:8123","localhost:8123"] {
            assert_eq!(resolve(netloc).unwrap(),vec!["127.0.0.1:8123".parse::<SocketAddr>().unwrap()]);
        }
        assert_eq!(resolve("[::1]:8123").unwrap(),vec!["[::1]:8123".parse::<SocketAddr>().unwrap()]);
    }
}
