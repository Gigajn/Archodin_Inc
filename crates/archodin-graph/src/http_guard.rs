//! One hardened HTTP client for every outbound fetch in this crate (KR-01).
//!
//! Ronin used to fetch search-result URLs with a bare `reqwest::Client::new()`:
//! no timeout, no size limit, no URL validation, default redirects — a search
//! hit could point at the cloud metadata address (`169.254.169.254`), a
//! loopback service, or a multi-gigabyte body. The same shape of client lived
//! in Gemini and MCP. Everything in this crate now goes through this module:
//!
//! - [`validate_url`] — the strict check for URLs that come from untrusted
//!   data (search hits, tool output, model output): `https` only, a real
//!   host, no credentials in the URL, and no private, loopback, link-local,
//!   or otherwise non-routable host.
//! - [`validate_endpoint`] — the looser check for endpoints the *operator*
//!   configured (a self-hosted SearXNG, a GitHub Enterprise API, a Gemini
//!   proxy, an MCP server URL): `http(s)` with a host and no credentials.
//!   Those may be plain `http` on a private host, because the operator chose
//!   them — and the existing mockito tests point Gemini and MCP at
//!   `http://127.0.0.1` fake servers. The trust boundary is "operator-chosen
//!   vs attacker-influenced", not "public vs private".
//! - [`shared_client`] — one client for the whole crate: 30 s total timeout,
//!   at most 5 redirects, and a redirect is followed only when its target
//!   passes the strict check or stays on the same origin as the request that
//!   started the chain (so a redirect can never smuggle a request from a
//!   public URL to the metadata address).
//! - [`read_capped`] — reads a response body up to [`MAX_RESPONSE_BYTES`]
//!   (2 MiB) and errors instead of growing without bound.
//!
//! Residual risk, stated plainly: a *hostname* is not resolved here, so a
//! public name that resolves to a private address (DNS rebinding / a
//! compromised resolver) is not caught — only IP literals and the
//! `localhost` names are. Closing that needs resolving first and pinning the
//! connection to a validated address, which `reqwest` cannot express through
//! this client alone; it is an open follow-up, not something this module
//! claims to do.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::{redirect, Url};

/// The most bytes of a response body that are read (2 MiB).
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Total timeout of one request: connect, redirects and reading the body.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The most redirects one request follows.
pub const MAX_REDIRECTS: usize = 5;

/// Why a URL was refused or a response could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HttpGuardError {
    /// The URL is not one this crate will fetch.
    #[error("refused URL {0}: {1}")]
    BlockedUrl(String, &'static str),
    /// The response body grew past [`MAX_RESPONSE_BYTES`].
    #[error("response exceeded the {limit} byte limit")]
    TooLarge { limit: usize },
    /// The request itself failed.
    #[error("http transport error: {0}")]
    Transport(String),
}

/// Whether `ip` is one this crate must never be sent a request: loopback,
/// private, link-local, CGNAT, documentation, benchmarking, multicast,
/// reserved, or unspecified. Written out by hand (octets/segments) instead
/// of the newer `std` helpers so the check does not depend on the toolchain.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_ipv4(v4),
        IpAddr::V6(v6) => is_blocked_ipv6(v6),
    }
}

fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 0 // 0.0.0.0/8 ("this network")
        || o[0] == 127 // 127/8 loopback
        || o[0] == 10 // 10/8 private
        || (o[0] == 172 && (16..=31).contains(&o[1])) // 172.16/12 private
        || (o[0] == 192 && o[1] == 168) // 192.168/16 private
        || (o[0] == 169 && o[1] == 254) // 169.254/16 link-local (incl. cloud metadata)
        || (o[0] == 100 && (64..=127).contains(&o[1])) // 100.64/10 CGNAT, not routable
        || (o[0] == 192 && o[1] == 0 && o[2] == 2) // 192.0.2/24 TEST-NET-1
        || (o[0] == 198 && o[1] == 51 && o[2] == 100) // 198.51.100/24 TEST-NET-2
        || (o[0] == 203 && o[1] == 0 && o[2] == 113) // 203.0.113/24 TEST-NET-3
        || (o[0] == 198 && (18..=19).contains(&o[1])) // 198.18/15 benchmarking
        || o[0] >= 224 // 224/4 multicast + 240/4 reserved (incl. 255.255.255.255)
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return true; // ::1, ::
    }
    let s = ip.segments();
    if (s[0] & 0xffc0) == 0xfe80 {
        return true; // fe80::/10 link-local
    }
    if (s[0] & 0xfe00) == 0xfc00 {
        return true; // fc00::/7 unique-local
    }
    if (s[0] & 0xff00) == 0xff00 {
        return true; // ff00::/8 multicast
    }
    if s[0] == 0x2001 && s[1] == 0x0db8 {
        return true; // 2001:db8::/32 documentation
    }
    // An IPv4-mapped (`::ffff:127.0.0.1`) or IPv4-compatible (`::127.0.0.1`)
    // address is judged by the IPv4 it carries.
    if s[0] == 0
        && s[1] == 0
        && s[2] == 0
        && s[3] == 0
        && s[4] == 0
        && (s[5] == 0 || s[5] == 0xffff)
    {
        let v4 = Ipv4Addr::new((s[6] >> 8) as u8, s[6] as u8, (s[7] >> 8) as u8, s[7] as u8);
        return is_blocked_ipv4(v4);
    }
    false
}

