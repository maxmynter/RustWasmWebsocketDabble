use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, Document, HtmlCanvasElement, KeyboardEvent, WebSocket};

// Game state types - must match server definitions
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

// When the wasm module is instantiated
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    // Get window and document
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    // Set up game canvas
    setup_game(&document)?;

    Ok(())
}

fn setup_game(document: &Document) -> Result<(), JsValue> {
    // Set up the UI
    let body = document.body().expect("document should have a body");

    // Create canvas
    let canvas = document
        .create_element("canvas")?
        .dyn_into::<HtmlCanvasElement>()?;
    canvas.set_width(800);
    canvas.set_height(600);
    canvas.set_id("game-canvas");

    // Set border using attribute
    canvas.set_attribute("style", "border: 1px solid black")?;

    body.append_child(&canvas)?;

    // Get canvas context for drawing
    let context = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    // Add instructions
    let instructions = document.create_element("p")?;
    instructions.set_text_content(Some("Use WASD keys to move your square"));
    body.append_child(&instructions)?;

    // Create WebSocket connection
    let ws = WebSocket::new("ws://127.0.0.1:8080")?;
    let ws_clone = ws.clone();

    // Create a shared reference to the game state
    let game_state = std::rc::Rc::new(std::cell::RefCell::new(GameState {
        players: HashMap::new(),
    }));

    // Clone for the render loop
    let game_state_clone = game_state.clone();
    let context_clone = context.clone();

    // Set up WebSocket message handler
    let onmessage_callback = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
            let text = String::from(txt);
            match serde_json::from_str::<ServerMessage>(&text) {
                Ok(msg) => {
                    // Update game state
                    *game_state.borrow_mut() = msg.game_state;

                    // Render the updated game state
                    render_game(&context, &game_state.borrow());
                }
                Err(e) => {
                    console_log!("Error parsing server message: {:?}", e);
                }
            }
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    onmessage_callback.forget();

    // Set up keyboard event handler
    let keydown_callback = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        let key = e.key();
        match key.as_str() {
            "w" | "a" | "s" | "d" => {
                // Send movement command to server
                let msg = ClientMessage::Move { direction: key };

                if let Ok(json) = serde_json::to_string(&msg) {
                    if let Err(err) = ws_clone.send_with_str(&json) {
                        console_log!("Error sending move command: {:?}", err);
                    }
                }
            }
            _ => {}
        }
    }) as Box<dyn FnMut(KeyboardEvent)>);

    document
        .add_event_listener_with_callback("keydown", keydown_callback.as_ref().unchecked_ref())?;
    keydown_callback.forget();

    // Set up onopen handler to send Join message
    let ws_join = ws.clone();
    let onopen_callback = Closure::wrap(Box::new(move |_| {
        console_log!("WebSocket connection established");

        // Send join message
        let msg = ClientMessage::Join;
        if let Ok(json) = serde_json::to_string(&msg) {
            if let Err(err) = ws_join.send_with_str(&json) {
                console_log!("Error sending join command: {:?}", err);
            }
        }
    }) as Box<dyn FnMut(JsValue)>);

    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    // Set up animation frame loop for smooth rendering
    setup_render_loop(game_state_clone, context_clone)?;

    Ok(())
}

fn render_game(context: &CanvasRenderingContext2d, game_state: &GameState) {
    // Clear the canvas
    context.clear_rect(0.0, 0.0, 800.0, 600.0);

    // Draw each player
    for (_, player) in &game_state.players {
        // Use fill_style property instead of set_fill_style method
        context.set_fill_style(&JsValue::from_str(&player.color));
        context.fill_rect(player.x as f64, player.y as f64, 50.0, 50.0);

        // Draw player ID
        context.set_fill_style(&JsValue::from_str("white"));
        context.set_font("14px Arial");
        context
            .fill_text(&player.id, player.x as f64 + 5.0, player.y as f64 + 25.0)
            .unwrap();
    }
}

fn setup_render_loop(
    game_state: std::rc::Rc<std::cell::RefCell<GameState>>,
    context: CanvasRenderingContext2d,
) -> Result<(), JsValue> {
    let f = std::rc::Rc::new(std::cell::RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        // Render the current game state
        render_game(&context, &game_state.borrow());

        // Schedule the next frame
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());

    Ok(())
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .unwrap();
}

// Helper macro for logging to console
#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => {
        web_sys::console::log_1(&format!($($t)*).into());
    }
}

