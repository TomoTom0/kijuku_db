# ts-sdk: close 後の非同期バックアップ失敗による stderr ノイズ

## ステータス

**解決済み（TASK-112・同期化方式を採用）**

## 現状（旧実装の問題）

ts-sdk の `KijukuDB` は CUD 操作ごとに `BackupManager.recordOperation()` を **非同期（fire-and-forget）** で呼び出していた。バックアップ デフォルト有効化（TASK-23）により、短命な DB 接続（テストやスクリプト的な使い方）で CUD を行うと、`db.close()` 後にバックアップ処理が走り、`better-sqlite3` が "The database connection is not open" エラーを投げる。このエラーは `recordOperation().catch()` で握り潰されるためテストは通るが、stderr にノイズが出る。またバックアップ自体も取りこぼされる。

- 実運用でも、即座に close するような使い方をするとバックアップが取得されずエラーログが出る。
- Rust SDK は `record_operation()` が **同期** のためこの問題は起きない。ts-sdk だけ非同期設計で挙動が異なる。

## 解決方式（TASK-112）

**`recordOperation()` を同期化し、CUD 呼び出しスレッドでバックアップを完結させる（Rust と整合）**。改善案のうち「close 時の drain」ではなく同期化を選んだ。`close()` は同期シグネチャ (`void`) のまま、pending なバックアップが存在しない状態を作る。

### 実装詳細

- `BackupManager.recordOperation(): void`（同期・Rust parity）
  - トランザクション中（`db.inTransaction`）は記録しない。外側の `KijukuDB.transaction()` が commit 後に1回だけ記録する（Rust の autocommit ガードと同等）。
- 同期 DB コピー: better-sqlite3 の `VACUUM INTO ?`（パラメータバインド）による `copyDbToSync()`。非同期 Online Backup API (`db.backup()`) は不使用化。
  - `VACUUM INTO` は転送先が既に存在すると失敗するため、残骸があれば先に削除する。
- `backupAuto()`・差分作成（`createDiffBackupFile`）も同期化。手動 `backup()` はシグネチャを `async` のまま維持（既存の `await` 呼び出し互換）し、内部は同期処理に委譲。`await` は非 Promise 値に対しても問題なく動作するため、`await manager.backupAuto()` 等の既存呼び出しはそのまま動く。
- `ts-sdk/src/index.ts` の CUD 後 `recordOperation().catch(...)` な fire-and-forget 呼び出し 14 箇所を、private helper `recordBackupOperation()`（try/catch で `console.error("Backup operation failed:", err)` のみ・Rust lib.rs と同等）に集約。浮遊 Promise は残らない。
- `onProgress` オプションは API 互換のため残置するが、`VACUUM INTO` は進捗通知を持たないため現在は呼び出されない。

### トレードオフ

- CUD 操作のレイテンシがバックアップ完了分だけ増える（Rust SDK と同じ挙動・同期 SDK の一貫性を優先）。

## 回帰テスト

`ts-sdk/test/integration/backup.test.ts` の「CUD直後の即時close（TASK-112）」: バックアップ有効で migrate + createMedia + 即 `close()` の後、(a) `auto/` にファイルが実在すること、(b) `console.error` が呼ばれていないこと（stderr ノイズなし）、(c) バックアップに作成した media が含まれることを検証する。

## 優先度

medium

## 関連

- タスク: TASK-23（backup デフォルト有効化）・TASK-112（本件修正）
- 関連ファイル: ts-sdk/src/index.ts, ts-sdk/src/backup.ts
