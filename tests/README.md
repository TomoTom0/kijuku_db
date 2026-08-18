# テスト構成

## ディレクトリ構成

```
rust-sdk/tests/              # Rust SDK のインテグレーションテスト
  backup_test.rs             - バックアップ機能（scope/kind/auto-records.csv/restore/pre_migrate・pre_promote snapshot/copy_db_online）
  cli_integration_test.rs    - CLIコマンドのインテグレーションテスト（file/trash/sync-db サブコマンド）
  d1_test.rs                 - D1バックエンドのテスト
  db_options_test.rs         - DBOptions（WAL・busy_timeout・readonly 時の WAL skip）
  integration_test.rs        - CRUD・タグ・属性などのDB操作
  remote_test.rs             - リモートDB接続テスト（--target 配線）
  sync_test.rs               - sync（prod→stg フル複製・replicate_db/copy_db_online）

rust-sdk/src/                # 内蔵ユニットテスト（#[cfg(test)]）
  config.rs                  - config.toml 読み込み・target解決（Target/resolve_target・db_path明示必須の検証）
  file_ops.rs                - media root 配下ファイル操作（cp/mv/sync）
  trash.rs                   - 論理削除（move/list/restore/purge）
  media_path.rs              - media path 安全化・sandbox 境界
  backup.rs                  - バックアップ（BackupManager・保持ポリシー・file-less DBのbackupDir明示必須）
  migration.rs               - マイグレーション（ベーススキーマ履歴記録のトリップワイヤ含む）

ts-sdk/test/
  unit/                      # 純粋関数・モックのテスト（I/O なし）
    create-database.test.ts  - createDatabase() の分岐ロジック
    db-options.test.ts       - DBOptions/open 挙動（WAL・readonly skip・migrate拒否・pre_migrate snapshot・file-less DBのバックアップ既定無効化）
    error-handling.test.ts   - エラークラスとハンドラ関数
    file-ops.test.ts         - media root 配下ファイル操作（cp/mv/sync・dry-run）
    media-path.test.ts       - media path 安全化・sandbox 境界
    parse-db-path.test.ts    - parseDbPath() のパース処理
    remote-target.test.ts    - RemoteKijukuDB の --target / stgDbPath 配線
    remote-session-pool.test.ts - SSH Session 接続プール（再利用・無効化・disconnect/connectCount・ssh2 mock）
    target-resolution.test.ts - resolveTarget/parseTarget の target 解決
    trash.test.ts            - 論理削除（move/list/restore/purge）
  integration/               # 実SQLite・実ファイルシステムを使うテスト
    attribute.test.ts        - メディア追加属性（CRUD・削除）
    backup.test.ts           - BackupManager・BackupSelector
    bulk.test.ts             - 一括操作
    config.test.ts           - config.toml 読み込み・マージ
    crud.test.ts             - メディアCRUD操作
    errors.test.ts           - エラーケース
    migration.test.ts        - マイグレーション・スキーマバージョン
    search.test.ts           - 検索・フィルタ・ソート・ページネーション
    sync.test.ts             - sync（prod→stg フル複製・replicateDb）
    tag.test.ts              - タグCRUD・メディアへの紐付け・使用統計
    thumbnail.test.ts        - checkThumbnail・updateThumbnail・resolveThumbnailPath
    transaction.test.ts      - トランザクション
    update-exist.test.ts     - flag_exist の更新ロジック
    workflow.test.ts         - migrate → import → search の操作フロー
  e2e/                       # 外部サーバーが必要なテスト（デフォルトskip）
    cli-local.test.ts        - CLIのローカル操作
    cli-remote.test.ts       - CLIのリモート操作
    sdk-remote.test.ts       - SDKのリモート操作
```

## unit / integration の区別

| カテゴリ | 基準 |
|---|---|
| `unit/` | 外部I/Oなし。モックまたは純粋関数のテスト |
| `integration/` | 実SQLite・実ファイルシステムを使用。外部サービスは不要 |
| `e2e/` | リモートサーバーへのSSH接続が必要 |

## テスト実行

```bash
# TypeScript SDK テスト（全て）
mise run test:ts

# TypeScript SDK テスト（unit のみ）
mise run test:ts:unit

# TypeScript SDK テスト（integration のみ）
mise run test:ts:integration

# Rust SDK テスト（全て）
mise run test:rust

# Rust SDK テスト（特定ファイル）
cd rust-sdk && cargo test --test backup_test
```

