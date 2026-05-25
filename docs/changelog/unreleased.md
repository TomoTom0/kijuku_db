# Unreleased

## Breaking

### update-exist レスポンス構造変更

- `UpdateExistResult.items` を廃止。大量レスポンスによるSSLストリームハングを防ぐため
- 代わりに以下のフィールドを追加:
  - `detail_file`: 全件詳細（`UpdateExistItemResult[]`）を含むJSONファイルパス（常に書き出し）
  - `updated_ids`: 変更があったメディアのID一覧（1000件以下の場合のみインライン）
  - `updated_ids_file`: `updated_ids` が1000件超の場合のJSONファイルパス

**ファイル:**
- `rust-sdk/src/update_exist.rs`
- `ts-sdk/src/update_exist.ts`

## Added

### update-thumbnail の Video 対応（ffmpeg によるサムネイル抽出）

- `update-thumbnail` が `media_type: video` のメディアに対応（従来は Comic のみ）
- ffmpeg で動画の1秒地点からフレームを抽出し、サムネイルとして `{parent}/cover/{uuid}.jpg` に保存
- `media_type: music` はサムネイル対象外としてスキップ
- `media.path` に拡張子が含まれない場合、`{path}.{ext}` または `{uuid}.*` 形式で実際のファイルパスを自動解決

**ファイル:**
- `rust-sdk/src/thumbnail.rs`: `resolve_media_file_path` 追加、`update_media_thumbnail` の media_type 分岐
- `ts-sdk/src/thumbnail.ts`: `resolveMediaFilePath` 追加、`updateMediaThumbnail` の media_type 分岐

### getDistinctValues: フィールドの重複なし値一覧を取得する機能を追加

- `KijukuDB.getDistinctValues(fields, filter)`: 指定したフィールド群の重複なしの値の組み合わせ一覧を取得
- `RemoteKijukuDB.getDistinctValues(fields, filter)`: SSH経由の対応メソッドを追加
- Rust SDK の `KijukuDB::get_distinct_values(fields, filter)` / `RemoteKijukuDB::get_distinct_values(fields, filter)` も同様に追加
- 単一フィールド（例: `artist` の一覧）・複数フィールドの組み合わせ（例: `artist` × `series` の組み合わせ）に対応
- `filter: MediaFilter` で絞り込み可能（`find_media` と同じ形式）
- SQLインジェクション防止のため、フィールド名はホワイトリストで検証（`ALLOWED_DISTINCT_FIELDS` をエクスポート）
- 戻り値は各フィールドの値を `fields` と同じ順序で格納した配列の配列（TypeScript: `(string | null)[][]`、Rust: `Vec<Vec<Option<String>>>`）

**ファイル:**
- `rust-sdk/src/search.rs`: `get_distinct_values` 関数追加
- `rust-sdk/src/lib.rs`: `KijukuDB::get_distinct_values` メソッド追加
- `rust-sdk/src/remote.rs`: `RemoteKijukuDB::get_distinct_values` メソッド追加
- `rust-sdk/src/bin/cli.rs`: `getDistinctValues` オペレーション追加
- `ts-sdk/src/search.ts`: `getDistinctValues` 関数・`ALLOWED_DISTINCT_FIELDS` 追加
- `ts-sdk/src/index.ts`: `KijukuDB.getDistinctValues` メソッド追加
- `ts-sdk/src/remote.ts`: `RemoteKijukuDB.getDistinctValues` メソッド追加

### Rust SDK: RemoteKijukuDBにcheck_thumbnail/update_thumbnailを追加 (TASK-186)

- `RemoteKijukuDB::check_thumbnail()` / `RemoteKijukuDB::update_thumbnail()` を追加（SSH経由でサムネイル操作可能に）
- `ThumbnailOptions` の `dry_run` / `force` フィールドに `#[serde(default)]` を追加（省略時のデシリアライズエラーを防止）

**ファイル:**
- `rust-sdk/src/remote.rs`: `check_thumbnail` / `update_thumbnail` メソッド追加
- `rust-sdk/src/thumbnail.rs`: `ThumbnailOptions` フィールドに `#[serde(default)]` 追加

### TypeScript SDK に checkThumbnail / updateThumbnail を追加

- `KijukuDB.checkThumbnail(filter?, options?)`: サムネイル状態をチェック（ファイル生成なし）
- `KijukuDB.updateThumbnail(filter?, options?, thumbnailOptions?)`: ImageMagick でサムネイルを生成・更新
- `RemoteKijukuDB.checkThumbnail()` / `RemoteKijukuDB.updateThumbnail()`: SSH 経由の対応メソッドを追加
- 型定義: `ThumbnailOptions`, `CheckThumbnailResult`, `UpdateThumbnailResult` 等を `types.ts` に追加

