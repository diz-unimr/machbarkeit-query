mod auth;
mod client;
mod config;
mod model;

use crate::auth::{TokenService, TokenServiceConfig};
use crate::client::RestClient;
use crate::config::AppConfig;
use crate::model::FeasibilityRequest;
use crate::model::QueryState::Completed;
use anyhow::anyhow;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info};
use reqwest::header::{HeaderValue, AUTHORIZATION};
use reqwest::StatusCode;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};
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
    let token_service = match &config.broker.auth.map(|a| a.client_credentials).flatten() {
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

    let (ws_stream, _) = connect_async(request.clone())
        .await
        .expect("Failed to connect");
    info!("WebSocket client connected");

    // read incoming messages
    info!("Reading messages from {}", request.uri());
    tokio::spawn(ws_read(ws_stream, client)).await?;

    Ok(())
}

async fn ws_read(ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>, client: RestClient) {
    let (mut sink, mut stream) = ws_stream.split();

    loop {
        while let Some(Ok(msg)) = stream.next().await {
            match msg {
                Message::Text(msg) => {
                    debug!("Message received: {}", msg);

                    // parse request
                    let mut request = serde_json::from_str::<FeasibilityRequest>(&msg);
                    match &mut request {
                        Ok(r) => {
                            // execute request
                            match client.clone().execute(r).await {
                                Ok(result) => {
                                    // send back to websocket
                                    if let Ok(result) = serde_json::to_string(result) {
                                        debug!("Feasibility result: {}", result);
                                        let msg = Message::text(result.to_string());
                                        if let Err(e) = sink.send(msg).await {
                                            error!("Failed to send message: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    r.status = Completed;
                                    r.result_body =
                                        Some(format!("Failed to execute request: {}", e));
                                    r.result_code =
                                        Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16());
                                    debug!("Sending back error result: {}", e);

                                    if let Err(e) = sink
                                        .send(Message::text(
                                            serde_json::to_string(r)
                                                .expect("Failed to serialize FeasibilityRequest"),
                                        ))
                                        .await
                                    {
                                        error!("Failed to send message: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse feasibility request: {}", e);
                        }
                    }
                }
                Message::Close(_) => {
                    debug!("Closing WebSocket connection");
                    break;
                }
                _ => error!("Unexpected message type"),
            }
        }
    }
}
