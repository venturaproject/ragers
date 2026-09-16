//! Concurrent webhook fan-out — POST the same payload to many URLs at once,
//! each with its own timeout and retry/backoff, reporting per-target
//! success/failure. The point is showing what Tokio buys you here that a
//! synchronous, one-request-per-worker runtime (PHP-FPM) fundamentally
//! can't: hundreds of in-flight requests on a handful of OS threads, each
//! one independently timed out and retried, with the whole batch bounded by
//! the slowest *surviving* target rather than the sum of all of them.

use std::net::IpAddr;
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use reqwest::Url;
use serde::Serialize;

/// Hard ceiling on how many targets one call accepts — this fans out real
/// outbound HTTP requests, so an unbounded list is a way to turn one caller
/// into a lot of egress traffic (or a DDoS floor aimed at someone else).
pub const MAX_TARGETS: usize = 200;
/// How many requests are in flight at once, regardless of how many targets
/// were requested — caps this server's own outbound connection/thread
/// pressure independent of the caller's input size.
pub const MAX_CONCURRENCY: usize = 50;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DispatchResult {
    pub url: String,
    pub ok: bool,
    pub http_status: Option<u16>,
    pub attempts: u32,
    pub error: Option<String>,
    pub elapsed_ms: u128,
}

/// Reject the obvious SSRF cases — a literal loopback/private/link-local IP,
/// or one of this deployment's own internal service names, so a caller can't
/// point the dispatcher at `http://postgres:5432` or a cloud metadata
/// endpoint and use this server as a network proxy into places it shouldn't
/// reach. This is a literal-address check, not a guarantee: a hostname that
/// *resolves* to a private address (DNS rebinding) is not caught here, since
/// that needs control over the connection at the point it's made, not just
/// the URL string — reqwest doesn't expose that without a custom connector.
/// Treat this as reducing the obvious cases, not a complete SSRF defense.
fn reject_unsafe_target(url: &Url) -> Result<(), String> {
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(format!("unsupported scheme: {other}")),
    }
    let host = url.host_str().ok_or_else(|| "missing host".to_string())?;

    const BLOCKED_HOSTNAMES: &[&str] = &[
        "localhost",
        "postgres",
        "redis",
        "qdrant",
        "erp-db",
        "api",
        "nginx",
        "frontend",
    ];
    if BLOCKED_HOSTNAMES.contains(&host.to_lowercase().as_str()) {
        return Err(format!("target host '{host}' is not allowed"));
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        let blocked = match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_broadcast()
                    || v4.is_unspecified()
            }
            IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xfe00) == 0xfc00,
        };
        if blocked {
            return Err(format!("target address '{ip}' is not allowed"));
        }
    }
    Ok(())
}

async fn send_with_retry(
    client: &reqwest::Client,
    url: String,
    payload: &serde_json::Value,
    timeout: Duration,
    max_retries: u32,
) -> DispatchResult {
    let started = Instant::now();

    let parsed = match Url::parse(&url) {
        Ok(u) => u,
        Err(e) => {
            return DispatchResult {
                url,
                ok: false,
                http_status: None,
                attempts: 0,
                error: Some(format!("invalid URL: {e}")),
                elapsed_ms: started.elapsed().as_millis(),
            };
        }
    };
    if let Err(reason) = reject_unsafe_target(&parsed) {
        return DispatchResult {
            url,
            ok: false,
            http_status: None,
            attempts: 0,
            error: Some(reason),
            elapsed_ms: started.elapsed().as_millis(),
        };
    }

    let mut last_status: Option<u16> = None;
    let mut last_error: Option<String> = None;

    for attempt in 0..=max_retries {
        if attempt > 0 {
            let backoff = Duration::from_millis(200 * 2u64.saturating_pow(attempt - 1));
            tokio::time::sleep(backoff).await;
        }

        match client
            .post(parsed.clone())
            .timeout(timeout)
            .json(payload)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                last_status = Some(status.as_u16());
                if status.is_success() {
                    return DispatchResult {
                        url,
                        ok: true,
                        http_status: Some(status.as_u16()),
                        attempts: attempt + 1,
                        error: None,
                        elapsed_ms: started.elapsed().as_millis(),
                    };
                }
                last_error = Some(format!("upstream returned {status}"));
            }
            Err(e) => last_error = Some(e.to_string()),
        }
    }

    DispatchResult {
        url,
        ok: false,
        http_status: last_status,
        attempts: max_retries + 1,
        error: last_error,
        elapsed_ms: started.elapsed().as_millis(),
    }
}

/// Fan out `payload` to every target concurrently (bounded by
/// [`MAX_CONCURRENCY`]), each with up to `max_retries` retries on a non-2xx
/// response or a transport error, exponential backoff between attempts
/// (200ms, 400ms, 800ms, ...). Order of the results matches `targets`.
pub async fn dispatch(
    client: &reqwest::Client,
    targets: Vec<String>,
    payload: &serde_json::Value,
    timeout: Duration,
    max_retries: u32,
) -> Vec<DispatchResult> {
    stream::iter(targets)
        .map(|url| {
            let client = client.clone();
            async move { send_with_retry(&client, url, payload, timeout, max_retries).await }
        })
        .buffer_unordered(MAX_CONCURRENCY)
        .collect()
        .await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rejects_loopback_and_private_addresses() {
        for url in [
            "http://127.0.0.1/hook",
            "http://localhost/hook",
            "http://10.0.0.5/hook",
            "http://192.168.1.1/hook",
            "http://169.254.169.254/latest/meta-data",
            "http://postgres:5432/",
        ] {
            let parsed = Url::parse(url).unwrap();
            assert!(
                reject_unsafe_target(&parsed).is_err(),
                "expected {url} to be rejected"
            );
        }
    }

    #[test]
    fn accepts_a_plain_public_looking_host() {
        let parsed = Url::parse("https://example.com/webhook").unwrap();
        assert!(reject_unsafe_target(&parsed).is_ok());
    }

    #[test]
    fn rejects_non_http_schemes() {
        let parsed = Url::parse("file:///etc/passwd").unwrap();
        assert!(reject_unsafe_target(&parsed).is_err());
    }
}
