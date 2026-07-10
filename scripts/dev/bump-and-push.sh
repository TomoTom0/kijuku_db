#!/bin/bash
# push 時に version の patch を +1 して commit & push する
#
# 目的: デプロイされた kijuku-cli バイナリの新旧を --version の数値だけで判別できるようにする。
#   push ごとに rust-sdk/Cargo.toml の version（= --version の元）の patch を加算する。
#
# Usage:
#   mise run push             # patch +1 → commit → push
#   mise run push --dry-run   # 5箇所を更新して commit/push せず終了（git diff で確認）
#
# 注意:
# - dev / main ブランチでは実行不可（origin/dev, origin/main は PR のみ）。
# - CHANGELOG / tm release とは独立。version は push 単位、CHANGELOG は機能リリース単位。
#   リリース時はその時点の version で docs/changelog に記載し tm release する。
# - dry-run でファイルを更新したまま終了する。戻す場合は git restore（または git checkout -- <file>）。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_ROOT"

DRY_RUN=0
if [ "${1:-}" = "--dry-run" ]; then
  DRY_RUN=1
fi

# 保護ブランチ拒否（origin/dev, origin/main は PR のみ）
BRANCH=$(git branch --show-current)
if [ -z "$BRANCH" ]; then
  echo "Error: 現在ブランチが特定できません（デタッチドHEAD状態の可能性があります）。feature ブランチを checkout してください。" >&2
  exit 1
fi
if [ "$BRANCH" = "dev" ] || [ "$BRANCH" = "main" ]; then
  echo "Error: ブランチ '$BRANCH' では mise run push を実行できません（origin/dev, origin/main は PR のみ）。feature ブランチで実行してください。" >&2
  exit 1
fi

CARGO_TOML="$PROJECT_ROOT/rust-sdk/Cargo.toml"
PACKAGE_JSON="$PROJECT_ROOT/ts-sdk/package.json"
README="$PROJECT_ROOT/rust-sdk/README.md"
CLI_RS="$PROJECT_ROOT/rust-sdk/src/bin/cli.rs"
# Cargo.lock はプロジェクトルート（ワークスペースルート）にあるものを cargo check で更新するため、
# ここでは明示的なパスを持たない（rust-sdk/Cargo.lock は stale な未使用ファイルとして削除済み）。

for f in "$CARGO_TOML" "$PACKAGE_JSON" "$README" "$CLI_RS"; do
  if [ ! -f "$f" ]; then
    echo "Error: $f が見つかりません" >&2
    exit 1
  fi
done

# 現在の version 取得（rust-sdk/Cargo.toml の [package] version）
CURRENT=$(awk '
  /^\[package\][[:space:]]*$/ { in_pkg=1; next }
  /^\[/ { in_pkg=0 }
  in_pkg && /^[[:space:]]*version[[:space:]]*=/ {
    sub(/^version[[:space:]]*=[[:space:]]*/, "")
    gsub(/"/, "")
    print
    exit
  }
' "$CARGO_TOML")

if [ -z "$CURRENT" ]; then
  echo "Error: rust-sdk/Cargo.toml から version を取得できません" >&2
  exit 1
fi

# patch +1
MAJOR=$(echo "$CURRENT" | cut -d. -f1)
MINOR=$(echo "$CURRENT" | cut -d. -f2)
PATCH=$(echo "$CURRENT" | cut -d. -f3)
if [[ ! "$MAJOR" =~ ^[0-9]+$ ]] || [[ ! "$MINOR" =~ ^[0-9]+$ ]] || [[ ! "$PATCH" =~ ^[0-9]+$ ]]; then
  echo "Error: version '$CURRENT' の形式が不正です（半角数字の X.Y.Z 形式を期待しています。プレリリース表記 0.2.0-beta.1 等は対応外）" >&2
  exit 1
fi
NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"

echo "version: $CURRENT -> $NEW_VERSION"

# 1. rust-sdk/Cargo.toml ([package] version のみ更新。他セクションの version は触らない)
awk -v new="\"$NEW_VERSION\"" '
  /^\[package\][[:space:]]*$/ { in_pkg=1; print; next }
  /^\[/ { in_pkg=0 }
  in_pkg && /^[[:space:]]*version[[:space:]]*=/ { sub(/"[^"]*"/, new) }
  { print }
' "$CARGO_TOML" > "$CARGO_TOML.tmp" && mv "$CARGO_TOML.tmp" "$CARGO_TOML"

# 2. ts-sdk/package.json
perl -pi -e "s/(\"version\"[[:space:]]*:[[:space:]]*)\"[^\"]*\"/\${1}\"$NEW_VERSION\"/" "$PACKAGE_JSON"

# 3. rust-sdk/README.md (kijuku-db = "x" の依存関係例)
perl -pi -e "s/(kijuku-db[[:space:]]*=[[:space:]]*\")[^\"]*\"/\${1}$NEW_VERSION\"/" "$README"

# 4. rust-sdk/src/bin/cli.rs (//! Version: x の docコメント)
# perl の s{}{} だと // がデリミタと衝突するため awk を使用
awk -v new="$NEW_VERSION" '/^\/\/![[:space:]]*Version:[[:space:]]*[0-9]/ { sub(/[0-9]+\.[0-9]+\.[0-9]+/, new) } { print }' "$CLI_RS" > "$CLI_RS.tmp" && mv "$CLI_RS.tmp" "$CLI_RS"

# 5. ワークスペースルートの Cargo.lock を cargo check で更新
#    perl 等での直接書き換えは改行コード（CRLF 等）やフォーマット変更に脆弱なため、cargo に解決させる。
#    rust-sdk/Cargo.toml の [package] version を上記で更新済みなので、cargo check が Cargo.lock の
#    kijuku-db エントリを NEW_VERSION に同期する（ワークスペース構成のため lock はルートに1つ）。
#    ※ ビルドに失敗する状態なら push すべきでないため、cargo check が安全網にもなる。
if ! ( cd "$PROJECT_ROOT" && cargo check --quiet ); then
  echo "Error: cargo check が失敗しました（ビルドエラー）。version を bump して push する前に修正してください。" >&2
  exit 1
fi

echo "更新ファイル:"
echo "  - rust-sdk/Cargo.toml"
echo "  - ts-sdk/package.json"
echo "  - rust-sdk/README.md"
echo "  - rust-sdk/src/bin/cli.rs"
echo "  - Cargo.lock (cargo check が更新)"

if [ "$DRY_RUN" = "1" ]; then
  echo ""
  echo "[dry-run] commit / push をスキップしました。git diff で確認してください。"
  echo "戻す場合: git restore rust-sdk/Cargo.toml ts-sdk/package.json rust-sdk/README.md rust-sdk/src/bin/cli.rs Cargo.lock"
  exit 0
fi

# commit & push
# 更新した4ファイル + cargo check で更新されたルート Cargo.lock のみを明示的に add する。
# git add -A は作業中のファイルや未追跡ファイル（一時ファイル・機密情報等）を意図せず巻き込む危険があるため使用しない。
git add "$CARGO_TOML" "$PACKAGE_JSON" "$README" "$CLI_RS" "$PROJECT_ROOT/Cargo.lock"
git commit -m "chore: bump version to $NEW_VERSION"

echo "push 中..."
# -u で初回 push 時に upstream を設定（2回目以降は無害）
git push -u origin "$BRANCH"

echo ""
echo "完了: version $NEW_VERSION を push しました。"
echo "mise run deploy でバイナリを配置すると kijuku-cli --version が $NEW_VERSION になります。"
