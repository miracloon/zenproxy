use super::{ProxyConfig, ProxyType};
use serde_json::json;

/// Parse plain-text proxy lines with a given proxy type.
///
/// Supported line formats:
/// - `host:port`
/// - `host:port:user:pass`
/// - `user:pass@host:port`
///
/// Also accepts URI lines (socks5://, http://, etc.) and delegates them to v2ray parser.
pub fn parse(content: &str, proxy_type_str: &str) -> Vec<ProxyConfig> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let line = line.trim();
            // If the line looks like a URI, delegate to the v2ray parser
            if line.contains("://") {
                return super::v2ray::parse_uri(line);
            }
            parse_plain_line(line, proxy_type_str)
        })
        .collect()
}

fn parse_plain_line(line: &str, proxy_type_str: &str) -> Option<ProxyConfig> {
    let (server, port, username, password) = if line.contains('@') {
        // user:pass@host:port
        let (userinfo, host_port) = line.split_once('@')?;
        let (user, pass) = userinfo.split_once(':')?;
        let (host, port) = parse_plain_host_port(host_port)?;
        let port: u16 = port;
        (host.to_string(), port, user.to_string(), pass.to_string())
    } else if line.starts_with('[') {
        let (host, port) = parse_plain_host_port(line)?;
        (host.to_string(), port, String::new(), String::new())
    } else {
        // Count colons to distinguish host:port from host:port:user:pass
        let parts: Vec<&str> = line.rsplitn(3, ':').collect();
        // rsplitn(3, ':') on "host:port:user:pass" won't work well since we need to handle
        // the case where host could be IPv6. Let's use a simpler approach.
        let colon_count = line.chars().filter(|&c| c == ':').count();
        if colon_count >= 3 {
            // host:port:user:pass — split from the right
            let rpos1 = line.rfind(':')?;
            let pass = &line[rpos1 + 1..];
            let rest = &line[..rpos1];
            let rpos2 = rest.rfind(':')?;
            let user = &rest[rpos2 + 1..];
            let host_port = &rest[..rpos2];
            let rpos3 = host_port.rfind(':')?;
            let port_str = &host_port[rpos3 + 1..];
            let host = &host_port[..rpos3];
            let port: u16 = port_str.parse().ok()?;
            (host.to_string(), port, user.to_string(), pass.to_string())
        } else {
            // host:port
            let _ = parts;
            let rpos = line.rfind(':')?;
            let port_str = &line[rpos + 1..];
            let host = &line[..rpos];
            let port: u16 = port_str.parse().ok()?;
            (host.to_string(), port, String::new(), String::new())
        }
    };

    if server.is_empty() || port == 0 {
        return None;
    }

    let name = format!("{server}:{port}");

    match proxy_type_str {
        "socks4" => {
            let mut outbound = json!({
                "type": "socks",
                "server": server,
                "server_port": port,
                "version": "4a",
            });
            if !username.is_empty() {
                outbound["username"] = json!(username);
                outbound["password"] = json!(password);
            }
            Some(ProxyConfig {
                name,
                proxy_type: ProxyType::Socks,
                server,
                port,
                singbox_outbound: outbound,
            })
        }
        "socks5" => {
            let mut outbound = json!({
                "type": "socks",
                "server": server,
                "server_port": port,
                "version": "5",
            });
            if !username.is_empty() {
                outbound["username"] = json!(username);
                outbound["password"] = json!(password);
            }
            Some(ProxyConfig {
                name,
                proxy_type: ProxyType::Socks,
                server,
                port,
                singbox_outbound: outbound,
            })
        }
        "http" => {
            let mut outbound = json!({
                "type": "http",
                "server": server,
                "server_port": port,
            });
            if !username.is_empty() {
                outbound["username"] = json!(username);
                outbound["password"] = json!(password);
            }
            Some(ProxyConfig {
                name,
                proxy_type: ProxyType::Http,
                server,
                port,
                singbox_outbound: outbound,
            })
        }
        "https" => {
            let mut outbound = json!({
                "type": "http",
                "server": server,
                "server_port": port,
                "tls": {
                    "enabled": true,
                    "server_name": server,
                    "insecure": true,
                },
            });
            if !username.is_empty() {
                outbound["username"] = json!(username);
                outbound["password"] = json!(password);
            }
            Some(ProxyConfig {
                name,
                proxy_type: ProxyType::Http,
                server,
                port,
                singbox_outbound: outbound,
            })
        }
        _ => None,
    }
}

fn parse_plain_host_port(input: &str) -> Option<(&str, u16)> {
    if input.starts_with('[') {
        let end_bracket = input.find(']')?;
        let host = &input[1..end_bracket];
        let port_str = input[end_bracket + 1..].strip_prefix(':')?;
        let port: u16 = port_str.parse().ok()?;
        Some((host, port))
    } else {
        let (host, port_str) = input.rsplit_once(':')?;
        let port: u16 = port_str.parse().ok()?;
        Some((host, port))
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parse_plain_supports_bracketed_ipv6_host_port() {
        let proxies = parse("[2001:db8::1]:1080", "socks5");

        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].server, "2001:db8::1");
        assert_eq!(proxies[0].port, 1080);
    }

    #[test]
    fn parse_plain_supports_bracketed_ipv6_with_userinfo() {
        let proxies = parse("user:pass@[2001:db8::1]:1080", "socks5");

        assert_eq!(proxies.len(), 1);
        assert_eq!(proxies[0].server, "2001:db8::1");
        assert_eq!(proxies[0].port, 1080);

        let username = proxies[0]
            .singbox_outbound
            .get("username")
            .and_then(|v| v.as_str());
        let password = proxies[0]
            .singbox_outbound
            .get("password")
            .and_then(|v| v.as_str());

        assert_eq!(username, Some("user"));
        assert_eq!(password, Some("pass"));
    }
}
