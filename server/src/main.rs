use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message};

// Shared state between all connections
type Clients = Arc<Mutex<HashMap<SocketAddr, tokio::sync::mpsc::UnboundedSender<Message>>>>;

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("Failed to bind");
    println!("WebSocket server started on 127.0.0.1:8080");

    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));

    while let Ok((stream, addr)) = listener.accept().await {
        let clients_clone = clients.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, addr, clients_clone).await {
                println!("Error in connection: {}", e);
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    clients: Clients,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("New client connected: {}", addr);

    let ws_stream = accept_async(stream).await?;
    let (mut tx, mut rx) = ws_stream.split();

    let (client_sender, mut client_receiver) = tokio::sync::mpsc::unbounded_channel();

    clients.lock().unwrap().insert(addr, client_sender);

    let forward_task = tokio::spawn(async move {
        while let Some(msg) = client_receiver.recv().await {
            if let Err(e) = tx.send(msg).await {
                println!("Error sending to {}: {}", addr, e);
                break;
            }
        }
    });

    while let Some(result) = rx.next().await {
        match result {
            Ok(msg) => {
                if msg.is_text() || msg.is_binary() {
                    println!("Received message from {}: {:?}", addr, msg);

                    let clients_map = clients.lock().unwrap();

                    // Broadcast to ALL clients including the sender
                    for (_, client) in clients_map.iter() {
                        if let Err(e) = client.send(msg.clone()) {
                            println!("Error broadcasting: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                println!("Error receiving from {}: {}", addr, e);
                break;
            }
        }
    }

    println!("Client disconnected: {}", addr);
    clients.lock().unwrap().remove(&addr);

    forward_task.abort();

    Ok(())
}

