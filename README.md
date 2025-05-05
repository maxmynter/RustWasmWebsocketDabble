## Building and Running
1. Build the WASM client
```bash
cd client
wasm-pack build --target web
```

2. Run the server
```bash
cd server
cargo run
```

3. Serve the client files
```bash
cd client
python3 -m http.server 8000 # Or use any other http server
```
