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

### バックアップ復元判断支援（read-only参照・差分表示・事後ラベル/メモ）(TASK-25,26,27)

復元先を判断する手段がなく実用性に欠けていた backup 復元を見直し、3機能を追加:

**機能1: バックアップの read-only 参照基盤(TASK-25)**
- 差分バックアップ(.diff)を読み取り専用で開けるよう `with_backup_db` を拡張（基底フルから一時フルを再構成、コールバック終了後に一時ファイルと WAL 副産物を削除）
- `BackupSelector::by_id(id)` を追加（タイムスタンプ文字列でバックアップを直接指定）。CLI `restore --id`、stdin/Remote selector に `byId` を追加
- hash 系 `*_from_backup` を追加（既存 media/tag/attribute と対応化）

**機能2: バックアップとの差分表示(TASK-26)**
- `diff_with_backup(selector, options)` を追加（media/tags/media_tags/attributes/hashes を比較し added/removed/changed を算出）
- `added`=復元で復活、`removed`=復元で失われる、`changed`=復元で上書き
- `DiffOptions.detail` で `summaryOnly`/`limited{n}`/`full` を切替
- CLI `diff-backup` サブコマンド、stdin/Remote `diffBackup`

**機能3: 事後ラベル/メモ付与(TASK-27)**
- 既存バックアップに後からラベル・メモを付与（ファイル名は変更せずサイドカー `backup/meta/backup-meta.json`）
- `set_backup_label(id, label?)` / `set_backup_note(id, note?)` / `get_backup_meta(id)`
- `listBackups` はサイドカーを優先マージ（`labelSource`/`note` を返す）、間引き時に orphan エントリを掃除
- CLI `set-backup-label` / `set-backup-note` サブコマンド、stdin/Remote 対応

**ファイル:** `rust-sdk/src/{backup,lib,diff,bin/cli,remote,types,attribute,hash,tag}.rs`, `ts-sdk/src/{backup,index,diff,remote,types,attribute,hash,tag}.ts`, `docs/design/backup.md`

## Fixed

### `find_backup_by_id_in_scope` がスコープ引数を無視して Auto 固定を返すバグを修正(TASK-25)

- 検索スコープを Manual/Tmp で指定しても返り値の `scope` が常に `Auto` になっていた
- 公開化（`pub(crate)`）に伴い修正。実害は基底フル探索（Auto）に限られていたが一般性のバグ

**ファイル:** `rust-sdk/src/backup.rs`

### 差分バックアップ復元時の一時ファイル WAL 副産物（-wal/-shm）が残存するバグを修正(TASK-25)

- 差分(.diff)から一時フルDBを再構成して開く際、WAL モードの副産物 `*-wal`/`*-shm` が削除されず `backup/tmp/` に残留していた
- `with_backup_db` と `restore` の両方で `.db`/`-wal`/`-shm` の3ファイルを削除するよう修正

**ファイル:** `rust-sdk/src/{backup,lib}.rs`, `ts-sdk/src/{backup,index}.ts`

### バックアップの作成順ソートを mtime から id ベースに変更（差分復元の非決定失敗修正）(TASK-17)

- バックアップ一覧の作成順ソートが `stat.mtime` 基準だったため、同ミリ秒に作成されたフル(.db)と差分(.diff)が同 mtime になり `latest` 選択が非決定で古いフル(空状態)を選ぶことがあった。結果として差分復元で件数が 0 になる間欠的失敗の原因
- ソート基準をファイル名タイムスタンプ(id = `YYYYMMDDHHMMSS-mmm`, `lastBackupTimestampMs` で単調一意保証)の文字列比較に変更し決定論化
- TS SDK `backup.ts`(`listBackupsFiltered` / `findTodayFullBackup`) + Rust SDK `backup.rs`(`list_backups` / `find_today_full_backup`) の両方を修正
- 回帰テスト追加: TS `backup.test.ts` に `fs.utimesSync` でフル/差分の mtime を同一化し `latest` が差分を選ぶことを検証するテスト（CI フルスイートでも検出可能）

**ファイル:** `ts-sdk/src/backup.ts`, `rust-sdk/src/backup.rs`, `ts-sdk/test/integration/backup.test.ts`

## Changed
