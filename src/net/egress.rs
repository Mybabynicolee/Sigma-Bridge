use std::fmt;

use crate::support::error::{AppError, AppResult};

pub const DEFAULT_SCHEME: &str = "socks5";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proxy {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub pass: Option<String>,
}

impl Proxy {
    pub fn parse(raw: &str) -> AppResult<Proxy> {
        let text = raw.trim();
        if text.is_empty() {
            return Err(AppError::config("the proxy line is empty"));
        }

        let (scheme, rest) = match text.split_once("://") {
            Some((scheme, rest)) => (scheme.trim().to_ascii_lowercase(), rest),
            None => (DEFAULT_SCHEME.to_string(), text),
        };
        let scheme = normalise_scheme(&scheme);

        let (user, pass, hostport) = if let Some((creds, hostport)) = rest.rsplit_once('@') {
            let (user, pass) = split_credentials(creds);
            (user, pass, hostport.to_string())
        } else {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 4 {
                let host = parts[0].trim().to_string();
                let port = parse_port(parts[1])?;
                let user = decode(parts[2].trim());
                let pass = decode(parts[3].trim());
                (Some(user), Some(pass), format!("{}:{}", host, port))
            } else {
                (None, None, rest.to_string())
            }
        };

        let (host, port) = match hostport.rsplit_once(':') {
            Some((host, port)) => (host.trim().to_string(), parse_port(port)?),
            None => return Err(AppError::config("the proxy line needs a host and a port")),
        };
        if host.is_empty() {
            return Err(AppError::config("the proxy host is missing"));
        }

        Ok(Proxy {
            scheme,
            host,
            port,
            user,
            pass,
        })
    }

    pub fn is_socks(&self) -> bool {
        matches!(
            self.scheme.as_str(),
            "socks5" | "socks5h" | "socks4" | "socks4a"
        )
    }

    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn credentials(&self) -> Option<(String, String)> {
        match (&self.user, &self.pass) {
            (Some(user), Some(pass)) if !user.is_empty() => Some((user.clone(), pass.clone())),
            (Some(user), None) if !user.is_empty() => Some((user.clone(), String::new())),
            _ => None,
        }
    }

    pub fn url(&self) -> String {
        match self.credentials() {
            Some((user, pass)) => format!(
                "{}://{}:{}@{}",
                self.scheme,
                encode(&user),
                encode(&pass),
                self.authority()
            ),
            None => format!("{}://{}", self.scheme, self.authority()),
        }
    }

    pub fn label(&self) -> String {
        let scheme = if self.scheme == "socks5" {
            "socks5"
        } else {
            self.scheme.as_str()
        };
        format!("{}://{}", scheme, self.authority())
    }
}

impl fmt::Display for Proxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

fn normalise_scheme(scheme: &str) -> String {
    match scheme {
        "socks" | "socks5" | "socks5h" => "socks5".to_string(),
        "socks4" | "socks4a" => "socks4".to_string(),
        "http" | "https" => "http".to_string(),
        other if other.is_empty() => DEFAULT_SCHEME.to_string(),
        other => other.to_string(),
    }
}

fn parse_port(raw: &str) -> AppResult<u16> {
    raw.trim()
        .parse::<u16>()
        .map_err(|_| AppError::config(format!("the proxy port {} is not a number", raw.trim())))
}

fn split_credentials(creds: &str) -> (Option<String>, Option<String>) {
    match creds.split_once(':') {
        Some((user, pass)) => (Some(decode(user.trim())), Some(decode(pass.trim()))),
        None => {
            let user = decode(creds.trim());
            if user.is_empty() {
                (None, None)
            } else {
                (Some(user), None)
            }
        }
    }
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        let keep = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_host_and_port_is_socks5() {
        let proxy = Proxy::parse("127.0.0.1:1080").unwrap();
        assert_eq!(proxy.scheme, "socks5");
        assert_eq!(proxy.host, "127.0.0.1");
        assert_eq!(proxy.port, 1080);
        assert!(proxy.credentials().is_none());
        assert!(proxy.is_socks());
    }

    #[test]
    fn the_userinfo_form_keeps_both_halves() {
        let proxy = Proxy::parse("socks5://user:pass@10.0.0.1:1081").unwrap();
        assert_eq!(proxy.host, "10.0.0.1");
        assert_eq!(proxy.port, 1081);
        assert_eq!(proxy.credentials(), Some(("user".into(), "pass".into())));
    }

    #[test]
    fn the_four_field_form_keeps_both_halves() {
        let proxy = Proxy::parse("10.0.0.2:1082:alice:secret").unwrap();
        assert_eq!(proxy.host, "10.0.0.2");
        assert_eq!(proxy.port, 1082);
        assert_eq!(proxy.credentials(), Some(("alice".into(), "secret".into())));
    }

    #[test]
    fn the_credentials_survive_a_roundtrip() {
        let proxy = Proxy::parse("socks5://user%40host:p%3Ass@10.0.0.3:1083").unwrap();
        assert_eq!(
            proxy.credentials(),
            Some(("user@host".into(), "p:ss".into()))
        );
        assert_eq!(proxy.url(), "socks5://user%40host:p%3Ass@10.0.0.3:1083");
    }

    #[test]
    fn an_empty_line_is_refused() {
        assert!(Proxy::parse("   ").is_err());
    }

    #[test]
    fn a_line_without_a_port_is_refused() {
        assert!(Proxy::parse("socks5://10.0.0.4").is_err());
    }

    #[test]
    fn a_bad_port_is_refused() {
        assert!(Proxy::parse("10.0.0.5:notaport").is_err());
    }

    #[test]
    fn the_label_hides_nothing_but_the_credentials() {
        let proxy = Proxy::parse("socks5://u:p@1.2.3.4:1080").unwrap();
        assert_eq!(proxy.label(), "socks5://1.2.3.4:1080");
        assert!(!proxy.label().contains('p'));
    }
}
