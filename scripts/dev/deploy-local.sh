#!/bin/bash
# ローカル環境へのRustバイナリデプロイスクリプト

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUST_SDK_DIR="$PROJECT_ROOT/rust-sdk"
BINARY_NAME="kijuku-cli"
LOCAL_BIN_DIR="$HOME/.local/bin"

echo "Rustバイナリをビルド中..."
cd "$RUST_SDK_DIR"
cargo build --release

echo "ローカルにデプロイ中..."
mkdir -p "$LOCAL_BIN_DIR"
cp "$RUST_SDK_DIR/target/release/$BINARY_NAME" "$LOCAL_BIN_DIR/"
chmod +x "$LOCAL_BIN_DIR/$BINARY_NAME"

echo "デプロイ完了: $LOCAL_BIN_DIR/$BINARY_NAME"
ls -lh "$LOCAL_BIN_DIR/$BINARY_NAME"
