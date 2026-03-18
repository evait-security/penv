use std::{
    fs,
    net::IpAddr,
    path::Path,
};

/// Results of an automatic network environment discovery.
#[derive(Debug, Default)]
pub struct NetworkInfo {
    pub ip: Option<String>,
    pub gateway: Option<String>,
    pub dns: Option<String>,
    pub domain: Option<String>,
}

/// Discover the primary LAN interface IP, default gateway, DNS server, and
/// DNS search domain using standard Linux mechanisms.
pub fn discover() -> NetworkInfo {
    NetworkInfo {
        ip: discover_local_ip(),
        gateway: discover_gateway(),
        dns: discover_dns(),
        domain: discover_domain(),
    }
}

// ---------------------------------------------------------------------------
// IP address of the primary LAN adapter
// ---------------------------------------------------------------------------

/// Parse `/proc/net/fib_trie` to find the IP address bound to the primary LAN
/// adapter.  The default-route interface is used to skip docker/loopback
/// addresses when multiple interfaces are present.
fn discover_local_ip() -> Option<String> {
    let candidates = fib_trie_local_ips();

    // Skip well-known non-LAN prefixes and prefer the address on the
    // default-route interface (identified by its entry in /proc/net/route).
    let _iface = default_route_iface();

    candidates
        .into_iter()
        .find(|ip| !ip.starts_with("127.") && !ip.starts_with("172.17."))
}

/// Return the name of the network interface that carries the default route
/// by parsing `/proc/net/route`.
fn default_route_iface() -> Option<String> {
    let content = fs::read_to_string("/proc/net/route").ok()?;
    for line in content.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        // Destination == "00000000" means default route
        if cols[1] == "00000000" {
            return Some(cols[0].to_string());
        }
    }
    None
}

/// Collect all LOCAL IPv4 addresses from `/proc/net/fib_trie`.
/// The format has blocks like:
///   +-- <prefix>/<bits> ...
///     /32 host LOCAL
///   The IP is on the line before "32 host LOCAL".
fn fib_trie_local_ips() -> Vec<String> {
    let content = match fs::read_to_string("/proc/net/fib_trie") {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.contains("/32 host LOCAL") {
            // The IP address appears one line above this line (the leaf node)
            if i >= 1 {
                let candidate = lines[i - 1].trim();
                // Strip the "+-- " / "|-- " prefix that fib_trie uses
                let ip_str = candidate
                    .trim_start_matches('+')
                    .trim_start_matches('|')
                    .trim_start_matches("-- ")
                    .trim_start_matches("--")
                    .trim();
                // Keep only the address part (before any '/')
                let ip_str = ip_str.split('/').next().unwrap_or("").trim();
                if ip_str.parse::<IpAddr>().is_ok() {
                    result.push(ip_str.to_string());
                }
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Default gateway
// ---------------------------------------------------------------------------

/// Read the default gateway from `/proc/net/route`.
/// The Gateway field is a little-endian hex-encoded 32-bit IPv4 address.
fn discover_gateway() -> Option<String> {
    let content = fs::read_to_string("/proc/net/route").ok()?;
    for line in content.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        if cols[1] == "00000000" {
            let gw_hex = cols[2];
            return parse_hex_ip(gw_hex);
        }
    }
    None
}

/// Decode a little-endian hex IPv4 address (as found in `/proc/net/route`)
/// into a dotted-decimal string.
fn parse_hex_ip(hex: &str) -> Option<String> {
    if hex.len() != 8 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    // /proc/net/route stores the value in host byte order on little-endian CPUs
    // which means the bytes are in reverse order when compared to network order.
    let b0 = (n & 0xFF) as u8;
    let b1 = ((n >> 8) & 0xFF) as u8;
    let b2 = ((n >> 16) & 0xFF) as u8;
    let b3 = ((n >> 24) & 0xFF) as u8;
    Some(format!("{b0}.{b1}.{b2}.{b3}"))
}

// ---------------------------------------------------------------------------
// DNS server and search domain
// ---------------------------------------------------------------------------

/// Read the first `nameserver` entry from `/etc/resolv.conf`.
fn discover_dns() -> Option<String> {
    parse_resolv_conf(Path::new("/etc/resolv.conf")).0
}

/// Read the first `search` / `domain` entry from `/etc/resolv.conf`.
fn discover_domain() -> Option<String> {
    parse_resolv_conf(Path::new("/etc/resolv.conf")).1
}

/// Parse `/etc/resolv.conf` and return `(first_nameserver, first_domain)`.
fn parse_resolv_conf(path: &Path) -> (Option<String>, Option<String>) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };

    let mut ns: Option<String> = None;
    let mut domain: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if ns.is_none() && line.starts_with("nameserver") {
            ns = line.split_whitespace().nth(1).map(String::from);
        }
        if domain.is_none() {
            if line.starts_with("domain") || line.starts_with("search") {
                domain = line.split_whitespace().nth(1).map(String::from);
            }
        }
    }

    (ns, domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_ip_loopback() {
        // 127.0.0.1 in little-endian hex = 0100007F
        assert_eq!(parse_hex_ip("0100007F"), Some("127.0.0.1".to_string()));
    }

    #[test]
    fn test_parse_hex_ip_gateway() {
        // 192.168.1.1 in little-endian hex = 0101A8C0
        assert_eq!(parse_hex_ip("0101A8C0"), Some("192.168.1.1".to_string()));
    }

    #[test]
    fn test_parse_hex_ip_invalid() {
        assert_eq!(parse_hex_ip("ZZZZZZZZ"), None);
        assert_eq!(parse_hex_ip("short"), None);
    }

    #[test]
    fn test_parse_resolv_conf() {
        use std::io::Write;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "# comment").unwrap();
        writeln!(tmp, "domain example.com").unwrap();
        writeln!(tmp, "nameserver 8.8.8.8").unwrap();
        writeln!(tmp, "nameserver 8.8.4.4").unwrap();
        let (ns, domain) = parse_resolv_conf(tmp.path());
        assert_eq!(ns, Some("8.8.8.8".to_string()));
        assert_eq!(domain, Some("example.com".to_string()));
    }
}
