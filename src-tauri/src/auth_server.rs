use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;
use warp::Filter;

#[derive(Debug, Clone)]
pub struct AuthCallbackData {
    pub api_key: String,
    pub tenant_id: String,
    pub tenant_name: String,
}

#[derive(Debug, Clone)]
pub enum AuthError {
    ServerStartFailed(String),
    TimeoutError,
    CallbackError(String),
    PortUnavailable,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::ServerStartFailed(msg) => write!(f, "Failed to start auth server: {}", msg),
            AuthError::TimeoutError => write!(f, "Authentication timed out"),
            AuthError::CallbackError(msg) => write!(f, "Authentication failed: {}", msg),
            AuthError::PortUnavailable => write!(f, "Unable to find available port"),
        }
    }
}

impl std::error::Error for AuthError {}

pub struct AuthServer {
    server_handle: Option<tokio::task::JoinHandle<()>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    pub port: u16,
    pub callback_url: String,
}

impl AuthServer {
    pub async fn start(
    ) -> Result<(Self, oneshot::Receiver<Result<AuthCallbackData, AuthError>>), AuthError> {
        let port = Self::find_available_port().await?;
        let callback_url = format!("http://localhost:{}/callback", port);

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (result_tx, result_rx) = oneshot::channel();

        let server_handle = Self::start_server(port, shutdown_rx, result_tx).await?;

        let auth_server = AuthServer {
            server_handle: Some(server_handle),
            shutdown_tx: Some(shutdown_tx),
            port,
            callback_url,
        };

        Ok((auth_server, result_rx))
    }

    async fn find_available_port() -> Result<u16, AuthError> {
        // Start with the default CLI port
        let preferred_ports = [8765, 8766, 8767, 8768, 8769, 8770];

        for &port in &preferred_ports {
            if Self::is_port_available(port).await {
                return Ok(port);
            }
        }

        Err(AuthError::PortUnavailable)
    }

    async fn is_port_available(port: u16) -> bool {
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
        tokio::net::TcpListener::bind(addr).await.is_ok()
    }

    async fn start_server(
        port: u16,
        shutdown_rx: oneshot::Receiver<()>,
        result_tx: oneshot::Sender<Result<AuthCallbackData, AuthError>>,
    ) -> Result<tokio::task::JoinHandle<()>, AuthError> {
        let result_tx = Arc::new(Mutex::new(Some(result_tx)));
        let result_tx_filter = warp::any().map(move || result_tx.clone());

        let callback_route = warp::path("callback")
            .and(warp::query::<HashMap<String, String>>())
            .and(result_tx_filter)
            .and_then(Self::handle_callback);

        let routes = callback_route.recover(Self::handle_rejection);

        let addr: SocketAddr = format!("127.0.0.1:{}", port)
            .parse()
            .map_err(|e: std::net::AddrParseError| AuthError::ServerStartFailed(e.to_string()))?;

        use tracing::info;
        info!(address = %addr, "Starting auth server");

        let (_, server) = warp::serve(routes).bind_with_graceful_shutdown(addr, async {
            shutdown_rx.await.ok();
        });

        let handle = tokio::spawn(server);
        info!(address = %addr, "Auth server started successfully");

        Ok(handle)
    }