/// The strict check for a URL from untrusted data: `https` only, a real host,
/// no credentials, and no blocked host (IP literal or `localhost` name).
pub fn validate_url(raw: &str) -> Result<Url, HttpGuardError> {
    let url = parse(raw)?;
    if url.scheme() != "https" {
        return Err(HttpGuardError::BlockedUrl(
            raw.to_string(),
            "only https URLs are fetched",
        ));
    }
    check_host(&url, raw)?;
    Ok(url)
}

/// The check for an endpoint the operator configured: `http` or `https`, a
/// real host, no credentials. A private or loopback host is allowed here —
/// the operator chose it — but the URL must still be well-formed.
pub fn validate_endpoint(raw: &str) -> Result<Url, HttpGuardError> {
    let url = parse(raw)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(HttpGuardError::BlockedUrl(
            raw.to_string(),
            "only http(s) endpoints are allowed",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| HttpGuardError::BlockedUrl(raw.to_string(), "endpoint has no host"))?;
    if host.is_empty() {
        return Err(HttpGuardError::BlockedUrl(
            raw.to_string(),
            "endpoint has no host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(HttpGuardError::BlockedUrl(
            raw.to_string(),
            "URLs with credentials are not allowed",
        ));
    }
    Ok(url)
}

/// Whether two URLs share an origin (scheme, host, port): the same service.
/// A hit URL on the same origin as the operator-configured search endpoint
/// is served by the operator's own service, so it may use the operator
/// check; a hit on any other origin is attacker-influenced and stays strict.
/// Unparseable URLs are never the same origin.
pub fn same_origin(a: &str, b: &str) -> bool {
    let (Ok(a), Ok(b)) = (Url::parse(a.trim()), Url::parse(b.trim())) else {
        return false;
    };
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

fn parse(raw: &str) -> Result<Url, HttpGuardError> {
    Url::parse(raw.trim())
        .map_err(|_| HttpGuardError::BlockedUrl(raw.to_string(), "not a valid URL"))
}

fn check_host(url: &Url, raw: &str) -> Result<(), HttpGuardError> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(HttpGuardError::BlockedUrl(
            raw.to_string(),
            "URLs with credentials are not allowed",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| HttpGuardError::BlockedUrl(raw.to_string(), "URL has no host"))?;
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") || lower.ends_with(".local") {
        return Err(HttpGuardError::BlockedUrl(
            raw.to_string(),
            "localhost hosts are not allowed",
        ));
    }
    // `host_str()` keeps the brackets on an IPv6 literal (`[::1]`), and
    // `IpAddr` does not parse brackets, so strip them before the check;
    // without this, every IPv6 literal silently skipped the block list.
    let host_ip = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host_ip.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(HttpGuardError::BlockedUrl(
                raw.to_string(),
                "host is a private, loopback or link-local address",
            ));
        }
    }
    Ok(())
}

/// Whether `target` may be followed as a redirect from a request whose first
/// URL was `previous[0]`: either it passes the strict check, or it stays on
/// the same origin (scheme, host, port) as the request that started the
/// chain. A downgrade (`https` to `http`) or a hop to another host never
/// counts as same-origin.
pub fn redirect_allowed(previous: &[Url], target: &Url) -> bool {
    if previous.len() > MAX_REDIRECTS {
        return false;
    }
    if validate_url(target.as_str()).is_ok() {
        return true;
    }
    match previous.first() {
        Some(first) => {
            target.scheme() == first.scheme()
                && target.host_str() == first.host_str()
                && target.port_or_known_default() == first.port_or_known_default()
        }
        None => false,
    }
}

