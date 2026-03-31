use anyhow::{Context, Result};
use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::debug;

const PORT_RANGE_START: u16 = 18484;
const PORT_RANGE_END: u16 = 18489;

/// A local HTTP server that waits for an OAuth callback.
///
/// The server starts accepting HTTP requests immediately on `bind()` so that
/// the callback URL is reachable before the browser process is launched (avoids
/// a deadlock when the browser launcher blocks until exit, e.g. `open::with`).
pub struct CallbackServer {
    pub port: u16,
    rx: tokio::sync::oneshot::Receiver<CallbackResult>,
    handle: tokio::task::JoinHandle<()>,
}

/// The result extracted from the OAuth callback query parameters.
#[derive(Debug, Clone)]
pub struct CallbackResult {
    pub code: String,
    pub state: String,
}

impl CallbackServer {
    /// Bind to a specific port or auto-select from the default range (18484-18489).
    /// Immediately starts serving so the callback URL is ready before the browser opens.
    pub async fn bind(port: Option<u16>) -> Result<Self> {
        let listener = if let Some(p) = port {
            TcpListener::bind(("127.0.0.1", p))
                .await
                .with_context(|| format!("Failed to bind to port {}", p))?
        } else {
            let mut bound = None;
            for p in PORT_RANGE_START..=PORT_RANGE_END {
                match TcpListener::bind(("127.0.0.1", p)).await {
                    Ok(listener) => {
                        debug!("bound callback server to port {}", p);
                        bound = Some(listener);
                        break;
                    }
                    Err(_) => continue,
                }
            }
            bound.ok_or_else(|| {
                anyhow::anyhow!(
                    "No available ports in range {}-{}",
                    PORT_RANGE_START,
                    PORT_RANGE_END
                )
            })?
        };

        let bound_port = listener.local_addr()?.port();

        let (tx, rx) = tokio::sync::oneshot::channel::<CallbackResult>();
        let tx = Arc::new(Mutex::new(Some(tx)));

        let tx_clone = Arc::clone(&tx);
        let app = Router::new().route(
            "/callback",
            get(move |Query(params): Query<HashMap<String, String>>| {
                let tx = Arc::clone(&tx_clone);
                async move {
                    // Prefer connect_code (Connected Accounts), fall back to code (PKCE)
                    let code = params
                        .get("connect_code")
                        .or_else(|| params.get("code"))
                        .cloned()
                        .unwrap_or_default();
                    let state = params.get("state").cloned().unwrap_or_default();

                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(CallbackResult { code, state });
                    }

                    Html(html_page("Connected", "You can close this window."))
                }
            }),
        );

        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        Ok(Self {
            port: bound_port,
            rx,
            handle,
        })
    }

    /// Wait for the OAuth callback. The server is already accepting requests;
    /// this simply awaits the result and then shuts down.
    /// Times out after 5 minutes to prevent indefinite hangs.
    pub async fn wait(self) -> Result<CallbackResult> {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            self.rx,
        )
        .await
        .map_err(|_| anyhow::anyhow!("OAuth callback timed out after 5 minutes. Please retry."))?
        .context("Callback channel closed without receiving a result")?;

        self.handle.abort();
        Ok(result)
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Generate a simple HTML page for the callback response shown in the browser.
pub(crate) fn html_page(title: &str, message: &str) -> String {
    let title = html_escape(title);
    let message = html_escape(message);
    format!(
        r#"<!DOCTYPE html>
<html><head><title>{title}</title></head>
<body style="font-family: sans-serif; text-align: center; margin-top: 50px;">
  <h1>{title}</h1>
  <p>{message}</p>
</body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn callback_extracts_code_and_state() {
        let server = CallbackServer::bind(None).await.unwrap();
        let port = server.port;

        let client = reqwest::Client::new();
        let url = format!(
            "http://127.0.0.1:{}/callback?code=test_code_123&state=test_state_456",
            port
        );
        let _ = client.get(&url).send().await.unwrap();

        let result = server.wait().await.unwrap();
        assert_eq!(result.code, "test_code_123");
        assert_eq!(result.state, "test_state_456");
    }

    #[tokio::test]
    async fn callback_prefers_connect_code_over_code() {
        let server = CallbackServer::bind(None).await.unwrap();
        let port = server.port;

        let client = reqwest::Client::new();
        let url = format!(
            "http://127.0.0.1:{}/callback?code=regular_code&connect_code=connect_789&state=s1",
            port
        );
        let _ = client.get(&url).send().await.unwrap();

        let result = server.wait().await.unwrap();
        assert_eq!(result.code, "connect_789");
        assert_eq!(result.state, "s1");
    }

    #[tokio::test]
    async fn callback_missing_code_returns_empty() {
        let server = CallbackServer::bind(None).await.unwrap();
        let port = server.port;

        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/callback?state=only_state", port);
        let _ = client.get(&url).send().await.unwrap();

        let result = server.wait().await.unwrap();
        assert_eq!(result.code, "");
        assert_eq!(result.state, "only_state");
    }

    #[tokio::test]
    async fn callback_binds_to_specific_port() {
        // Find a free port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let server = CallbackServer::bind(Some(port)).await.unwrap();
        assert_eq!(server.port, port);
        server.handle.abort();
    }

    #[tokio::test]
    async fn callback_auto_selects_port_in_range() {
        let server = CallbackServer::bind(None).await.unwrap();
        assert!(
            server.port >= PORT_RANGE_START && server.port <= PORT_RANGE_END,
            "port {} should be in range {}-{}",
            server.port,
            PORT_RANGE_START,
            PORT_RANGE_END
        );
        server.handle.abort();
    }

    #[test]
    fn html_escape_encodes_special_chars() {
        assert_eq!(html_escape("<script>alert(1)</script>"), "&lt;script&gt;alert(1)&lt;/script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("say \"hello\""), "say &quot;hello&quot;");
        assert_eq!(html_escape("plain text"), "plain text");
    }

    #[test]
    fn html_page_escapes_parameters() {
        let page = html_page("<evil>", "a & b");
        assert!(page.contains("&lt;evil&gt;"));
        assert!(page.contains("a &amp; b"));
        assert!(!page.contains("<evil>"));
    }
}