**ファイル:**
- `ts-sdk/src/thumbnail.ts`: コアロジック（新規）
- `ts-sdk/src/index.ts`: `KijukuDB` にメソッド追加
- `ts-sdk/src/remote.ts`: `RemoteKijukuDB` にメソッド追加
- `ts-sdk/src/types.ts`: 型定義追加

### --verbose フラグを Rust CLI に追加

- `kijuku-cli --verbose`: 実行した SQL を stderr に出力（デバッグ用）
- rusqlite の `trace` 機能（feature `"trace"`）を使用
- 全サブコマンドおよび stdin モードで有効

**ファイル:**
- `rust-sdk/src/bin/cli.rs`: `--verbose` フラグ追加、全ヘルパーに伝播
- `rust-sdk/src/lib.rs`: `open_with_options` で `conn.trace()` を設定
- `rust-sdk/Cargo.toml`: rusqlite feature に `"trace"` を追加

### TypeScript CLI に --verbose フラグを追加

- `kijuku-cli ... --verbose`: TypeScript CLI でも `--verbose` フラグで SQL ログを有効化
- `parseArgs` をブールフラグ対応に修正（値なしの `--verbose` が正しく動作するよう改善）
- 環境変数 `KIJUKU_DB_VERBOSE=true` でも引き続き有効

**ファイル:**
- `ts-sdk/src/cli.ts`: `parseArgs` 修正、`--verbose` フラグの伝播



### check-thumbnail / update-thumbnail サブコマンドの追加 (TASK-161)

- `kijuku-cli check-thumbnail [--filter '{}']`: サムネイルの状態をチェック（DB・ファイル整合性確認）
- `kijuku-cli update-thumbnail [--filter '{}'] [--dry-run] [--force]`: サムネイルを生成・更新
- Rust SDK に `check_thumbnail()` / `update_thumbnail()` メソッドを追加
- CLI JSON APIに `checkThumbnail` / `updateThumbnail` オペレーションを追加
- サムネイルパスは `{pathのlast content親}/cover/{uuid}.jpg` に一貫して作成
- ImageMagick `convert` で高さ180px固定（幅は縦横比維持）、品質85でリサイズ
- `path` が未設定、`path` に `content` コンポーネントが含まれない、`001.{ext}` が存在しない場合はスキップ

**ファイル:**
- `rust-sdk/src/thumbnail.rs`: コアロジック（新規）
- `rust-sdk/src/lib.rs`: モジュール追加・KijukuDBメソッド追加
- `rust-sdk/src/bin/cli.rs`: サブコマンド・JSONハンドラ追加

### kijuku-cli に backup/list-backups/restore サブコマンドを追加 (TASK-158)

- `kijuku-cli backup <db-path> [--label <label>]`: バックアップを作成しパスを出力
- `kijuku-cli list-backups <db-path>`: バックアップ一覧を表示（index・name・scope・label）
- `kijuku-cli restore <db-path> [--nth <n>]`: バックアップから復元（デフォルトは最新）

**ファイル:**
- `rust-sdk/src/bin/cli.rs`: backup/list-backups/restore サブコマンド追加

### CLIとRemoteKijukuDBへのバックアップ機能組み込み (TASK-155)

- CLIのJSONコマンドハンドラに`backup`/`listBackups`/`restore`オペレーションを追加
- `RemoteKijukuDB`に`backup()`/`listBackups()`/`restore()`メソッドを追加（SSH経由でRust CLIを呼び出す）
- `BackupManager.getDb()`メソッドを追加（restore後のDB参照取得用）

**ファイル:**
- `rust-sdk/src/bin/cli.rs`: backup/listBackups/restoreオペレーション追加
- `ts-sdk/src/remote.ts`: RemoteKijukuDBにbackup/listBackups/restoreメソッド追加
- `ts-sdk/src/backup.ts`: `BackupManager.getDb()`追加

### 自動バックアップ機能 (TASK-44)

- 時間間隔ベースの自動バックアップ機能を実装
- `BackupManager`クラスを新規作成し、`KijukuDB`クラスに統合
- 各書き込み操作後に自動的にバックアップの必要性をチェック
- 設定された時間間隔を超えた場合に自動的にバックアップを実行
- 手動バックアップ機能（`backup()`メソッド）
- バックアップ一覧の取得機能（`listBackups()`メソッド）
- バックアップ設定のカスタマイズ（バックアップ間隔、バックアップディレクトリ等）

