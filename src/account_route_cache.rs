//! A last verified public candidate, never an authorization or presence cache.
//! Callers must first obtain a fresh owned-device route and verify the same key
//! again at the endpoint before sending any connection authorization.

fn endpoint(address: &str, port: u16) -> Option<(String, u16)> {
    let ip: std::net::Ipv4Addr = address.parse().ok()?;
    let [a, b, c, _] = ip.octets();
    if !(40000..60000).contains(&port)
        || a == 0
        || a >= 224
        || ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0)
        || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113)
    {
        return None;
    }
    Some((ip.to_string(), port))
}

pub fn encode_verified(api: &str, key: &str, address: &str, port: u16) -> Option<String> {
    if !api.starts_with("https://")
        || api.contains(['\n', '\r'])
        || key.len() != 44
        || key.contains(['\n', '\r'])
    {
        return None;
    }
    let (address, port) = endpoint(address, port)?;
    Some(format!(
        "v1\n{}\n{}\n{}\n{}",
        api.trim_end_matches('/'),
        key,
        address,
        port
    ))
}

pub fn resolve(value: &str, api: &str, current_key: &str) -> Option<(String, u16)> {
    let fields: Vec<_> = value.split('\n').collect();
    if fields.len() != 5
        || fields[0] != "v1"
        || fields[1] != api.trim_end_matches('/')
        || fields[2] != current_key
        || current_key.len() != 44
    {
        return None;
    }
    endpoint(fields[3], fields[4].parse().ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    const API: &str = "https://staging.example.test";
    fn key() -> String {
        format!("{}=", "A".repeat(43))
    }
    #[test]
    fn same_device_and_api_can_recover() {
        let saved = encode_verified(API, &key(), "47.203.39.150", 55981).unwrap();
        assert_eq!(
            resolve(&saved, API, &key()),
            Some(("47.203.39.150".into(), 55981))
        );
        assert!(resolve(&saved, "https://other.example.test", &key()).is_none());
        assert!(resolve(&saved, API, &format!("{}=", "B".repeat(43))).is_none());
        assert!(resolve(&saved, API, "").is_none());
    }
    #[test]
    fn only_bounded_public_candidates_are_accepted() {
        for address in [
            "127.0.0.1",
            "192.168.1.2",
            "100.64.0.1",
            "169.254.169.254",
            "203.0.113.1",
            "198.51.100.2",
            "224.1.1.1",
            "0.1.2.3",
            "example.test",
        ] {
            assert!(encode_verified(API, &key(), address, 55981).is_none());
        }
        for port in [0, 22, 21128, 39999, 60000, 65535] {
            assert!(encode_verified(API, &key(), "47.203.39.150", port).is_none());
        }
        assert!(encode_verified("http://example.test", &key(), "47.203.39.150", 55981).is_none());
        assert!(resolve("v1\nincomplete", API, &key()).is_none());
    }
}
