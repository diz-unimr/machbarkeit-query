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
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, trace};
use reqwest::header::{HeaderValue, AUTHORIZATION};
use reqwest::StatusCode;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Utf8Bytes;
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
    info!("WebSocket client connected to {}", config.broker.url);

    // split stream and build channel to send messages to further downstream
    let (mut sink, stream) = ws_stream.split();
    let (sender, _) = broadcast::channel(10);
    let mut receiver: Receiver<FeasibilityRequest> = sender.subscribe();

    // forward messages from the channel to the sink
    tokio::spawn(async move {
        while let Ok(msg) = receiver.recv().await {
            match msg.try_into() {
                Ok(message) => {
                    if sink.send(message).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to parse FeasibilityRequest: {}", e);
                    break;
                }
            }
        }
    });

    // read incoming messages
    info!("Reading messages from {}", request.uri());
    tokio::spawn(ws_read(stream, sender, client)).await?;

    Ok(())
}

async fn ws_read(
    receiver: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    sender: Sender<FeasibilityRequest>,
    client: RestClient,
) {
    loop {
        receiver
            .for_each_concurrent(42, |m| async {
                match m {
                    Ok(Message::Text(msg)) => {
                        trace!("Message received: {}", msg);

                        if let Err(e) = handle_request(&client, &sender, msg).await {
                            error!("Error handling request: {}", e);
                        }
                    }
                    Ok(Message::Close(_)) => {
                        debug!("Closing WebSocket connection");
                    }
                    Ok(_) => error!("Unexpected message type"),
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        return;
                    }
                }
            })
            .await;

        info!("Websocket closed");
        break;
    }
}

async fn handle_request(
    client: &RestClient,
    sender: &Sender<FeasibilityRequest>,
    msg: Utf8Bytes,
) -> Result<(), anyhow::Error> {
    // parse request
    let request = serde_json::from_str::<FeasibilityRequest>(&msg);
    match request {
        Ok(mut r) => {
            // execute request
            match client.clone().execute(&mut r).await {
                Ok(result) => {
                    // send back to websocket
                    info!("Sending back feasibility result id={}", result.id);
                    if let Err(e) = sender.send(result.clone()) {
                        Err(anyhow!("Failed to send message: {}", e))?;
                    }
                }
                Err(e) => {
                    r.status = Completed;
                    r.result_body = Some(format!("Failed to execute request: {}", e));
                    r.result_code = Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16());
                    info!("Sending back feasibility result id={}: {}", r.id, e);

                    sender.send(r)?;
                }
            }
        }
        Err(e) => {
            error!("Failed to parse feasibility request: {}", e);
        }
    }

    Ok(())
}