    async fn handle_callback(
        params: HashMap<String, String>,
        result_tx: Arc<Mutex<Option<oneshot::Sender<Result<AuthCallbackData, AuthError>>>>>,
    ) -> Result<impl warp::Reply, Infallible> {
        use tracing::info;
        info!(params_count = params.len(), "Received auth callback");
        let result = if let Some(error) = params.get("error") {
            Err(AuthError::CallbackError(error.clone()))
        } else if let (Some(api_key), Some(tenant_id), Some(tenant_name)) = (
            params.get("key"),
            params.get("tenant_id"),
            params.get("tenant_name"),
        ) {
            Ok(AuthCallbackData {
                api_key: api_key.clone(),
                tenant_id: tenant_id.clone(),
                tenant_name: tenant_name.clone(),
            })
        } else {
            Err(AuthError::CallbackError(
                "Missing required parameters".to_string(),
            ))
        };

        // Return JavaScript to automatically close the window
        let html = match &result {
            Ok(_) => String::from(
                r#"<!DOCTYPE html>
<html>
<head>
    <title>Authentication Successful</title>
    <script>
        // Show brief success message then close window
        document.addEventListener('DOMContentLoaded', function() {
            setTimeout(function() {
                window.close();
            }, 1000);
        });
    </script>
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; text-align: center; padding: 20px; background: #f5f5f5;">
    <div style="background: white; padding: 20px; border-radius: 8px; max-width: 300px; margin: 0 auto; box-shadow: 0 2px 10px rgba(0,0,0,0.1);">
        <h1 style="color: #22c55e; margin: 0;">✓ Success</h1>
        <p style="margin: 10px 0 0 0;">Authentication successful! Closing...</p>
    </div>
</body>
</html>"#,
            ),
            Err(ref e) => {
                format!(
                    r#"<!DOCTYPE html>
<html>
<head>
    <title>Authentication Failed</title>
    <script>
        // Show error message then close window
        document.addEventListener('DOMContentLoaded', function() {{
            setTimeout(function() {{
                window.close();
            }}, 3000);
        }});
    </script>
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; text-align: center; padding: 20px; background: #f5f5f5;">
    <div style="background: white; padding: 20px; border-radius: 8px; max-width: 300px; margin: 0 auto; box-shadow: 0 2px 10px rgba(0,0,0,0.1);">
        <h1 style="color: #ef4444; margin: 0;">✗ Failed</h1>
        <p style="margin: 10px 0 0 0;">{}</p>
        <p style="margin: 10px 0 0 0; font-size: 14px; color: #666;">Closing in 3 seconds...</p>
    </div>
</body>
</html>"#,
                    e
                )
            }
        };

        // Create the HTTP response
        let response = warp::reply::html(html);

        // Schedule the result sending after a small delay to ensure HTTP response is sent
        let result_tx_clone = result_tx.clone();
        let result_clone = result.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if let Some(tx) = result_tx_clone.lock().await.take() {
                let _ = tx.send(result_clone);
            }
        });

        Ok(response)
    }

    async fn handle_rejection(_err: warp::Rejection) -> Result<impl warp::Reply, Infallible> {
        let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>Authentication Error</title>
    <script>
        // Show error message then close window
        document.addEventListener('DOMContentLoaded', function() {
            setTimeout(function() {
                window.close();
            }, 3000);
        });
    </script>
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; text-align: center; padding: 20px; background: #f5f5f5;">
    <div style="background: white; padding: 20px; border-radius: 8px; max-width: 300px; margin: 0 auto; box-shadow: 0 2px 10px rgba(0,0,0,0.1);">
        <h1 style="color: #ef4444; margin: 0;">✗ Request Error</h1>
        <p style="margin: 10px 0 0 0;">Authentication request error</p>
        <p style="margin: 10px 0 0 0; font-size: 14px; color: #666;">Closing in 3 seconds...</p>
    </div>
</body>
</html>"#;

        Ok(warp::reply::with_status(
            warp::reply::html(html),
            warp::http::StatusCode::BAD_REQUEST,
        ))
    }

    pub async fn wait_for_callback_with_timeout(
        result_rx: oneshot::Receiver<Result<AuthCallbackData, AuthError>>,
        timeout_duration: Duration,
    ) -> Result<AuthCallbackData, AuthError> {
        match timeout(timeout_duration, result_rx).await {
            Ok(Ok(Ok(data))) => Ok(data),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err(AuthError::CallbackError(
                "Server closed unexpectedly".to_string(),
            )),
            Err(_) => Err(AuthError::TimeoutError),
        }
    }

    pub async fn shutdown(mut self) {
        // Send shutdown signal
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        // Wait for server to complete gracefully
        if let Some(server_handle) = self.server_handle.take() {
            let _ = server_handle.await;
        }

        // Verify port is released (optional verification)
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

impl Drop for AuthServer {
    fn drop(&mut self) {
        // Emergency cleanup - this should not normally be needed
        // since shutdown() should be called explicitly
        if let Some(handle) = &self.server_handle {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_port_availability() {
        // Test that port checking works
        let available = AuthServer::is_port_available(0).await; // Port 0 should be available
        assert!(available);
    }

    #[tokio::test]
    async fn test_server_lifecycle() {
        let (server, _result_rx) = AuthServer::start().await.expect("Failed to start server");
        let port = server.port;

        // Server should be running
        assert!(!AuthServer::is_port_available(port).await);

        // Shutdown server
        server.shutdown().await;

        // Port should be available again after a brief delay
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(AuthServer::is_port_available(port).await);
    }
}