/// The one client this crate fetches with: a 30 s total timeout and the
/// redirect policy above. Cloning it is cheap (it is reference-counted).
///
/// Building the client returns an error instead of panicking: the settings
/// are fixed constants, so in practice it always builds — but the failure is
/// a value, not a panic, because every caller is on a job path (KR-06). A
/// failed build is cached like a successful one: fixed settings either
/// always build or never do.
pub fn shared_client() -> Result<reqwest::Client, HttpGuardError> {
    static CLIENT: OnceLock<Result<reqwest::Client, HttpGuardError>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            let policy = redirect::Policy::custom(|attempt: redirect::Attempt| {
                if redirect_allowed(attempt.previous(), attempt.url()) {
                    attempt.follow()
                } else {
                    let url = attempt.url().clone();
                    attempt.error(format!("redirect to {url} is not allowed"))
                }
            });
            reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .redirect(policy)
                .build()
                .map_err(|e| HttpGuardError::Transport(e.to_string()))
        })
        .clone()
}

/// Adds `chunk` to `body`, failing instead of growing past
/// [`MAX_RESPONSE_BYTES`]. Pure, so the limit is testable without a server.
pub fn push_limited(body: &mut Vec<u8>, chunk: &[u8]) -> Result<(), HttpGuardError> {
    if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
        return Err(HttpGuardError::TooLarge {
            limit: MAX_RESPONSE_BYTES,
        });
    }
    body.extend_from_slice(chunk);
    Ok(())
}

/// Reads a response body, stopping with [`HttpGuardError::TooLarge`] instead
/// of growing without bound. Needs the `stream` feature of `reqwest`, which
/// this crate declares in its own `Cargo.toml` (not borrowed from another
/// crate's feature unification).
pub async fn read_capped(mut response: reqwest::Response) -> Result<Vec<u8>, HttpGuardError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| HttpGuardError::Transport(e.to_string()))?
    {
        push_limited(&mut body, &chunk)?;
    }
    Ok(body)
}

