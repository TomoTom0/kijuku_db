# Unreleased

## Breaking

## Added

### kijuku-cli に --version / -V フラグを追加(TASK-20)

- `kijuku-cli --version`（または `-V`）で現在のバージョンを表示できるようにした（`CARGO_PKG_VERSION` を参照）
- これまで CLI にバージョン表示機能がなく、デプロイ後のバージョン確認が不可能だった

**ファイル:** `rust-sdk/src/bin/cli.rs`, `rust-sdk/tests/cli_integration_test.rs`

### CLI利用ガイドに --backend / bulk-load / D1 の記載を追加(TASK-21)

- v0.2.0 の D1バックエンド機能が `docs/usage/cli/README.md` に未記載だったのを修正
- グローバルオプション `--backend`（`local` / `d1`）と `bulk-load` サブコマンド（ローカル→D1 バルクロード、`--chunk-size` / `--verify-only`）を追記
- D1 利用の前提（事前の `wrangler login` + `D1_ACCOUNT_ID` / `D1_DATABASE_ID` 環境変数）も明記

**ファイル:** `docs/usage/cli/README.md`

## Fixed

### バックアップの作成順ソートを mtime から id ベースに変更（差分復元の非決定失敗修正）(TASK-17)

- バックアップ一覧の作成順ソートが `stat.mtime` 基準だったため、同ミリ秒に作成されたフル(.db)と差分(.diff)が同 mtime になり `latest` 選択が非決定で古いフル(空状態)を選ぶことがあった。結果として差分復元で件数が 0 になる間欠的失敗の原因
- ソート基準をファイル名タイムスタンプ(id = `YYYYMMDDHHMMSS-mmm`, `lastBackupTimestampMs` で単調一意保証)の文字列比較に変更し決定論化
- TS SDK `backup.ts`(`listBackupsFiltered` / `findTodayFullBackup`) + Rust SDK `backup.rs`(`list_backups` / `find_today_full_backup`) の両方を修正
- 回帰テスト追加: TS `backup.test.ts` に `fs.utimesSync` でフル/差分の mtime を同一化し `latest` が差分を選ぶことを検証するテスト（CI フルスイートでも検出可能）

**ファイル:** `ts-sdk/src/backup.ts`, `rust-sdk/src/backup.rs`, `ts-sdk/test/integration/backup.test.ts`

## Changed
