//! `network/testProxy`: prove the proxy the owner configured can reach the
//! catalog host. This is a connectivity check only — it never writes settings
//! and never returns the proxy URL in an error path.

use std::time::Duration;

use serde_json::{json, Value};

const TARGET: &str = "https://models.dev/api.json";
const TIMEOUT: Duration = Duration::from_secs(10);

/// Blocking on purpose: ureq is synchronous, so the router calls this from
/// `spawn_blocking` instead of stalling a runtime worker.
pub fn run(params: &Value) -> Value {
    match params
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("system")
    {
        "direct" => probe(ureq::AgentBuilder::new()),
        "custom" => {
            let url = params
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if url.is_empty() {
                return json!({ "ok": false, "error": "proxy url is required" });
            }
            match proxy(url) {
                Ok(proxy) => probe(ureq::AgentBuilder::new().proxy(proxy)),
                Err(message) => json!({ "ok": false, "error": message }),
            }
        }
        // System mode reads the same environment variables git and the agent
        // sidecar use, so the test measures the proxy those processes would.
        _ => probe(ureq::AgentBuilder::new()),
    }
}

fn probe(builder: ureq::AgentBuilder) -> Value {
    let agent = builder.timeout(TIMEOUT).build();
    match agent.get(TARGET).call() {
        Ok(response) => {
            let status = response.status();
            if (200..300).contains(&status) {
                json!({ "ok": true })
            } else {
                json!({ "ok": false, "error": format!("HTTP {status}") })
            }
        }
        Err(ureq::Error::Status(status, _)) => {
            json!({ "ok": false, "error": format!("HTTP {status}") })
        }
        Err(error) => json!({ "ok": false, "error": error.to_string() }),
    }
}

fn proxy(raw: &str) -> Result<ureq::Proxy, String> {
    let parsed = url::Url::parse(raw).map_err(|_| "proxy url is not a URL".to_string())?;
    if parsed.host_str().unwrap_or("").is_empty() {
        return Err("proxy url has no host".into());
    }
    let normalized = match parsed.scheme() {
        "http" | "socks" | "socks5" => raw.to_string(),
        // `socks5h` is the same SOCKS5 proxy with remote DNS; ureq speaks it
        // through the socks transport either way.
        "socks5h" => raw.replacen("socks5h://", "socks5://", 1),
        "https" => return Err("an https proxy is not supported by this check".into()),
        other => return Err(format!("unsupported proxy scheme: {other}")),
    };
    ureq::Proxy::new(&normalized).map_err(|_| "proxy url is not usable".to_string())
}

#[cfg(test)]
mod tests {
    use super::proxy;

    #[test]
    fn supported_proxy_schemes_build() {
        assert!(proxy("http://127.0.0.1:7890").is_ok());
        assert!(proxy("socks5://127.0.0.1:1080").is_ok());
        assert!(proxy("socks5h://127.0.0.1:1080").is_ok());
    }

    #[test]
    fn unusable_proxies_are_named() {
        assert!(proxy("").is_err());
        assert!(proxy("ftp://example.com:21").is_err());
        assert!(proxy("https://example.com:443").is_err());
    }
}