/// The body as text, tolerating invalid UTF-8 the way a lossy decode does.
pub fn text_capped(body: Vec<u8>) -> String {
    String::from_utf8_lossy(&body).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked(raw: &str) {
        assert!(
            matches!(validate_url(raw), Err(HttpGuardError::BlockedUrl(..))),
            "{raw} must be refused"
        );
    }

    #[test]
    fn limits_are_the_audited_values() {
        assert_eq!(MAX_RESPONSE_BYTES, 2 * 1024 * 1024);
        assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(30));
        assert_eq!(MAX_REDIRECTS, 5);
    }

    #[test]
    fn plain_http_is_refused() {
        blocked("http://example.com/skill.md");
        blocked("http://api.github.com/repos/x/contents/SKILL.md");
    }

    #[test]
    fn cloud_metadata_address_is_refused() {
        blocked("http://169.254.169.254/latest/meta-data/");
        blocked("https://169.254.169.254/latest/meta-data/");
        blocked("https://169.254.169.254/");
    }

    #[test]
    fn loopback_and_private_hosts_are_refused() {
        for raw in [
            "http://localhost/",
            "https://localhost/",
            "https://localhost:8080/search",
            "https://localhost./",
            "https://sub.localhost./",
            "https://printer.local./",
            "https://sub.localhost/",
            "https://printer.local/",
            "http://127.0.0.1/",
            "https://127.0.0.1:8443/",
            "https://127.1.2.3/",
            "http://10.0.0.1/",
            "https://10.255.255.255/",
            "https://172.16.0.5/",
            "https://172.31.255.255/",
            "https://192.168.1.1/",
            "https://0.0.0.0/",
            "https://[::1]/",
            "https://[fe80::1]/",
            "https://[fc00::1]/",
            "https://[::ffff:127.0.0.1]/",
            "https://[::ffff:10.0.0.1]/",
            "https://100.64.0.1/",
            "https://224.0.0.1/",
        ] {
            blocked(raw);
        }
    }

    #[test]
    fn credentials_missing_host_and_other_schemes_are_refused() {
        blocked("https://user:pass@example.com/skill.md");
        blocked("https://user@example.com/");
        blocked("https://");
        blocked("ftp://example.com/skill.md");
        blocked("file:///etc/passwd");
        blocked("not a url");
        blocked("");
    }

    #[test]
    fn public_https_urls_are_accepted() {
        for raw in [
            "https://api.github.com/repos/x/contents/SKILL.md",
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent",
            "https://example.com:8443/path?q=1",
        ] {
            let url = validate_url(raw).unwrap_or_else(|e| panic!("{raw} must be accepted: {e}"));
            assert!(url.host_str().is_some_and(|h| !h.is_empty()));
        }
    }

    #[test]
    fn operator_endpoints_may_be_plain_http_on_loopback() {
        for raw in [
            "http://127.0.0.1:1234/mcp",
            "http://localhost:8080/search",
            "https://api.github.com",
        ] {
            assert!(
                validate_endpoint(raw).is_ok(),
                "{raw} must be accepted as an operator endpoint"
            );
        }
    }

    #[test]
    fn operator_endpoints_still_reject_bad_shapes() {
        for raw in [
            "ftp://example.com/mcp",
            "file:///etc/passwd",
            "https://user:pass@example.com/mcp",
            "https://",
            "not a url",
        ] {
            assert!(
                matches!(validate_endpoint(raw), Err(HttpGuardError::BlockedUrl(..))),
                "{raw} must be refused even as an operator endpoint"
            );
        }
    }

    #[test]
    fn blocked_ip_ranges_cover_the_private_space() {
        for raw in [
            "10.0.0.0",
            "172.16.0.0",
            "172.31.255.255",
            "192.168.0.0",
            "169.254.0.1",
            "127.0.0.1",
            "0.0.0.0",
            "100.64.0.0",
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "198.18.0.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
        ] {
            let ip: IpAddr = raw.parse().unwrap();
            assert!(is_blocked_ip(ip), "{raw} must be blocked");
        }
        for raw in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "172.15.0.1",
            "172.32.0.1",
        ] {
            let ip: IpAddr = raw.parse().unwrap();
            assert!(!is_blocked_ip(ip), "{raw} must not be blocked");
        }
        for raw in [
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd00::1",
            "ff02::1",
            "2001:db8::1",
        ] {
            let ip: IpAddr = raw.parse().unwrap();
            assert!(is_blocked_ip(ip), "{raw} must be blocked");
        }
        let public_v6: IpAddr = "2606:4700:4700::1111".parse().unwrap();
        assert!(!is_blocked_ip(public_v6));
    }

    #[test]
    fn response_over_the_limit_is_refused_before_it_grows() {
        let mut body = vec![0u8; MAX_RESPONSE_BYTES - 1];
        assert!(push_limited(&mut body, &[0]).is_ok());
        assert_eq!(body.len(), MAX_RESPONSE_BYTES);
        assert!(matches!(
            push_limited(&mut body, &[0]),
            Err(HttpGuardError::TooLarge { .. })
        ));
        let mut fresh = Vec::new();
        assert!(matches!(
            push_limited(&mut fresh, &[0; MAX_RESPONSE_BYTES + 1]),
            Err(HttpGuardError::TooLarge { .. })
        ));
        assert!(fresh.is_empty());
    }

    #[test]
    fn redirect_to_public_https_is_allowed_but_not_to_metadata() {
        let first = Url::parse("https://example.com/a").unwrap();
        let previous = [first];
        assert!(redirect_allowed(
            &previous,
            &Url::parse("https://example.com/b").unwrap()
        ));
        assert!(!redirect_allowed(
            &previous,
            &Url::parse("https://169.254.169.254/").unwrap()
        ));
        assert!(!redirect_allowed(
            &previous,
            &Url::parse("http://example.com/b").unwrap()
        ));
    }

    #[test]
    fn redirect_may_stay_on_the_same_origin_only() {
        let first = Url::parse("http://127.0.0.1:9/a").unwrap();
        let previous = [first];
        assert!(redirect_allowed(
            &previous,
            &Url::parse("http://127.0.0.1:9/b").unwrap()
        ));
        assert!(!redirect_allowed(
            &previous,
            &Url::parse("http://127.0.0.1:10/b").unwrap()
        ));
        assert!(!redirect_allowed(
            &previous,
            &Url::parse("https://169.254.169.254/").unwrap()
        ));
    }

    #[test]
    fn too_many_redirects_are_refused() {
        let previous: Vec<Url> = (0..=MAX_REDIRECTS)
            .map(|i| Url::parse(&format!("https://example.com/{i}")).unwrap())
            .collect();
        assert!(!redirect_allowed(
            &previous,
            &Url::parse("https://example.com/next").unwrap()
        ));
    }

    #[test]
    fn the_shared_client_builds() {
        assert!(
            shared_client().is_ok(),
            "the shared client must build from fixed settings"
        );
    }
}
