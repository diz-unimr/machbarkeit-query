use crate::client::TargetClient;
use crate::model::FeasibilityRequest;
use crate::model::QueryState::Completed;
use anyhow::anyhow;
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use http::StatusCode;
use log::{debug, error, info, trace};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio_tungstenite::tungstenite::handshake::server::Request;
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

pub async fn connect(ws_request: Request, client: TargetClient) -> anyhow::Result<()> {
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
    client: TargetClient,
) {
    let client = Arc::new(client);
    receiver
        .for_each_concurrent(42, |m| async {
            match m {
                Ok(Message::Text(msg)) => {
                    trace!("Message received: {msg}");

                    if let Err(e) = handle_request(client.clone(), &sender, msg).await {
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
    client: Arc<TargetClient>,
    sender: &Sender<FeasibilityRequest>,
    msg: Utf8Bytes,
) -> anyhow::Result<()> {
    // parse request
    let request = serde_json::from_str::<FeasibilityRequest>(&msg);
    match request {
        Ok(r) => {
            // execute request
            match client.execute(r.clone()).await {
                Ok(result) => {
                    // send back to websocket
                    info!(
                        "[Returning] success result({}): {}",
                        result.id,
                        result.result_body.as_deref().unwrap_or("-")
                    );
                    if let Err(e) = sender.send(result.clone()) {
                        Err(anyhow!("Failed to send message: {}", e))?;
                    }
                }
                Err(e) => {
                    let r = r.result(
                        Completed,
                        StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        format!("Failed to execute request: {e}"),
                    );
                    info!("[Returning] error result({}): {e}", r.id);

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
    use crate::client::cql::CqlClient;
    use crate::client::flare::FlareClient;
    use crate::client::TargetClient;
    use crate::config::{Feasibility, FhirServer};
    use crate::model::FeasibilityRequest;
    use crate::model::QueryState::Pending;
    use crate::websocket::connect;
    use chrono::Utc;
    use futures_util::SinkExt;
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;
    use serde_json::Value;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use uuid::Uuid;

    #[tokio::test]
    async fn flare_request_handling_test() {
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

        let client = TargetClient::Flare(
            FlareClient::new(&Feasibility {
                service: "flare".to_string(),
                base_url: format!("{}/query/execute", flare.base_url()),
                auth: None,
            })
            .unwrap(),
        );

        // setup websocket server
        let addr = feed_websocket(FeasibilityRequest {
            id: Uuid::new_v4(),
            status: Pending,
            query: Value::Null,
            date: Utc::now(),
            result_duration: None,
            result_code: None,
            result_body: None,
        })
        .await;
        let url = format!("ws://{addr}");

        // connect websocket
        connect(url.into_client_request().unwrap(), client)
            .await
            .unwrap();

        execute_mock.assert();
    }

    async fn feed_websocket(request: FeasibilityRequest) -> SocketAddr {
        let (tx, rx) = futures_channel::oneshot::channel();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_address = listener.local_addr().unwrap();
        let f = async move {
            tx.send(()).unwrap();
            let (connection, _) = listener.accept().await.expect("No connections to accept");
            let stream = accept_async(connection).await;
            let mut stream = stream.expect("Failed to handshake with connection");

            stream.send(request.try_into().unwrap()).await.unwrap();
        };
        tokio::spawn(f);
        rx.await.expect("Failed to wait for server to be ready");
        listener_address
    }

    #[tokio::test]
    async fn cql_request_handling_test() {
        // mock translate service
        let translate = MockServer::start();
        // mock execute request
        let translate_mock = translate.mock(|when, then| {
            when.method(POST)
                .header("content-type", "application/sq+json")
                .path("/translate");
            then.status(200).header("content-type", "text/plain").body(
                r#"
                    library Retrieve version '1.0.0'
                    using FHIR version '4.0.0'
                    include FHIRHelpers version '4.0.0'

                    context Patient

                    define Criterion:
                      Patient.gender = 'female'

                    define InInitialPopulation:
                      Criterion
                "#,
            );
        });

        // feasibility request
        let request = FeasibilityRequest {
            id: Uuid::new_v4(),
            status: Pending,
            query: Value::Null,
            date: Utc::now(),
            result_duration: None,
            result_code: None,
            result_body: None,
        };

        // mock fhir server
        let fhir_server = MockServer::start();
        let post_resources = fhir_server.mock(|when, then| {
            when.method(POST)
                .header("content-type", "application/fhir+json")
                .path("/fhir");
            then.status(200);
        });
        let evaluate = fhir_server.mock(|when, then| {
            when.method(GET)
                .path("/fhir/Measure/$evaluate-measure")
                .query_param_prefix("measure", "urn:uuid:")
                .query_param("periodStart", "2000")
                .query_param("periodEnd", "2030");
            then.status(200);
        });

        let client = TargetClient::Cql(
            CqlClient::new(
                &Feasibility {
                    service: "cql".to_string(),
                    base_url: translate.base_url(),
                    auth: None,
                },
                &FhirServer {
                    base_url: format!("{}/fhir", fhir_server.base_url()),
                    auth: None,
                },
            )
            .unwrap(),
        );

        // setup websocket server
        let addr = feed_websocket(request).await;
        let url = format!("ws://{addr}");

        // connect websocket
        connect(url.into_client_request().unwrap(), client)
            .await
            .unwrap();

        translate_mock.assert();
        post_resources.assert();
        evaluate.assert();
    }
}
