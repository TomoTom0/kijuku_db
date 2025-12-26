#!/bin/bash
# ローカル環境およびリモート環境へのRustバイナリデプロイスクリプト

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUST_SDK_DIR="$PROJECT_ROOT/rust-sdk"
BINARY_NAME="kijuku-cli"
LOCAL_BIN_DIR="$HOME/.local/bin"

# .envファイルの読み込み
ENV_FILE="$PROJECT_ROOT/.env"
if [ -f "$ENV_FILE" ]; then
  set -a
  source "$ENV_FILE"
  set +a
fi

echo "Rustバイナリをビルド中..."
cd "$RUST_SDK_DIR"
cargo build --release

echo "ローカルにデプロイ中..."
mkdir -p "$LOCAL_BIN_DIR"
cp "$RUST_SDK_DIR/target/release/$BINARY_NAME" "$LOCAL_BIN_DIR/"
chmod +x "$LOCAL_BIN_DIR/$BINARY_NAME"

echo "ローカルデプロイ完了: $LOCAL_BIN_DIR/$BINARY_NAME"
ls -lh "$LOCAL_BIN_DIR/$BINARY_NAME"

# リモートデプロイ
if [ -n "$REMOTE_SSH_HOST" ]; then
  echo ""
  echo "リモートデプロイを開始します..."
  echo "リモートホスト: $REMOTE_SSH_HOST"

  # リモートのバイナリパスを決定
  if [ -n "$REMOTE_BINARY_PATH" ]; then
    REMOTE_BIN_PATH="$REMOTE_BINARY_PATH"
  elif [ -n "$REMOTE_WORK_DIR" ]; then
    REMOTE_BIN_PATH="${REMOTE_WORK_DIR}/bin/${BINARY_NAME}"
  else
    REMOTE_BIN_PATH="~/.local/bin/${BINARY_NAME}"
  fi

  echo "リモートパス: $REMOTE_BIN_PATH"

  # リモート側のディレクトリを作成
  REMOTE_BIN_DIR=$(dirname "$REMOTE_BIN_PATH")
  echo "リモート側のディレクトリを作成中: $REMOTE_BIN_DIR"
  ssh "$REMOTE_SSH_HOST" "mkdir -p $REMOTE_BIN_DIR"

  # バイナリを転送
  echo "バイナリを転送中..."
  scp "$RUST_SDK_DIR/target/release/$BINARY_NAME" "$REMOTE_SSH_HOST:$REMOTE_BIN_PATH"

  # 実行権限を付与
  echo "実行権限を付与中..."
  ssh "$REMOTE_SSH_HOST" "chmod +x $REMOTE_BIN_PATH"

  echo "リモートデプロイ完了: $REMOTE_SSH_HOST:$REMOTE_BIN_PATH"
  ssh "$REMOTE_SSH_HOST" "ls -lh $REMOTE_BIN_PATH"
else
  echo ""
  echo "リモート設定が見つかりません。ローカルデプロイのみ実行しました。"
  echo "リモートデプロイを有効にするには、.envファイルにREMOTE_SSH_HOSTを設定してください。"
fi
