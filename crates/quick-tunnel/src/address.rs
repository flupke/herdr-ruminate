const DOMAIN: &str = ".trycloudflare.com";

// cloudflared names api.trycloudflare.com, the service that makes quick tunnels, when it cannot reach it.
pub(crate) fn quick_tunnel_host(line: &str) -> Option<&str> {
    line.match_indices("https://").find_map(|(start, scheme)| {
        let rest = &line[start + scheme.len()..];
        let end = rest
            .find(|character: char| {
                !(character.is_ascii_alphanumeric() || character == '-' || character == '.')
            })
            .unwrap_or(rest.len());
        let host = &rest[..end];
        let label = host.strip_suffix(DOMAIN)?;
        let valid = !label.is_empty()
            && label != "api"
            && !label.contains('.')
            && !label.starts_with('-')
            && !label.ends_with('-');
        valid.then_some(host)
    })
}

#[cfg(test)]
#[path = "address.tests.rs"]
mod tests;
