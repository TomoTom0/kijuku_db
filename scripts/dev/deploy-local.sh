#!/bin/bash
# ローカル環境およびリモート環境へのRustバイナリデプロイスクリプト

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUST_SDK_DIR="$PROJECT_ROOT/rust-sdk"
BINARY_NAME="kijuku-cli"

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

# ワークスペースのtargetディレクトリから取得
WORKSPACE_TARGET="$PROJECT_ROOT/target/release"

# ローカルデプロイ
echo "ローカルにデプロイ中..."
LOCAL_BINARY_DIR="$HOME/.local/kijuku-db/bin"
LOCAL_SYMLINK_DIR="$HOME/.local/bin"

mkdir -p "$LOCAL_BINARY_DIR" "$LOCAL_SYMLINK_DIR"
cp "$WORKSPACE_TARGET/$BINARY_NAME" "$LOCAL_BINARY_DIR/"
chmod +x "$LOCAL_BINARY_DIR/$BINARY_NAME"
ln -sf "$LOCAL_BINARY_DIR/$BINARY_NAME" "$LOCAL_SYMLINK_DIR/$BINARY_NAME"

echo "ローカルデプロイ完了:"
ls -lh "$LOCAL_BINARY_DIR/$BINARY_NAME"
ls -lh "$LOCAL_SYMLINK_DIR/$BINARY_NAME"

# リモートデプロイ
if [ -n "$REMOTE_SSH_HOST" ]; then
  echo ""
  echo "リモートデプロイを開始します..."
  echo "リモートホスト: $REMOTE_SSH_HOST"

  REMOTE_BINARY_DIR="~/.local/kijuku-db/bin"
  REMOTE_BINARY_PATH="${REMOTE_BINARY_DIR}/${BINARY_NAME}"
  REMOTE_SYMLINK_PATH="~/.local/bin/${BINARY_NAME}"

  echo "リモートバイナリパス: $REMOTE_BINARY_PATH"
  echo "リモートシンボリックリンク: $REMOTE_SYMLINK_PATH"

  # リモート側のディレクトリを作成
  echo "リモート側のディレクトリを作成中..."
  ssh "$REMOTE_SSH_HOST" "mkdir -p $REMOTE_BINARY_DIR ~/.local/bin"

  # バイナリを転送
  echo "バイナリを転送中..."
  scp "$WORKSPACE_TARGET/$BINARY_NAME" "$REMOTE_SSH_HOST:$REMOTE_BINARY_PATH"

  # 実行権限を付与
  echo "実行権限を付与中..."
  ssh "$REMOTE_SSH_HOST" "chmod +x $REMOTE_BINARY_PATH"

  # シンボリックリンクを作成
  echo "シンボリックリンクを作成中..."
  ssh "$REMOTE_SSH_HOST" "ln -sf $REMOTE_BINARY_PATH $REMOTE_SYMLINK_PATH"

  echo "リモートデプロイ完了:"
  ssh "$REMOTE_SSH_HOST" "ls -lh $REMOTE_BINARY_PATH && ls -lh $REMOTE_SYMLINK_PATH"
else
  echo ""
  echo "リモート設定が見つかりません。ローカルデプロイのみ実行しました。"
  echo "リモートデプロイを有効にするには、.envファイルにREMOTE_SSH_HOSTを設定してください。"
fi
