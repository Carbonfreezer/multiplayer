#!/bin/bash

OUT=deploy

echo "Creating output directory..."
mkdir -p "$OUT"

echo "Building WASM..."
cargo build -p tic-tac-toe --target wasm32-unknown-unknown --release

echo "Building Server..."
CARGO_PROFILE_RELEASE_OPT_LEVEL=3 cargo build -p relay-server --release

echo "Copying client files..."
cp target/wasm32-unknown-unknown/release/tic-tac-toe.wasm "$OUT/"
cp backbone-lib/web/*.js "$OUT/"
cp games/tic-tac-toe/web/*.* "$OUT/"

echo "Copying server files..."
cp target/release/relay-server "$OUT/"
cp relay-server/GameConfig.json "$OUT/"

echo "Create documentation..."
cargo doc --no-deps -p backbone-lib -p protocol -p tic-tac-toe -p relay-server --open

echo "Done!"
