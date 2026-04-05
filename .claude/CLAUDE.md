# kijuku_db プロジェクト固有の設定

## ビルド・デプロイ

### ローカルデプロイ（推奨）

```bash
./scripts/dev/deploy-local.sh
```

このスクリプトは以下を実行します:
- Rust SDKのリリースビルド (`cargo build --release`)
- バイナリを `~/.local/kijuku-db/bin/` にコピー
- `~/.local/bin/kijuku-cli` にシンボリックリンクを作成

ドキュメントはビルド時にバイナリに埋め込まれるため、ドキュメントを更新した場合は必ずデプロイスクリプトを実行してください。

### 個別ビルド

```bash
# Rust SDK
cd rust-sdk && cargo build --release

# TypeScript SDK
cd ts-sdk && pnpm run build
```

### テスト

```bash
# Rust SDK
cd rust-sdk && cargo test

# TypeScript SDK
cd ts-sdk && pnpm run test
```

## プロジェクト構成

- `rust-sdk/`: Rust SDK（kijuku-cli バイナリを含む）
- `ts-sdk/`: TypeScript SDK
- `docs/`: ドキュメント（ビルド時にRustバイナリに埋め込まれる）

## 設計原則

### SDKが先、CLIはSDKの薄いラッパー

機能は必ずSDK（`KijukuDB` メソッド）として実装し、CLIはそれを呼び出すだけ。

- 新機能はまず Rust SDK の `KijukuDB` に実装する
- TypeScript SDK の `KijukuDB`（ローカル）と `RemoteKijukuDB`（リモート）にも同様に公開する
- CLI サブコマンドは SDK メソッドを呼ぶだけの薄いラッパーとして実装する
- SDK に存在しない機能が CLI にあるのは実装漏れであり、即座に修正する
