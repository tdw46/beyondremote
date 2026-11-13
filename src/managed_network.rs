//! Bounded, intent-driven UPnP setup. No hosted polling or background lease job.
use hbb_common::{anyhow, bail, ResultType};
use std::{
    io::Read,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

const SERVICE: &str = "urn:schemas-upnp-org:service:WANIPConnection:1";
const PORTS: [(&str, u16); 4] = [
    ("TCP", 21115),
    ("TCP", 21116),
    ("UDP", 21116),
    ("TCP", 21117),
];

pub struct Mapping {
    client: reqwest::blocking::Client,
    url: url::Url,
    local: Ipv4Addr,
    owner: String,
    ports: Vec<(&'static str, u16)>,
    pub public_ip: Ipv4Addr,
}

impl Drop for Mapping {
    fn drop(&mut self) {
        self.remove();
    }
}

fn public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !ip.is_private()
        && !ip.is_loopback()
        && !ip.is_link_local()
        && !ip.is_unspecified()
        && !ip.is_broadcast()
        && !ip.is_multicast()
        && a != 0
        && a < 240
        && !(a == 100 && (64..=127).contains(&b))
        && !(a == 192 && b == 0)
        && !(a == 198 && (b == 18 || b == 19 || b == 51 && c == 100))
        && !(a == 203 && b == 0 && c == 113)
}

fn xml_value(xml: &str, name: &str) -> ResultType<Option<String>> {
    let doc = roxmltree::Document::parse(xml)?;
    Ok(doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .and_then(|n| n.text())
        .map(str::to_owned))
}

fn bounded_text(mut response: reqwest::blocking::Response) -> ResultType<String> {
    let mut bytes = Vec::new();
    response.by_ref().take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        bail!("Router response too large");
    }
    Ok(String::from_utf8(bytes)?)
}

fn local_url(raw: &str, ip: Ipv4Addr) -> ResultType<url::Url> {
    let parsed = url::Url::parse(raw)?;
    if parsed.scheme() != "http"
        || parsed.host_str() != Some(ip.to_string().as_str())
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        bail!("Router returned an unexpected control address");
    }
    Ok(parsed)
}

fn default_gateway() -> ResultType<Ipv4Addr> {
    if let Ok(gateway) = default_net::get_default_gateway() {
        if let std::net::IpAddr::V4(ip) = gateway.ip_addr {
            if ip.is_private() {
                return Ok(ip);
            }
        }
    }
    // default-net 0.14 does not resolve the route on every supported macOS
    // release. Query the OS routing table directly, without a shell or network.
    #[cfg(target_os = "macos")]
    {
        let result = std::process::Command::new("/sbin/route")
            .args(["-n", "get", "default"])
            .output()?;
        if result.status.success() {
            for line in String::from_utf8_lossy(&result.stdout).lines() {
                if let Some(value) = line.trim().strip_prefix("gateway:") {
                    if let Ok(ip) = value.trim().parse::<Ipv4Addr>() {
                        if ip.is_private() {
                            return Ok(ip);
                        }
                    }
                }
            }
        }
    }
    bail!("Could not identify the router. Connect to a local network and retry setup.")
}

