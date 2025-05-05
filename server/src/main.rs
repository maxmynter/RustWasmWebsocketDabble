use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use serde::{Serialize, Deserialize};

// Game constants
const CANVAS_WIDTH: u32 = 800;
const CANVAS_HEIGHT: u32 = 600;
const PLAYER_SIZE: u32 = 50;
const PLAYER_SPEED: u32 = 5;

// Game state types
#[derive(Clone, Serialize, Deserialize)]
struct Player {
    id: String,
    x: u32,
    y: u32,
    color: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct GameState {
    players: HashMap<String, Player>,
}

#[derive(Serialize, Deserialize)]
enum ClientMessage {
    Move { direction: String },
    Join,
}

#[derive(Serialize, Deserialize)]
struct ServerMessage {
    game_state: GameState,
}

// Shared state between all connections
type Clients = Arc<Mutex<HashMap<SocketAddr, tokio::sync::mpsc::UnboundedSender<Message>>>>;
type GameStateSync = Arc<Mutex<GameState>>;

#[tokio::main]
async fn main() {
    // Create a simple TCP listener on localhost:8080
    let listener = TcpListener::bind("127.0.0.1:8080").await.expect("Failed to bind");
    println!("Game server started on 127.0.0.1:8080");

    // Create shared state
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
    let game_state = Arc::new(Mutex::new(GameState {
        players: HashMap::new(),
    }));

    // Accept connections in a loop
    while let Ok((stream, addr)) = listener.accept().await {
        // Clone the clients for this connection
        let clients_clone = clients.clone();
        let game_state_clone = game_state.clone();
        
        // Spawn a task for each inbound connection
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, addr, clients_clone, game_state_clone).await {
                println!("Error in connection: {}", e);
            }
        });
    }
}

async fn handle_connection(
    stream: TcpStream, 
    addr: SocketAddr, 
    clients: Clients,
    game_state: GameStateSync
) -> Result<(), Box<dyn std::error::Error>> {
    println!("New player connected: {}", addr);
    
    // Generate a unique player ID and random color
    let player_id = format!("player_{}", addr.port());
    let colors = ["#FF0000", "#00FF00", "#0000FF", "#FFFF00", "#FF00FF", "#00FFFF"];
    let color = colors[addr.port() as usize % colors.len()];
    
    // Create a new player at a random position
    let player = Player {
        id: player_id.clone(),
        x: 100 + (addr.port() as u32 % 400),
        y: 100 + (addr.port() as u32 % 300),
        color: color.to_string(),
    };
    
    // Add player to game state - scope the lock
    {
        let mut state = game_state.lock().unwrap();
        state.players.insert(player_id.clone(), player);
    } // Lock is released here
    
    // Accept WebSocket connection
    let ws_stream = accept_async(stream).await?;
    let (mut tx, mut rx) = ws_stream.split();
    
    // Create channel for this client
    let (client_sender, mut client_receiver) = tokio::sync::mpsc::unbounded_channel();
    
    // Store the sender in shared state
    {
        let mut clients_map = clients.lock().unwrap();
        clients_map.insert(addr, client_sender);
    } // Lock is released here
    
    // Send initial game state to the new player
    let initial_state = {
        let state = game_state.lock().unwrap();
        serde_json::to_string(&ServerMessage {
            game_state: state.clone(),
        })?
    }; // Lock is released here
    
    tx.send(Message::Text(initial_state)).await?;
    
    // Broadcast updated game state to all players
    broadcast_game_state(&clients, &game_state).await?;
    
    // Task to forward messages from other clients to this client
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = client_receiver.recv().await {
            if let Err(e) = tx.send(msg).await {
                println!("Error sending to {}: {}", addr, e);
                break;
            }
        }
    });
    
    // Listen for messages from this client
    while let Some(result) = rx.next().await {
        match result {
            Ok(msg) => {
                if let Message::Text(text) = msg {
                    match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(ClientMessage::Move { direction }) => {
                            // Update player position based on direction
                            {
                                let mut state = game_state.lock().unwrap();
                                if let Some(player) = state.players.get_mut(&player_id) {
                                    match direction.as_str() {
                                        "w" => {
                                            if player.y > PLAYER_SPEED {
                                                player.y -= PLAYER_SPEED;
                                            }
                                        },
                                        "a" => {
                                            if player.x > PLAYER_SPEED {
                                                player.x -= PLAYER_SPEED;
                                            }
                                        },
                                        "s" => {
                                            if player.y < CANVAS_HEIGHT - PLAYER_SIZE - PLAYER_SPEED {
                                                player.y += PLAYER_SPEED;
                                            }
                                        },
                                        "d" => {
                                            if player.x < CANVAS_WIDTH - PLAYER_SIZE - PLAYER_SPEED {
                                                player.x += PLAYER_SPEED;
                                            }
                                        },
                                        _ => {}
                                    }
                                }
                            } // Lock is released here before await
                            
                            // Broadcast updated game state
                            broadcast_game_state(&clients, &game_state).await?;
                        },
                        Ok(ClientMessage::Join) => {
                            // Player has joined, state already updated
                            println!("Player {} joined the game", player_id);
                        },
                        Err(e) => {
                            println!("Error parsing message from {}: {}", addr, e);
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
    
    // Client disconnected or error occurred
    println!("Player disconnected: {}", addr);
    
    // Remove player from game state
    {
        let mut state = game_state.lock().unwrap();
        state.players.remove(&player_id);
    } // Lock is released here
    
    // Remove client from clients list
    {
        let mut clients_map = clients.lock().unwrap();
        clients_map.remove(&addr);
    } // Lock is released here
    
    // Broadcast updated game state
    broadcast_game_state(&clients, &game_state).await?;
    
    // Cancel the forward task
    forward_task.abort();
    
    Ok(())
}

async fn broadcast_game_state(clients: &Clients, game_state: &GameStateSync) -> Result<(), Box<dyn std::error::Error>> {
    // Get the game state as JSON - scope the lock
    let state_json = {
        let state = game_state.lock().unwrap();
        serde_json::to_string(&ServerMessage {
            game_state: state.clone(),
        })?
    }; // Lock is released here
    
    // Broadcast to all clients - scope the lock
    {
        let clients_map = clients.lock().unwrap();
        for (_, client) in clients_map.iter() {
            if let Err(e) = client.send(Message::Text(state_json.clone())) {
                println!("Error broadcasting game state: {}", e);
            }
        }
    } // Lock is released here
    
    Ok(())
}
