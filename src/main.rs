use futures::{FutureExt, StreamExt};
use itertools::Itertools;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::Filter;
use warp::ws::{Message, WebSocket};

// Client connection state
type ConnectionWrapper = Arc<Mutex<Vec<Connection>>>;

#[derive(Serialize, Deserialize, Debug)]
struct ClientMessage {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    data: Option<String>,
}

#[derive(PartialEq)]
enum ConnectionKind {
    Server,
    Client,
    Unknown,
}

struct Connection {
    id: Uuid,
    tx: mpsc::UnboundedSender<Result<Message, warp::Error>>,
    kind: ConnectionKind,
}

#[tokio::main]
async fn main() {
    // Initialize logger
    env_logger::init();

    info!("Starting WebSocket server");

    // Shared state for client connections
    let connections: ConnectionWrapper = Arc::new(Mutex::new(Vec::default()));

    // WebSocket route
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_clients(connections.clone()))
        .map(|ws: warp::ws::Ws, connections| {
            ws.on_upgrade(move |socket| handle_connection(socket, connections))
        });

    // Health check route
    let health_route = warp::path("health").map(|| "OK");

    // Combine routes
    let routes = ws_route.or(health_route);

    // Start the server
    let port = 8008;
    info!("Server listening on port {}", port);
    warp::serve(routes).run(([0, 0, 0, 0], port)).await;
}

fn with_clients(
    connections: ConnectionWrapper,
) -> impl Filter<Extract = (ConnectionWrapper,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || connections.clone())
}
//
async fn handle_connection(ws: WebSocket, connections: ConnectionWrapper) {
    let client_id: Uuid = Uuid::new_v4();
    info!("New connection: {}", client_id);

    let (ws_tx, mut ws_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel();
    let rx = UnboundedReceiverStream::new(rx);

    tokio::task::spawn(rx.forward(ws_tx).map(|result| {
        if let Err(e) = result {
            error!("Websocket send error: {}", e);
        }
    }));

    let connection = Connection {
        id: client_id,
        tx: tx.clone(),
        kind: ConnectionKind::Unknown,
    };

    connections.lock().unwrap().push(connection);

    // Process incoming messages
    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(msg) => {
                if msg.is_text() {
                    info!("message recieved: {}", msg.to_str().unwrap());
                    process_message(msg.to_str().unwrap_or_default(), client_id, &connections)
                        .await;
                }
            }
            Err(e) => {
                error!("WebSocket error from {}: {}", client_id, e);
                break;
            }
        }
    }

    // Client disconnected
    client_disconnected(client_id, connections).await;
}

async fn process_message(message: &str, connection_id: Uuid, connections: &ConnectionWrapper) {
    let parsed: Result<ClientMessage, _> = serde_json::from_str(message);
    let mut lock = connections.lock().unwrap();

    match parsed {
        Ok(msg) => {
            if let Some(kind) = msg.kind {
                match kind.as_str() {
                    "connect" => {
                        let connection = lock
                            .iter_mut()
                            .find(|conn| conn.id == connection_id)
                            .unwrap();
                        if let Some(who) = msg.data {
                            match who.to_lowercase().as_str() {
                                "server" => connection.kind = ConnectionKind::Server,
                                "client" => connection.kind = ConnectionKind::Client,
                                _ => {}
                            }
                        }
                    }
                    "message" => {
                        if let Some(message) = msg.data {
                            lock.iter()
                                .filter(|conn| conn.kind == ConnectionKind::Server)
                                .map(|conn| &conn.tx)
                                .for_each(|tx| {
                                    let mut message = message.clone();

                                    if let Some((name, body)) = message.split(" > ").next_tuple() {
                                        if name == "Suzzzi" && rand::random_bool(1.0 / 25.0) {
                                            message = format!("Sushi > {body}")
                                        }
                                    }

                                    tx.send(Ok(Message::text(message))).unwrap();
                                });
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            warn!("Failed to parse message from {}: {}", connection_id, e);
        }
    }
}

async fn client_disconnected(client_id: Uuid, connections: ConnectionWrapper) {
    info!("Client disconnected: {}", client_id);
    let mut lock = connections.lock().unwrap();

    let index = lock.iter().position(|conn| conn.id == client_id).unwrap();
    lock.remove(index);
}