**ファイル:**
- `ts-sdk/src/backup.ts`: BackupManagerクラス
- `ts-sdk/src/index.ts`: KijukuDBクラスへの統合
- `ts-sdk/src/types.ts`: BackupOptions型定義
- `test/unit/backup.test.ts`: テストコード（10テスト全て成功）

### 認証付きWeb GUIサーバー (TASK-45)

#### TypeScript SDK

- Honoフレームワークベースの認証付きWebサーバーを実装
- 起動時にランダムパスワードを自動生成（または手動指定可能）
- SHA-256ベースのセッション認証
- メディア一覧・検索・詳細表示機能を提供するAPIエンドポイント
- レスポンシブなフロントエンド（HTML/CSS/JavaScript）
- CLIに`server`コマンドを追加（ローカルDBのみ対応）
- デフォルトポート: 40001

**ファイル:**
- `ts-sdk/src/server/auth.ts`: 認証・セッション管理
- `ts-sdk/src/server/index.ts`: Honoサーバー実装
- `ts-sdk/src/server/static/`: フロントエンドファイル（HTML/CSS/JS）
- `ts-sdk/src/cli.ts`: serverコマンド追加

**APIエンドポイント:**
- `POST /api/auth/login`: ログイン
- `POST /api/auth/logout`: ログアウト
- `GET /api/media`: メディア一覧・検索（認証必須）
- `GET /api/media/:id`: メディア詳細（認証必須）

#### Rust SDK

- Axumフレームワークベースの認証付きWebサーバーを実装
- TypeScript SDKと同等の機能を提供
- クッキーベースのセッション管理
- 認証ミドルウェアによる保護されたAPIエンドポイント
- グレースフルシャットダウン機能（Ctrl+C対応）
- CLIに`server`コマンドを追加
- デフォルトポート: 40001

**ファイル:**
- `rust-sdk/src/server/auth.rs`: 認証・セッション管理
- `rust-sdk/src/server/mod.rs`: Axumサーバー実装
- `rust-sdk/src/server/static/`: フロントエンドファイル（TypeScript SDKと共通）
- `rust-sdk/src/bin/cli.rs`: serverコマンド追加
- `rust-sdk/src/lib.rs`: サーバーモジュールのエクスポート

**依存関係:**
- `axum`: Webフレームワーク
- `axum-extra`: クッキーサポート
- `tokio`: 非同期ランタイム
- `tower`, `tower-http`: ミドルウェアと静的ファイルサービング
- `rand`, `sha2`, `uuid`: 認証・セキュリティ

## Fixed

### SQLite DBロック時のリトライ戦略を実装 (TASK-174)

- バックアップ開始時に DB がロックされている場合、即エラーにならず段階的なリトライを行うよう改善
- `BackupOptions` に以下のフィールドを追加（Rust SDK）:
  - `busy_timeout_ms`: 1回の試行でロック解放を待つ最大時間（デフォルト: 5,000ms）
  - `retry_intervals_ms`: リトライ間隔リスト（デフォルト: `[5_000, 10_000, 30_000, 60_000]`ms）
- リトライ間隔リストの長さがリトライ回数を決定する（デフォルト: 4回）
- `SQLITE_BUSY` / `SQLITE_LOCKED` 以外のエラーは即座に返す

**ファイル:**
- `rust-sdk/src/backup.rs`: `BackupOptions`・`BackupManager` にリトライ設定追加、`copy_db_to` にリトライロジック実装

### RemoteKijukuDB: backup・restoreのタイムアウトをDBサイズから自動計算 (TASK-170)

- `backup(label?, timeoutMs?)` / `restore(selector?, timeoutMs?)` にオプションの `timeoutMs` 引数を追加
- 省略時はリモートDBのファイルサイズを `stat` で取得し、HDD速度・バッチ設定から自動計算
- rusqliteバックアップ設定を `5ページ/250msスリープ` から `750,000ページ/10秒スリープ` に変更。大容量DBでのバックアップ時間を大幅短縮
- SSH接続タイムアウト（`readyTimeout`）をコマンド実行タイムアウトから分離し、常に30秒固定に

### RemoteKijukuDB: execCommand・uploadFileにタイムアウトを追加 (TASK-168)

- `execCommand`にタイムアウト（デフォルト30秒）を追加。タイムアウト時はSSHストリームを`stream.destroy()`でクリーンアップしてエラーを返す
- `uploadFile`（バイナリ転送）にタイムアウト（デフォルト60秒）を追加

