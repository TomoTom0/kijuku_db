# テスト構成

## ディレクトリ構成

```
rust-sdk/tests/              # Rust SDK のインテグレーションテスト
  audit_test.rs              - 監査ログ（append-only JSONL・sync/discard/observe/promote での記録・promote 後も prod 側 audit.log が残る）
  backup_test.rs             - バックアップ機能（scope/kind/auto-records.csv/restore/pre_migrate・pre_promote snapshot/copy_db_online）
  cli_integration_test.rs    - CLIコマンドのインテグレーションテスト（stdin操作・file/trash/sync-db サブコマンド）
  d1_test.rs                 - D1バックエンドのテスト
  db_options_test.rs         - DBOptions（WAL・busy_timeout・readonly 時の WAL skip）
  diff_prod_stg_test.rs      - diff_with_prod（prod RO + stg 比較・stg 編集視点のセマンティクス担保）
  integration_test.rs        - CRUD・タグ・属性などのDB操作
  observe_test.rs            - observe（機械的 promote gate・evaluate_gate 純粋関数と統合テスト）
  prod_rw_test.rs            - prod RW 一時取得（ProdRwScope・排他ロック・pre-stash 強制・RAII 解放）
  promote_test.rs            - promote コア（stg→prod 反映・gate 不合格時の prod 無触発・同一パス拒否）
  remote_test.rs             - リモートDB接続テスト（--target 配線）
  stg_session_test.rs        - stg 排他ロック・sync 元 revision 記録
  sync_test.rs               - sync（prod→stg フル複製・replicate_db/copy_db_online）

rust-sdk/src/                # 内蔵ユニットテスト（#[cfg(test)]）
  config.rs                  - config.toml 読み込み・target解決（Target/resolve_target・db_path明示必須の検証）
  file_ops.rs                - media root 配下ファイル操作（cp/mv/sync）
  trash.rs                   - 論理削除（move/list/restore/purge）
  media_path.rs              - media path 安全化・sandbox 境界
  backup.rs                  - バックアップ（BackupManager・保持ポリシー・file-less DBのbackupDir明示必須）
  migration.rs               - マイグレーション（ベーススキーマ履歴記録のトリップワイヤ含む）
  lib.rs                     - KijukuDB オープン（:memory: の既定backup無効化・明示指定時のbackupDir必須検証）
  remote.rs                  - RemoteKijukuDB（stgパス導出・target別DBパス解決・sync/discardのfrom/to具象解決）
  crud.rs                    - メディアCRUD（作成・取得・更新・削除・行変換・巻数算出）
  tag.rs                     - タグ（作成・名前検索・メディアへの付与/解除）
  attribute.rs               - メディア追加属性（CRUD・一括削除）
  hash.rs                    - メディアハッシュ（追加・取得・hex変換・item_uuid/時間範囲検索）
  search.rs                  - 検索（フィルタ条件構築・LIKEエスケープ・get_distinct_values）
  thumbnail.rs               - サムネイル（パス解決・親メディア特定・存在チェック・DB反映）
  bulk.rs                    - 一括操作（bulk create/delete/update・chunking・async版）
  bulk_load.rs               - bulk_load（MediaHashInput/MediaInput 変換・verify・ログ出力）
  update_exist.rs            - flag_exist 更新（一時ファイル連番・代替拡張子・ページ数集計）
  db_value.rs                - DbValue のSQLパラメータ変換（from_opt_*・to_rusqlite_refs）
  types.rs                   - 型変換（MediaType from_str・nullableフィールドdeserialize）
  diff.rs                    - 差分（compute_diff・diff_media/tag/attribute・ObserveOptions既定値）
  error.rs                   - エラー種別（NotFound/Validation/Parse・Display/Debug出力）
  stg_session.rs             - stg編集セッション（排他ロック・stg meta読み書き・prod revision算出）

ts-sdk/test/
  unit/                      # 純粋関数・モックのテスト（I/O なし）
    create-database.test.ts  - createDatabase() の分岐ロジック
    db-options.test.ts       - DBOptions/open 挙動（WAL・readonly skip・migrate拒否・pre_migrate snapshot・file-less DBのバックアップ既定無効化）
    diff-prompt.test.ts      - buildDiffExplanationPrompt（diff説明プロンプト構築）
    diff-summary.test.ts     - summarizeDiff（diff要約）
    error-handling.test.ts   - エラークラスとハンドラ関数
    file-ops.test.ts         - media root 配下ファイル操作（cp/mv/sync・dry-run）
    media-path.test.ts       - media path 安全化・sandbox 境界
    parse-db-path.test.ts    - parseDbPath() のパース処理
    readonly-guard.test.ts   - readonly セッションの書込拒否ガード（prod保護・stgでの制限操作拒否）
    remote-target.test.ts    - RemoteKijukuDB の --target / stgDbPath 配線
    remote-session-pool.test.ts - SSH Session 接続プール（再利用・無効化・disconnect/connectCount・ssh2 mock）
    remote-version.test.ts   - parseSemver・needsDeploy（リモートCLIバージョン比較・自動デプロイ判断）
    target-resolution.test.ts - resolveTarget/parseTarget の target 解決
    trash.test.ts            - 論理削除（move/list/restore/purge）
  integration/               # 実SQLite・実ファイルシステムを使うテスト
    attribute.test.ts        - メディア追加属性（CRUD・削除）
    backup.test.ts           - BackupManager・BackupSelector
    bulk.test.ts             - 一括操作
    config.test.ts           - config.toml 読み込み・マージ
    crud.test.ts             - メディアCRUD操作
    diff-prod-stg.test.ts    - diffWithProd（prod RO / stg 差分・stg 編集視点）
    errors.test.ts           - エラーケース
    hash.test.ts             - メディアハッシュ（追加・取得・item_uuid/時間範囲検索）
    migration.test.ts        - マイグレーション・スキーマバージョン
    promote.test.ts          - promote（stg→prod 反映・gate）
    search.test.ts           - 検索・フィルタ・ソート・ページネーション
    stg-lock.test.ts         - stg 排他ロック・revision 記録
    sync.test.ts             - sync（prod→stg フル複製・replicateDb）
    tag.test.ts              - タグCRUD・メディアへの紐付け・使用統計
    thumbnail.test.ts        - checkThumbnail・updateThumbnail・resolveThumbnailPath
    transaction.test.ts      - トランザクション
    update-exist.test.ts     - flag_exist の更新ロジック
    workflow.test.ts         - migrate → import → search の操作フロー
  e2e/                       # 外部サーバーが必要なテスト（デフォルトskip）
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
