use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, HtmlButtonElement, HtmlDivElement, WebSocket};

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");

    setup_websocket(&document)?;

    Ok(())
}

fn setup_websocket(document: &Document) -> Result<(), JsValue> {
    // Create WebSocket connection
    let ws = WebSocket::new("ws://127.0.0.1:8080")?;
    let ws_clone = ws.clone();

    // Set up the UI
    let body = document.body().expect("document should have a body");

    // Create container for messages
    let messages_div = document
        .create_element("div")?
        .dyn_into::<HtmlDivElement>()?;
    messages_div.set_id("messages");
    body.append_child(&messages_div)?;

    // Create button
    let button = document
        .create_element("button")?
        .dyn_into::<HtmlButtonElement>()?;
    button.set_text_content(Some("Press Me"));
    button.set_id("button");
    body.append_child(&button)?;

    // Set up WebSocket message handler
    // Clone document to use inside the closure
    let document_clone = document.clone();
    let messages_div_clone = messages_div.clone();

    let onmessage_callback = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
            if let Ok(p) = document_clone.create_element("p") {
                p.set_text_content(Some(&format!("Received: {}", String::from(txt))));
                if let Err(e) = messages_div_clone.append_child(&p) {
                    web_sys::console::error_1(&format!("Error appending message: {:?}", e).into());
                }
            }
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);

    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    onmessage_callback.forget();

    // Set up button click handler
    let click_callback = Closure::wrap(Box::new(move |_event: web_sys::MouseEvent| {
        let message = format!("Button pressed at: {}", js_sys::Date::now());
        // Send the message but don't update UI - wait for server to send it back
        match ws_clone.send_with_str(&message) {
            Ok(_) => console_log!("Message sent to server: {}", message),
            Err(err) => console_log!("Error sending message: {:?}", err),
        }
        // No UI update here - we'll wait for the message to come back from the server
    }) as Box<dyn FnMut(web_sys::MouseEvent)>);

    button.add_event_listener_with_callback("click", click_callback.as_ref().unchecked_ref())?;
    click_callback.forget();

    // Add logging for connection
    let onopen_callback = Closure::wrap(Box::new(move |_| {
        console_log!("WebSocket connection established");
    }) as Box<dyn FnMut(JsValue)>);

    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    Ok(())
}

#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => {
        web_sys::console::log_1(&format!($($t)*).into());
    }
}