## テスト更新が必要なタイミング

| 変更内容 | 更新が必要なテストファイル |
|---|---|
| バックアップ機能の変更 | `rust-sdk/tests/backup_test.rs`, `ts-sdk/test/integration/backup.test.ts` |
| config.toml 読み込みの変更 | `rust-sdk/src/config.rs` (内蔵テスト), `ts-sdk/test/integration/config.test.ts` |
| CRUD操作の変更 | `rust-sdk/tests/integration_test.rs`, `ts-sdk/test/integration/crud.test.ts` |
| タグ機能の変更 | `ts-sdk/test/integration/tag.test.ts` |
| 属性機能の変更 | `ts-sdk/test/integration/attribute.test.ts` |
| flag_exist 更新の変更 | `ts-sdk/test/integration/update-exist.test.ts` |
| 検索・フィルタの変更 | `ts-sdk/test/integration/search.test.ts` |
| 一括操作の変更 | `ts-sdk/test/integration/bulk.test.ts` |
| サムネイル機能の変更 | `ts-sdk/test/integration/thumbnail.test.ts` |
| マイグレーションの変更 | `ts-sdk/test/integration/migration.test.ts` |
| エラーハンドリングの変更 | `ts-sdk/test/unit/error-handling.test.ts`, `ts-sdk/test/integration/errors.test.ts` |
| CLIコマンドの変更 | `rust-sdk/tests/cli_integration_test.rs`, `ts-sdk/test/integration/workflow.test.ts` |
| リモートDB接続の変更 | `rust-sdk/tests/remote_test.rs`, `ts-sdk/test/e2e/sdk-remote.test.ts` |
| リモート SSH Session 接続プール（再利用・slot無効化・disconnect/connectCount）の変更 | `ts-sdk/test/unit/remote-session-pool.test.ts`, `ts-sdk/test/e2e/sdk-remote.test.ts`, `rust-sdk/tests/remote_test.rs` |
| ファイル操作（media root内 cp/mv/sync）の変更 | `rust-sdk/src/file_ops.rs`（内蔵）, `ts-sdk/test/unit/file-ops.test.ts`, `rust-sdk/tests/cli_integration_test.rs` |
| trash（論理削除）機能の変更 | `rust-sdk/src/trash.rs`（内蔵）, `ts-sdk/test/unit/trash.test.ts`, `rust-sdk/tests/cli_integration_test.rs` |
| media path 安全化の変更 | `rust-sdk/src/media_path.rs`（内蔵）, `ts-sdk/test/unit/media-path.test.ts` |
| target解決・本番DB保護（readonly/migrate保全）の変更 | `rust-sdk/src/config.rs`（内蔵）, `ts-sdk/test/unit/target-resolution.test.ts`, `rust-sdk/tests/db_options_test.rs` |
| DBOptions/open 挙動（WAL・readonly）の変更 | `ts-sdk/test/unit/db-options.test.ts`, `rust-sdk/tests/db_options_test.rs` |
| sync/DB複製（prod→stg）の変更 | `rust-sdk/tests/sync_test.rs`, `rust-sdk/tests/backup_test.rs`, `ts-sdk/test/integration/sync.test.ts`, `rust-sdk/tests/cli_integration_test.rs` |

## 命名規則

- TypeScript: `{機能名}.test.ts`
- Rust: `{機能名}_test.rs`

## 重要なテスト対象（必須）

- **バックアップ機能**: `BackupManager` の全メソッド、`BackupSelector` のスコープフィルタ、auto-records.csv の書き込み
- **config.toml の読み込み**: 優先度マージ、デフォルト値、各設定項目
- **CRUD操作**: メディア作成・取得・更新・削除
- **タグ・属性**: 紐付け・削除・統計
- **flag_exist 更新**: ファイル存在チェックと DB 更新・dry_run

## 注意事項

- E2Eテスト（`e2e/`）はリモートサーバーが必要なため、デフォルトで skip される
- バックアップのタイミング依存テストは `setTimeout` を使用するが、CI環境では不安定になる可能性がある
- Rust の config テストはユニットテストとして `rust-sdk/src/config.rs` 内に `#[cfg(test)]` で記述する
