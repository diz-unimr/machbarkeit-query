mod auth;
mod client;
mod config;
mod model;
mod websocket;

use crate::auth::{TokenService, TokenServiceConfig};
use crate::client::RestClient;
use crate::config::AppConfig;
use anyhow::anyhow;
pub use futures_util::StreamExt;
use reqwest::header::{HeaderValue, AUTHORIZATION};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // config
    let config = AppConfig::new().expect("Failed to load config");
    let filter = format!(
        "{}={level},tower_http={level}",
        env!("CARGO_CRATE_NAME"),
        level = config.app.log_level
    );
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .init();

    // http client
    let client = RestClient::new(&config.feasibility)?;

    // auth token service
    let token_service = match &config.broker.auth.and_then(|a| a.client_credentials) {
        None => None,
        Some(auth) => {
            // token service
            let token_config = TokenServiceConfig {
                token_url: auth.clone().token_url,
                client_id: auth.client_id.clone(),
                client_secret: auth.client_secret.clone(),
            };

            // Create a new token service instance
            Some(TokenService::new(token_config))
        }
    };
    let mut request = config.broker.url.as_str().into_client_request()?;
    if let Some(service) = token_service {
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(
                format!(
                    "Bearer {}",
                    service
                        .get_token()
                        .await
                        .map(|t| t.secret().to_string())
                        .map_err(|e| anyhow!("Failed to open file: {}", e))?
                )
                .as_str(),
            )?,
        );
    };

    // connect to websocket
    websocket::connect(request, client).await
}