impl Mapping {
    pub fn establish(device: &str) -> ResultType<Self> {
        if device.is_empty() || !device.bytes().all(|b| b.is_ascii_alphanumeric()) {
            bail!("A registered device is required for automatic server setup");
        }
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        let gateway = default_gateway()?;
        let deadline = Instant::now() + Duration::from_secs(3);
        socket.send_to(b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n", "239.255.255.250:1900")?;
        let mut location = None;
        for _ in 0..8 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            socket.set_read_timeout(Some(remaining))?;
            let mut data = [0u8; 4096];
            let (n, peer) = socket.recv_from(&mut data)?;
            let SocketAddr::V4(peer) = peer else {
                continue;
            };
            if !peer.ip().is_private() || *peer.ip() != gateway {
                continue;
            }
            let text = std::str::from_utf8(&data[..n])?;
            if let Some(raw) = text.lines().find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("location"))
                    .map(|(_, v)| v.trim())
            }) {
                location = Some((local_url(raw, *peer.ip())?, *peer.ip()));
                break;
            }
        }
        let (location, gateway_ip) =
            location.ok_or_else(|| anyhow::anyhow!("Router does not offer automatic setup"))?;
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(4))
            .build()?;
        let xml = bounded_text(client.get(location.clone()).send()?.error_for_status()?)?;
        let doc = roxmltree::Document::parse(&xml)?;
        let control = doc
            .descendants()
            .find(|n| {
                n.is_element()
                    && n.tag_name().name() == "service"
                    && n.children().any(|c| {
                        c.is_element()
                            && c.tag_name().name() == "serviceType"
                            && c.text() == Some(SERVICE)
                    })
            })
            .and_then(|n| {
                n.children()
                    .find(|c| c.is_element() && c.tag_name().name() == "controlURL")
            })
            .and_then(|n| n.text())
            .ok_or_else(|| anyhow::anyhow!("Router protocol is not supported"))?;
        let url = local_url(location.join(control)?.as_str(), gateway_ip)?;
        let route = UdpSocket::bind("0.0.0.0:0")?;
        route.connect((gateway_ip, 1900))?;
        let SocketAddr::V4(local) = route.local_addr()? else {
            bail!("IPv4 router required");
        };
        let mut mapping = Self {
            client,
            url,
            local: *local.ip(),
            owner: format!("BeyondRemote-Staging-{device}"),
            ports: Vec::new(),
            public_ip: Ipv4Addr::UNSPECIFIED,
        };
        let external = mapping.soap("GetExternalIPAddress", "")?;
        mapping.public_ip = xml_value(&external, "NewExternalIPAddress")?
            .ok_or_else(|| anyhow::anyhow!("Router did not report an internet address"))?
            .parse()?;
        if !public_ipv4(mapping.public_ip) {
            bail!("This router is behind another private network; internet setup needs an upstream route");
        }
        for (protocol, port) in PORTS {
            if let Err(e) = mapping.add(protocol, port) {
                mapping.remove();
                return Err(e);
            }
        }
        Ok(mapping)
    }

    fn soap(&self, action: &str, args: &str) -> ResultType<String> {
        let body = format!("<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><u:{action} xmlns:u=\"{SERVICE}\">{args}</u:{action}></s:Body></s:Envelope>");
        bounded_text(
            self.client
                .post(self.url.clone())
                .header("Content-Type", "text/xml")
                .header("SOAPAction", format!("\"{SERVICE}#{action}\""))
                .body(body)
                .send()?,
        )
    }
    fn identity_args(protocol: &str, port: u16) -> String {
        format!("<NewRemoteHost></NewRemoteHost><NewExternalPort>{port}</NewExternalPort><NewProtocol>{protocol}</NewProtocol>")
    }
    fn owned(&self, xml: &str, port: u16) -> ResultType<bool> {
        Ok(
            xml_value(xml, "NewInternalClient")?.as_deref()
                == Some(self.local.to_string().as_str())
                && xml_value(xml, "NewInternalPort")?.as_deref() == Some(port.to_string().as_str())
                && xml_value(xml, "NewPortMappingDescription")?.as_deref()
                    == Some(self.owner.as_str()),
        )
    }
    fn add(&mut self, protocol: &'static str, port: u16) -> ResultType<()> {
        let identity = Self::identity_args(protocol, port);
        let current = self.soap("GetSpecificPortMappingEntry", &identity)?;
        if self.owned(&current, port)? {
            self.ports.push((protocol, port));
            return Ok(());
        }
        if xml_value(&current, "errorCode")?.as_deref() != Some("714") {
            bail!(
                "Port {port}/{protocol} already belongs to another service, or cannot be inspected"
            );
        }
        let args = format!("{identity}<NewInternalPort>{port}</NewInternalPort><NewInternalClient>{}</NewInternalClient><NewEnabled>1</NewEnabled><NewPortMappingDescription>{}</NewPortMappingDescription><NewLeaseDuration>0</NewLeaseDuration>",self.local,self.owner);
        let result = self.soap("AddPortMapping", &args)?;
        if xml_value(&result, "errorCode")?.is_some() {
            bail!("Router refused automatic port setup");
        }
        self.ports.push((protocol, port));
        if !self.owned(&self.soap("GetSpecificPortMappingEntry", &identity)?, port)? {
            bail!("Router port setup could not be verified");
        }
        Ok(())
    }
    pub fn remove(&mut self) {
        for (protocol, port) in self.ports.drain(..).collect::<Vec<_>>() {
            let identity = Self::identity_args(protocol, port);
            match self
                .soap("GetSpecificPortMappingEntry", &identity)
                .and_then(|xml| self.owned(&xml, port))
            {
                Ok(true) => {
                    if let Err(e) = self.soap("DeletePortMapping", &identity).and_then(|xml| {
                        if xml_value(&xml, "errorCode")?.is_some() {
                            bail!("Router refused removal");
                        }
                        Ok(())
                    }) {
                        hbb_common::log::warn!("Could not remove owned server port: {}", e);
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    hbb_common::log::warn!("Could not verify owned router port for cleanup: {}", e)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_non_public_routes() {
        for ip in [
            "10.0.0.1",
            "192.168.4.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "203.0.113.2",
            "224.0.0.1",
        ] {
            assert!(!public_ipv4(ip.parse().unwrap()));
        }
        assert!(public_ipv4("47.203.39.150".parse().unwrap()));
    }
    #[test]
    fn control_endpoint_stays_on_discovered_router() {
        let ip = "192.168.4.1".parse().unwrap();
        assert!(local_url("http://192.168.4.1:1900/ipc", ip).is_ok());
        for url in [
            "http://example.com/ipc",
            "https://192.168.4.1/ipc",
            "http://user@192.168.4.1/ipc",
        ] {
            assert!(local_url(url, ip).is_err());
        }
    }

    #[test]
    #[ignore = "Explicit local router mutation rehearsal only"]
    fn map_verify_remove() {
        assert_eq!(
            std::env::var("BEYONDREMOTE_TEST_ROUTER").unwrap(),
            "192.168.4.1"
        );
        let mut mapping = Mapping::establish("236321073").unwrap();
        assert_eq!(mapping.ports.len(), 4);
        mapping.remove();
        for (protocol, port) in PORTS {
            let xml = mapping
                .soap(
                    "GetSpecificPortMappingEntry",
                    &Mapping::identity_args(protocol, port),
                )
                .unwrap();
            assert_eq!(
                xml_value(&xml, "errorCode").unwrap().as_deref(),
                Some("714")
            );
        }
    }
}
