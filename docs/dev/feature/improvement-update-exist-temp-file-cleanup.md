# update_exist 一時ファイルの自動削除仕組み

## 現状（2026-11 時点・TASK-113 更新）
- Rust SDK: `UpdateExistResult` に `Drop` による一時ファイル自動削除を実装済み（PR #44・
  `rust-sdk/src/update_exist.rs`）。in-process SDK 利用者は結果を drop するだけでクリーンアップされる。
- CLI: `updateExist`（stdin / `update-exist` サブコマンド）のレスポンスに含まれる
  `detail_file` / `updated_ids_file` はプロセス終了後も実在する（TASK-113 で Drop 回避を修正）。
  削除責任は CLI 呼び出し側（TS SDK `updateExist` と同じ契約・docs/usage/cli/README.md に明記）。
- TS SDK: 自動削除なし・呼び出し側削除責任のまま（当初契約通り・変更なし）。

## 残課題
- 呼び出し側が削除しない CLI 実行ごとに `/tmp` に `kijuku-update-exist-*` が残留する。
  リモート RPC 経由では SSH 先で CLI プロセスが都度起動されるごとにファイルが作成され
  （stdin プロトコルは1プロセス1コマンドで長寿命プロセスは存在しない）、リモート呼び出し側が
  サーバホスト上のファイルを削除できないため蓄積しうる。
- 改善案（将来検討）: 一時ファイルの TTL（有効期限）設定と期限切れファイルの定期削除
  （下記 改善案3）、またはプロセス境界ではファイル渡しをやめてインライン返却
  （checkThumbnail `details` 方式）への統一（スキーマ改版を伴うため別途検討）。

## 改善案
1. `Drop` トレイトを実装したラッパー構造体で一時ファイルを管理し、スコープ終了時に自動削除
2. 処理終了時にクリーンアップする仕組みを追加
3. 一時ファイルの TTL（有効期限）を設定し、期限切れファイルを自動削除

## 優先度
low

## 関連
- PR: #43
- Thread ID: PRRT_kwDOQt7xR86EfnOt
- タスク: TASK-199
- タスク: TASK-113
- 関連ファイル: rust-sdk/src/update_exist.rs
