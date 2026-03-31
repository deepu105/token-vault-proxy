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
    pub async fn wait(self) -> Result<CallbackResult> {
        let result = self
            .rx
            .await
            .context("Callback channel closed without receiving a result")?;

        self.handle.abort();
        Ok(result)
    }
}

/// Generate a simple HTML page for the callback response shown in the browser.
pub fn html_page(title: &str, message: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html><head><title>{title}</title></head>
<body style="font-family: sans-serif; text-align: center; margin-top: 50px;">
  <h1>{title}</h1>
  <p>{message}</p>
</body></html>"#
    )
}
