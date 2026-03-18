use std::{
    fs,
    path::Path,
    process::Command,
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
/// DNS search domain using standard Linux tools.
pub fn discover() -> NetworkInfo {
    let (iface, gateway) = discover_default_route();
    let ip = iface.as_ref().and_then(|i| discover_iface_ip(i));
    let (dns, domain) = discover_dns_info(iface.as_deref());

    NetworkInfo {
        ip,
        gateway,
        dns,
        domain,
    }
}

// ---------------------------------------------------------------------------
// Default route and gateway via `ip route`
// ---------------------------------------------------------------------------

/// Parse `ip route` output to find default route interface and gateway.
/// Returns (interface_name, gateway_ip).
fn discover_default_route() -> (Option<String>, Option<String>) {
    let output = Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok();

    let stdout = match output {
        Some(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return (None, None),
    };

    // Format: "default via 192.168.3.1 dev enp5s0 proto dhcp src 192.168.3.32 metric 100"
    let mut gateway = None;
    let mut iface = None;

    for line in stdout.lines() {
        if !line.starts_with("default") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        for (i, part) in parts.iter().enumerate() {
            if *part == "via" && i + 1 < parts.len() {
                gateway = Some(parts[i + 1].to_string());
            }
            if *part == "dev" && i + 1 < parts.len() {
                iface = Some(parts[i + 1].to_string());
            }
        }
        // Use first default route
        if gateway.is_some() || iface.is_some() {
            break;
        }
    }

    (iface, gateway)
}

// ---------------------------------------------------------------------------
// IP address of interface via `ip addr`
// ---------------------------------------------------------------------------

/// Get the IPv4 address of a specific interface using `ip addr show`.
fn discover_iface_ip(iface: &str) -> Option<String> {
    let output = Command::new("ip")
        .args(["-4", "addr", "show", iface])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Format: "    inet 192.168.3.32/24 brd 192.168.3.255 scope global dynamic enp5s0"
    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("inet ") {
            // Extract IP from "inet 192.168.3.32/24 ..."
            let ip_cidr = line.split_whitespace().nth(1)?;
            let ip = ip_cidr.split('/').next()?;
            return Some(ip.to_string());
        }
    }

    None
}

// ---------------------------------------------------------------------------
// DNS server and domain
// ---------------------------------------------------------------------------

/// Discover DNS server and search domain.
/// Tries multiple methods in order of preference:
/// 1. resolvectl (systemd-resolved) - if systemd-resolved is active
/// 2. nmcli (NetworkManager) - common on desktop Linux
/// 3. /run/systemd/resolve/resolv.conf - real DNS when using systemd stub
/// 4. /etc/resolv.conf - classic fallback
fn discover_dns_info(default_iface: Option<&str>) -> (Option<String>, Option<String>) {
    // Check if systemd-resolved is being used (stub resolver in resolv.conf)
    let uses_systemd_resolved = is_systemd_resolved_active();

    // 1. Try resolvectl if systemd-resolved is active
    if uses_systemd_resolved {
        if let Some(iface) = default_iface {
            if let Some((dns, domain)) = try_resolvectl(iface) {
                if dns.is_some() {
                    return (dns, domain);
                }
            }
        }
    }

    // 2. Try nmcli (NetworkManager)
    if let Some(iface) = default_iface {
        if let Some((dns, domain)) = try_nmcli(iface) {
            if dns.is_some() {
                return (dns, domain);
            }
        }
    }

    // 3. Try systemd-resolved's upstream resolv.conf (not the stub)
    let systemd_resolv = Path::new("/run/systemd/resolve/resolv.conf");
    if systemd_resolv.exists() {
        let (dns, domain) = parse_resolv_conf(systemd_resolv);
        if dns.is_some() {
            return (dns, domain);
        }
    }

    // 4. Fallback to /etc/resolv.conf
    parse_resolv_conf(Path::new("/etc/resolv.conf"))
}

/// Check if systemd-resolved is active by looking for the stub resolver.
fn is_systemd_resolved_active() -> bool {
    // Check if resolv.conf points to the stub resolver
    if let Ok(content) = fs::read_to_string("/etc/resolv.conf") {
        if content.contains("127.0.0.53") {
            return true;
        }
    }
    // Also check if the runtime directory exists
    Path::new("/run/systemd/resolve").exists()
}

/// Try to get DNS info using resolvectl (systemd-resolved).
fn try_resolvectl(iface: &str) -> Option<(Option<String>, Option<String>)> {
    let output = Command::new("resolvectl")
        .args(["status", iface])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut dns = None;
    let mut domain = None;

    for line in stdout.lines() {
        let line = line.trim();

        // "Current DNS Server: 192.168.0.13" or "DNS Servers: 192.168.0.13 192.168.0.8"
        if dns.is_none() {
            if line.starts_with("Current DNS Server:") {
                dns = line.split(':').nth(1).map(|s| s.trim().to_string());
            } else if line.starts_with("DNS Servers:") {
                dns = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .map(|s| s.to_string());
            }
        }

        // "DNS Domain: corp.local" or "Search Domains: corp.local"
        if domain.is_none() {
            if line.starts_with("DNS Domain:") || line.starts_with("Search Domains:") {
                let d = line.split(':').nth(1).map(|s| s.trim().to_string());
                if is_valid_domain(&d) {
                    domain = d;
                }
            }
        }
    }

    Some((dns, domain))
}

/// Try to get DNS info using nmcli (NetworkManager).
fn try_nmcli(iface: &str) -> Option<(Option<String>, Option<String>)> {
    // Get DNS servers
    let dns_output = Command::new("nmcli")
        .args(["-t", "-f", "IP4.DNS", "device", "show", iface])
        .output()
        .ok()?;

    let mut dns = None;
    let mut domain = None;

    if dns_output.status.success() {
        let stdout = String::from_utf8_lossy(&dns_output.stdout);
        // Format: "IP4.DNS[1]:192.168.0.13"
        for line in stdout.lines() {
            if line.starts_with("IP4.DNS") {
                if let Some(server) = line.split(':').nth(1) {
                    let server = server.trim();
                    if !server.is_empty() && server != "127.0.0.53" {
                        dns = Some(server.to_string());
                        break;
                    }
                }
            }
        }
    }

    // Get search domain
    let domain_output = Command::new("nmcli")
        .args(["-t", "-f", "IP4.DOMAIN", "device", "show", iface])
        .output()
        .ok();

    if let Some(output) = domain_output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Format: "IP4.DOMAIN[1]:corp.local"
            for line in stdout.lines() {
                if line.starts_with("IP4.DOMAIN") {
                    if let Some(d) = line.split(':').nth(1) {
                        let d = Some(d.trim().to_string());
                        if is_valid_domain(&d) {
                            domain = d;
                            break;
                        }
                    }
                }
            }
        }
    }

    if dns.is_some() || domain.is_some() {
        Some((dns, domain))
    } else {
        None
    }
}

/// Check if a domain string is valid (not empty, not ".", not "(none)").
fn is_valid_domain(d: &Option<String>) -> bool {
    d.as_ref()
        .map(|s| !s.is_empty() && s != "." && s != "(none)")
        .unwrap_or(false)
}

/// Parse a resolv.conf file and return `(first_nameserver, first_domain)`.
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
            let server = line.split_whitespace().nth(1).map(String::from);
            // Skip systemd-resolved stub and localhost
            if server
                .as_ref()
                .map(|s| !s.starts_with("127."))
                .unwrap_or(false)
            {
                ns = server;
            }
        }
        if domain.is_none() {
            if line.starts_with("domain") || line.starts_with("search") {
                let d = line.split_whitespace().nth(1).map(String::from);
                if is_valid_domain(&d) {
                    domain = d;
                }
            }
        }
    }

    (ns, domain)
}

#[cfg(test)]
mod tests {
    use super::*;

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