**ファイル:**
- `ts-sdk/src/remote.ts`: `execCommand` / `uploadFile` / `executeRemoteCommand` にタイムアウト追加
- `ts-sdk/src/remote.ts`: `backup()` / `restore()` のタイムアウト動的計算・引数追加
- `rust-sdk/src/backup.rs`: `run_to_completion` のバッチ設定を変更

### `KijukuDB.restore()`がDB接続参照を更新しないバグを修正 (TASK-155)

- `restore()`呼び出し後、`BackupManager`が内部でDB接続を再オープンするが、`KijukuDB`が旧参照を保持し続けるバグを修正
- `restore()`後に`backupManager.getDb()`で新しい接続参照を取得するよう変更

**ファイル:**
- `ts-sdk/src/index.ts`: `KijukuDB.restore()`にDB参照更新を追加

### media削除時のカスケード削除漏れを修正 (TASK-138)

- `media_tags` と `media_attributes` の外部キー制約に `ON DELETE CASCADE` が欠けていたバグを修正
- migration v5 でテーブルを再作成し `ON DELETE CASCADE` を付与（既存データは保持）
- Rust SDK の `KijukuDB::open()` / `open_with_options()` / `open_in_memory()` で `PRAGMA foreign_keys = ON` を設定（接続ごとに有効化が必要なため）
- TypeScript SDK は既にコンストラクタで設定済みだったが、`ON DELETE CASCADE` 欠如によりmedia削除がFKエラーで失敗するケースがあった

**ファイル:**
- `rust-sdk/schema.sql`, `schema/schema.sql`: `ON DELETE CASCADE` 追加、Version 5 に更新
- `rust-sdk/src/migration.rs`: migration v5 追加、接続時FK有効化
- `rust-sdk/src/lib.rs`: 全 open 系メソッドで `PRAGMA foreign_keys = ON` を設定
- `ts-sdk/src/migration.ts`: migration v5 追加

### id_inフィルタのSQLiteパラメータ数上限対応 (TASK-135, TASK-136)

- `id_in`に大量のIDを渡した場合にSQLiteのパラメータ数上限（デフォルト999）を超えてエラーが発生するバグを修正
- IDリストを999件ずつチャンクに分割し、各チャンクを個別の`IN (...)`句で処理し`OR`で連結するよう変更

**ファイル:**
- `rust-sdk/src/search.rs`: チャンク分割処理を追加
- `ts-sdk/src/search.ts`: チャンク分割処理を追加

### マイグレーション version 4 の冪等化

- `uuid`列がすでに存在するDBに対してマイグレーションを再実行した場合に`ALTER TABLE`が失敗するバグを修正
- `PRAGMA table_info(media)`で列の存在を事前チェックし、存在しない場合のみ`ADD COLUMN`を実行するよう変更
- `uuid`が既に設定されているレコードを再処理しないよう`WHERE uuid IS NULL`を追加

## Changed

- CLIコマンド名を`serve`から`server`に変更（TypeScript SDK）
- サーバーはCtrl+Cでグレースフルシャットダウン可能

### bulkCreateMedia / bulkDeleteMedia / bulkUpdateMedia のバッチ分割処理

- 大量データでの DB ロック長期化・タイムアウト異常終了を防ぐため、内部で 500 件ごとにトランザクションを分割して処理するよう変更
- 公開 API の変更なし（TS SDK では `maxBatchSize` パラメータを追加、デフォルト 500）
- **挙動変化**: 501 件以上の場合、先行バッチが commit 済みの状態でエラーが発生しうる（完全な all-or-nothing ではなくなる）

**ファイル:**
- `ts-sdk/src/bulk.ts`
- `rust-sdk/src/bulk.rs`

## Technical Notes

### 自動バックアップ

- バックアップは`better-sqlite3`のバックアップAPIを使用
- デフォルトのバックアップ間隔: 1時間
- バックアップファイル名形式: `{dbname}.backup.{timestamp}.db`
- バックアップディレクトリ: デフォルトでDBと同じディレクトリ

### Web GUIサーバー

- 認証: SHA-256ハッシュ + セッションID（UUID v4）
- セッション管理: インメモリ（サーバー再起動で無効化）
- TypeScript SDK: Hono + better-sqlite3
- Rust SDK: Axum + rusqlite
- フロントエンド: Vanilla JavaScript（フレームワーク不使用）
- 対応ブラウザ: モダンブラウザ（ES6+対応）

### 制限事項

- `server`コマンドはローカルDBのみサポート（リモートDB非対応）
- セッション永続化なし（サーバー再起動でログアウト）
- HTTPS非対応（本番環境ではリバースプロキシ推奨）
