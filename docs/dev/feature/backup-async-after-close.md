# ts-sdk: close 後の非同期バックアップ失敗による stderr ノイズ

## 現状

ts-sdk の `KijukuDB` は CUD 操作ごとに `BackupManager.recordOperation()` を **非同期（fire-and-forget）** で呼び出す。バックアップ デフォルト有効化（TASK-23）により、短命な DB 接続（テストやスクリプト的な使い方）で CUD を行うと、`db.close()` 後にバックアップ処理が走り、`better-sqlite3` が "The database connection is not open" エラーを投げる。このエラーは `recordOperation().catch()` で握り潰されるためテストは通るが、stderr にノイズが出る。

## 問題点

- 実運用でも、即座に close するような使い方をするとバックアップが取得されずエラーログが出る。
- Rust SDK は `record_operation()` が **同期** のためこの問題は起きない。ts-sdk だけ非同期設計で挙動が異なる。

## 改善案

- `recordOperation()` を同期化し、CUD 呼び出しスレッドでバックアップを完結させる（Rust と整合）。レイテンシ増加とのトレードオフ。
- または `close()` 時に実行中の非同期バックアップの完了を待つ。
- または「接続閉じ」エラーを警告レベルで扱い、リトライや抑止を入れる。

## 優先度
medium

## 関連
- タスク: TASK-23（backup デフォルト有効化）
- 関連ファイル: ts-sdk/src/index.ts, ts-sdk/src/backup.ts
