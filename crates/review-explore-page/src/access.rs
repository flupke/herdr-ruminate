use std::fmt;
use std::net::{Ipv4Addr, SocketAddr};

use axum::http::{HeaderMap, HeaderValue, header};

use crate::PageEvent;

#[derive(Clone)]
pub struct Token(String);

impl Token {
    pub fn random() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }

    pub fn chosen(token: String) -> Result<Self, String> {
        let valid = !token.is_empty()
            && token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        if valid {
            Ok(Self(token))
        } else {
            Err(format!(
                "invalid token {token:?}: use letters, digits, - and _"
            ))
        }
    }

    pub fn url(&self, address: SocketAddr) -> String {
        format!("http://{address}/?token={}", self.0)
    }

    pub fn tunnel_url(&self, host: &str) -> String {
        format!("https://{host}/?token={}", self.0)
    }

    pub fn loopback_url(&self, port: u16) -> String {
        self.url(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
    }

    pub fn matches(&self, candidate: &str) -> bool {
        let expected = self.0.as_bytes();
        let candidate = candidate.as_bytes();
        expected.len() == candidate.len()
            && expected
                .iter()
                .zip(candidate)
                .fold(0, |difference, (a, b)| difference | (a ^ b))
                == 0
    }
}

impl fmt::Display for Token {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(&self.0)
    }
}

// DNS rebinding: a foreign site can point its own name at this machine. Browsers name the
// `Origin` of a script's request to another site and of every post; a navigation, and a
// browser's report of a policy violation, may carry none.
pub struct Hosts {
    names: Vec<String>,
    scheme: Scheme,
}

#[derive(Clone, Copy)]
enum Scheme {
    Http,
    Https,
}

impl Hosts {
    pub fn loopback(port: u16) -> Self {
        Self {
            names: vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")],
            scheme: Scheme::Http,
        }
    }

    pub fn network(address: SocketAddr) -> Self {
        Self {
            names: vec![address.to_string()],
            scheme: Scheme::Http,
        }
    }

    pub fn tunnel(host: &str) -> Self {
        Self {
            names: vec![host.to_owned()],
            scheme: Scheme::Https,
        }
    }

    pub(crate) fn admit(&self, headers: &HeaderMap) -> Result<CookieScope, PageEvent> {
        let host = headers
            .get(header::HOST)
            .and_then(|host| host.to_str().ok())
            .filter(|host| self.names.iter().any(|allowed| allowed == host))
            .ok_or(PageEvent::UnknownHost)?;
        match headers.get(header::ORIGIN) {
            Some(origin) if origin.as_bytes() != self.origin(host).as_bytes() => {
                Err(PageEvent::ForeignOrigin)
            }
            _ => Ok(CookieScope::of(host)),
        }
    }

    fn origin(&self, host: &str) -> String {
        match self.scheme {
            Scheme::Http => format!("http://{host}"),
            Scheme::Https => format!("https://{host}"),
        }
    }

    pub(crate) fn socket(&self, host: &str) -> String {
        match self.scheme {
            Scheme::Http => format!("ws://{host}"),
            Scheme::Https => format!("wss://{host}"),
        }
    }
}

// Browsers keep cookies by host name, ignore the port, and send them to every port.
pub(crate) struct CookieScope {
    suffix: String,
}

impl CookieScope {
    fn of(host: &str) -> Self {
        let port = host.rsplit_once(':').map_or("80", |(_, port)| port);
        Self {
            suffix: format!("_{port}"),
        }
    }

    pub(crate) fn receive(&self, headers: &mut HeaderMap) {
        let kept: Vec<String> = headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|pair| {
                let (name, value) = pair.trim().split_once('=')?;
                Some(format!("{}={value}", name.strip_suffix(&self.suffix)?))
            })
            .collect();
        headers.remove(header::COOKIE);
        if !kept.is_empty()
            && let Ok(value) = HeaderValue::from_str(&kept.join("; "))
        {
            headers.insert(header::COOKIE, value);
        }
    }

    pub(crate) fn send(&self, headers: &mut HeaderMap) {
        let scoped: Vec<HeaderValue> = headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| {
                let (name, rest) = value.split_once('=')?;
                HeaderValue::from_str(&format!("{name}{}={rest}", self.suffix)).ok()
            })
            .collect();
        headers.remove(header::SET_COOKIE);
        for value in scoped {
            headers.append(header::SET_COOKIE, value);
        }
    }
}

pub(crate) struct TokenCookie;

impl TokenCookie {
    const NAME: &str = "explore_token";

    pub(crate) fn read(headers: &HeaderMap) -> Option<&str> {
        cookie(headers, Self::NAME)
    }

    pub(crate) fn set(token: &str) -> Option<HeaderValue> {
        HeaderValue::from_str(&format!(
            "{}={token}; Path=/; HttpOnly; SameSite=Lax",
            Self::NAME
        ))
        .ok()
    }
}

pub(crate) fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(name)?.strip_prefix('='))
}
