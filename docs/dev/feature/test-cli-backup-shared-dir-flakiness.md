# 並列テストでの backup 共有ディレクトリ競合による flaky テスト

## 現状

`rust-sdk/tests/cli_integration_test.rs` の `test_cli_set_backup_label_and_note` が、
フルスイート並列実行時におよそ 10〜30% の確率で失敗する（単独実行では常に成功）。

- 失敗箇所: `list["data"][0]["id"]`（`data` が空、または別テストの backup と混線）
- 原因: 複数の CLI 統合テストが `backup` 操作をデフォルトの共有 backup ディレクトリ
  （`/tmp/backup` 系）に書き込み、並列実行でメタデータ（`backup-meta.json`・
  `auto-records.csv`）の読み書きが競合する。テスト側にも `/tmp/backup は並列テストで
  共有される` という注記あり（cli_integration_test.rs:359）。

## 問題点

- 本番DB保護 P0（TASK-42）など、CLI/backup 周辺を触る作業でテストを回すたびに
  ランダム失敗が発生し、実作業のノイズになる。
- P0 の WAL 有効化（lib.rs open_with_options）とは無関係（TRUE base でも再現確認済み）。
  WAL は競合窓を若干広げる可能性はあるが、根本原因は共有ディレクトリ。

## 改善案

- 各テストで `TempDir` を用意し、`backup` 操作に per-test の `backup_dir` を指定する
  （CLI の `backup` params に `backup_dir` を渡せるか要確認・無ければ `--backup-dir` 等
  のフラグ追加）。
- または `execute_cli_command` が暗黙に依存するデフォルト backup_dir を db_path 相対にし、
  テスト間で分離する。

## 優先度

medium

## 関連

- タスク: TASK-42（本番DB保護 P0）の検証中に顕在化
- 関連ファイル: `rust-sdk/tests/cli_integration_test.rs`（`test_cli_set_backup_label_and_note`:335・
  `execute_cli_command_with_env`:13）・`rust-sdk/src/backup.rs`（デフォルト backup_dir 解決）
