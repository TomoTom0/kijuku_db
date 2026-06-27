# Unreleased

## Breaking

## Added

## Fixed

### バックアップの作成順ソートを mtime から id ベースに変更（差分復元の非決定失敗修正）(TASK-17)

- バックアップ一覧の作成順ソートが `stat.mtime` 基準だったため、同ミリ秒に作成されたフル(.db)と差分(.diff)が同 mtime になり `latest` 選択が非決定で古いフル(空状態)を選ぶことがあった。結果として差分復元で件数が 0 になる間欠的失敗の原因
- ソート基準をファイル名タイムスタンプ(id = `YYYYMMDDHHMMSS-mmm`, `lastBackupTimestampMs` で単調一意保証)の文字列比較に変更し決定論化
- TS SDK `backup.ts`(`listBackupsFiltered` / `findTodayFullBackup`) + Rust SDK `backup.rs`(`list_backups` / `find_today_full_backup`) の両方を修正
- 回帰テスト追加: TS `backup.test.ts` に `fs.utimesSync` でフル/差分の mtime を同一化し `latest` が差分を選ぶことを検証するテスト（CI フルスイートでも検出可能）

**ファイル:** `ts-sdk/src/backup.ts`, `rust-sdk/src/backup.rs`, `ts-sdk/test/integration/backup.test.ts`

## Changed
