use crate::client::RestClient;
use crate::model::FeasibilityRequest;
use crate::model::QueryState::Completed;
use anyhow::anyhow;
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use http::StatusCode;
use log::{debug, error, info, trace};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio_tungstenite::tungstenite::handshake::server::Request;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

pub async fn connect(ws_request: Request, client: RestClient) -> anyhow::Result<()> {
    let (ws_stream, _) = connect_async(ws_request.clone()).await?;
    info!("WebSocket client connected to {}", ws_request.uri());

    // split stream (read, write)
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
                    error!("Failed to parse FeasibilityRequest: {e}");
                    break;
                }
            }
        }
    });

    // read incoming messages
    info!("Reading messages from {}", ws_request.uri());
    tokio::spawn(ws_read(stream, sender, client)).await?;

    Ok(())
}

async fn ws_read(
    receiver: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    sender: Sender<FeasibilityRequest>,
    client: RestClient,
) {
    receiver
        .for_each_concurrent(42, |m| async {
            match m {
                Ok(Message::Text(msg)) => {
                    trace!("Message received: {msg}");

                    if let Err(e) = handle_request(&client, &sender, msg).await {
                        error!("Error handling request: {e}");
                    }
                }
                Ok(Message::Close(_)) => {
                    debug!("Closing WebSocket connection");
                }
                Ok(_) => error!("Unexpected message type"),
                Err(e) => {
                    error!("WebSocket error: {e}");
                }
            }
        })
        .await;

    info!("Websocket closed");
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
                    r.result_body = Some(format!("Failed to execute request: {e}"));
                    r.result_code = Some(StatusCode::INTERNAL_SERVER_ERROR.as_u16());
                    info!("Sending back feasibility result id={}: {}", r.id, e);

                    sender.send(r)?;
                }
            }
        }
        Err(e) => {
            error!("Failed to parse feasibility request: {e}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::client::RestClient;
    use crate::config::Server;
    use crate::model::FeasibilityRequest;
    use crate::model::QueryState::Pending;
    use crate::websocket::connect;
    use chrono::Utc;
    use futures_util::SinkExt;
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use serde_json::Value;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use uuid::Uuid;

    #[tokio::test]
    async fn request_handling_test() {
        let _ = env_logger::try_init();

        // mock flare server
        let flare = MockServer::start();
        // mock execute request
        let execute_mock = flare.mock(|when, then| {
            when.method(POST)
                .header("content-type", "application/sq+json")
                .path("/query/execute");
            then.status(200)
                .header("content-type", "text/plain")
                .body("42");
        });

        let client = RestClient::new(&Server {
            base_url: format!("{}/query/execute", flare.base_url()),
            auth: None,
        })
        .unwrap();

        // setup websocket server
        let (tx, rx) = futures_channel::oneshot::channel();
        let f = async move {
            let listener = TcpListener::bind("127.0.0.1:12345").await.unwrap();
            tx.send(()).unwrap();
            let (connection, _) = listener.accept().await.expect("No connections to accept");
            let stream = accept_async(connection).await;
            let mut stream = stream.expect("Failed to handshake with connection");

            stream
                .send(
                    FeasibilityRequest {
                        id: Uuid::new_v4(),
                        status: Pending,
                        query: Value::Null,
                        date: Utc::now(),
                        result_duration: None,
                        result_code: None,
                        result_body: None,
                    }
                    .try_into()
                    .unwrap(),
                )
                .await
                .unwrap();
        };
        tokio::spawn(f);
        rx.await.expect("Failed to wait for server to be ready");

        let url = "ws://localhost:12345/";
        connect(url.into_client_request().unwrap(), client)
            .await
            .unwrap();

        execute_mock.assert();
    }
}
