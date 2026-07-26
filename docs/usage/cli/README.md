# kijuku-cli 利用ガイド

`kijuku-cli` はきじゅくDBをターミナルから操作するためのCLIツールです。

## インストール

```bash
mise run deploy
```

mise を使用しない場合は `./scripts/dev/deploy-local.sh` を直接実行することもできます。

バイナリは `~/.local/bin/kijuku-cli` にインストールされます。

## 基本的な使い方

```bash
kijuku-cli --db <dbファイルのパス> [--verbose] <サブコマンド> [オプション]
```

### グローバルオプション

| オプション | 説明 |
|-----------|------|
| `--db <path>` | データベースファイルのパス（省略時: `./kijuku.db`） |
| `--verbose` | 実行したSQLをstderrに出力する（デバッグ用） |
| `--backend <BACKEND>` | バックエンド（`local` / `d1`）。`d1` は `D1_ACCOUNT_ID` / `D1_DATABASE_ID` 環境変数と事前の `wrangler login` が必要（省略時: `local`） |
| `--media-root <path>` | ファイル操作（`file` / `trash` サブコマンド）のサンドボックス境界。media root の**絶対パス**を指定。`file` / `trash` サブコマンドで必須（未指定時は SDK がエラーを返す） |
| `--target <prod\|stg>` | 操作対象DB（省略時: `stg`）。`prod` は readonly 接続 + migrate スキップで本番DBを保護（設計 §13・[本番DB保護](../../design/db-protection.md)） |
| `--read-source <prod\|stg>` | 読込先DB（省略時: `--target` に従う）。`prod` 指定で prod を readonly で読む読込専用セッションになり、書込操作は拒否される（設計 §3.5）。優先順位: `--read-source` > `KIJUKU_READ_SOURCE`(env) > `--target` |
| `-V`, `--version` | バージョンを表示して終了する |

**例:**

```bash
kijuku-cli --db ./data/kijuku.db --verbose search
```

## サブコマンド一覧

### stdin（デフォルト）

サブコマンドを省略した場合、標準入力からJSONコマンドを受け取って処理します。
TypeScript SDKの `RemoteKijukuDB` はこのモードを使用してSSH経由でDBを操作します。

```bash
echo '{"operation":"listBackups","params":{}}' | kijuku-cli --db ./data/kijuku.db
```

**stdin操作一覧（55種類）:**

| カテゴリ | 操作名 | 説明 | 主なパラメータ |
|---------|--------|------|--------------|
| **DB操作** | `migrate` | マイグレーション実行 | なし |
| | `getSchemaVersion` | スキーマバージョン取得 | なし |
| | `getTables` | テーブル一覧取得 | なし |
| | `getTableInfo` | テーブル定義取得 | `table_name` |
| **メディアCRUD** | `createMedia` | メディア作成 | `data: MediaInput` |
| | `getMedia` | メディア取得 | `id` |
| | `updateMedia` | メディア更新 | `id`, `data` |
| | `deleteMedia` | メディア削除 | `id` |
| **メディア検索** | `findMedia` | メディア検索 | `filter`, `options?` |
| | `getDistinctValues` | ユニーク値取得 | `fields`, `filter` |
| **バルク操作** | `bulkCreateMedia` | 一括作成 | `data_list: MediaInput[]` |
| | `bulkUpdateMedia` | 一括更新 | `updates: BulkUpdateItem[]` |
| | `bulkDeleteMedia` | 一括削除 | `ids: number[]` |
| **タグ操作** | `createTag` | タグ作成 | `name` |
| | `getTagByName` | タグ名で取得 | `name` |
| | `getAllTags` | 全タグ取得 | なし |
| | `addTagToMedia` | タグ追加 | `media_id`, `tag_id` |
| | `removeTagFromMedia` | タグ削除 | `media_id`, `tag_id` |
| | `getMediaTags` | メディアのタグ取得 | `media_id` |
| | `getMediaTagsBulk` | 複数メディアのタグ一括取得（N+1回避） | `media_ids: number[]` |
| | `getTagUsageStats` | タグ使用統計 | なし |
| | `findUnusedTags` | 未使用タグ検索 | なし |
| **属性操作** | `setMediaAttribute` | 属性設定 | `media_id`, `key`, `value`, `value_type?` |
| | `getMediaAttribute` | 属性取得 | `media_id`, `key` |
| | `getMediaAttributes` | 全属性取得 | `media_id` |
| | `deleteMediaAttribute` | 属性削除 | `media_id`, `key` |
| | `deleteAllMediaAttributes` | 全属性削除 | `media_id` |
| **ファイル操作** | `updateExist` | flag_exist更新 | `filter?`, `options?`, `update_options?` |
| | `checkThumbnail` | サムネイル状態確認 | `filter?`, `options?` |
| | `updateThumbnail` | サムネイル生成 | `filter?`, `options?`, `thumbnail_options?` |
| **ハッシュ操作** | `addMediaHash` | ハッシュ登録（単件） | `input: MediaHashInput` |
| | `addMediaHashes` | ハッシュ一括登録 | `inputs: MediaHashInput[]` |
| | `getMediaHashes` | 作品の全ハッシュ取得 | `item_uuid` |
| | `getMediaHash` | 位置指定ハッシュ取得 | `item_uuid`, `filename`, `time_range` |
| | `findByContentHash` | SHA256完全一致検索 | `hash_hex` |
| | `deleteMediaHash` | ハッシュ削除（連鎖） | `item_uuid`, `filename`, `time_range` |
| | `deleteMediaHashes` | 作品ハッシュ全削除 | `item_uuid` |
| | `findDuplicateHashes` | 重複ハッシュ検出 | なし |
| | `computeMediaHash` | ハッシュ計算・登録 | `item_uuid`, `media_path`, `media_type`, `duration_sec?` |
| | `computeMediaHashes` | 一括ハッシュ計算 | `filter`, `options?`, `force` |
| **バックアップ** | `backup` | バックアップ作成 | `label?` |
| | `listBackups` | バックアップ一覧 | なし |
| | `restore` | バックアップ復元 **(b)** | `selector?`, `dryRun?` |
| | `diffBackup` | 現在DBとの差分 | `selector?`, `options?` |
| | `setBackupLabel` | 事後ラベル付与 | `id`, `label?` |
| | `setBackupNote` | 事後メモ付与 | `id`, `note?` |
| | `getBackupMeta` | 事後メタ取得 | `id` |
| **DB複製** | `sync` | prod(RO)→stg(RW) フル複製（設計 §4.2） | `from`, `to` |
| | `diffProdStg` | prod(RO)/stg 差分（promote 判断用・設計 §4.4） | `prodDbPath?`, `options?` |
| | `promote` | stg→prod 反映（promote gate 通過後・設計 §4.5） | `prodDbPath?`, `options?`, `backupOpts?` |
| **ファイル操作（media root）** | `mediaCp` | ファイル/ディレクトリ複製（dry-run ファースト・上書きは trash 経由） | `src`, `dst`, `options?` |
| | `mediaMv` | ファイル/ディレクトリ移動 **(b)**（dry-run ファースト・上書きは trash 経由） | `src`, `dst`, `options?` |
| | `mediaSync` | ディレクトリ同期（safe モード・余分/上書きは trash 経由） | `src`, `dst`, `options?` |
| | `moveToTrash` | trash へ論理移動 | `target_rel`, `operation?`, `reason?` |
| | `listTrash` | trash エントリ一覧 | なし |
| | `restoreFromTrash` | trash から復元（衝突時はエラー） | `id` |
| | `purgeTrash` | trash を物理削除 **(b)**（dry-run ファースト） | `ids?`, `dry_run?` |

> **(b) 制限操作**（設計 §9・[本番DB保護](../../design/db-protection.md)）: `mediaMv`/`purgeTrash`/`restore` は破壊的/全床上書きのため stg（既定）では**拒否**されます。prod 直接 `--target prod` で起票すると、環境が **dry-run + trash + pre-stash** のシステム gate を強制して実行します（人間承認不要・§3.2/§5.2）。`restore` は `dryRun: true` で復元差分を返し prod を変更しません。stg のリセットは `sync`（prod→stg 再複製）を使用してください。

**使用例:**

```bash
# メディア作成
echo '{"operation":"createMedia","params":{"data":{"title":"テスト","media_type":"comic"}}}' | kijuku-cli --db ./data/kijuku.db

# メディア検索（QueryOptions付き）
echo '{"operation":"findMedia","params":{"filter":{"media_type":"comic"},"options":{"sortKeys":[{"field":"title","order":"ASC"}],"limit":10}}}' | kijuku-cli --db ./data/kijuku.db

# タグ作成
echo '{"operation":"createTag","params":{"name":"お気に入り"}}' | kijuku-cli --db ./data/kijuku.db

# 複数メディアのタグ一括取得（N+1回避）
echo '{"operation":"getMediaTagsBulk","params":{"media_ids":[1,2,3]}}' | kijuku-cli --db ./data/kijuku.db

# 属性設定
echo '{"operation":"setMediaAttribute","params":{"media_id":1,"key":"rating","value":"5"}}' | kijuku-cli --db ./data/kijuku.db

# バックアップ一覧
echo '{"operation":"listBackups","params":{}}' | kijuku-cli --db ./data/kijuku.db
```

**QueryOptions対応:** `findMedia`, `updateExist`, `checkThumbnail`, `updateThumbnail`では`options`パラメータで`QueryOptions`（`sortKeys`, `limit`, `offset`）を渡して対象を絞り込めます。

---

### backup

### backup

バックアップを作成します。作成されたバックアップファイルのパスを出力します。

```bash
# バックアップを作成
kijuku-cli --db ./data/kijuku.db backup

# ラベル付きでバックアップを作成
kijuku-cli --db ./data/kijuku.db backup --label "before-migration"

# リモートDB（--db <host>:<path>）のSSH RPCタイムアウトを明示（ms）
kijuku-cli --db user@host:./data/kijuku.db backup --timeout-ms 120000
```

> **長操作のタイムアウト（`--timeout-ms <MS>`・リモートDBのみ）:** `backup` / `restore` / `diff-backup` / `diff-prod-stg` / `observe` / `sync-db` / `discard-db` は SSH RPC のタイムアウト（ms）を `--timeout-ms` で上書きできます。省略時はリモート DB のサイズから適応的に算出（HDD 50MB/s 想定・最低 60s・`restore` は現在DB退避+復元で 2 倍・TS parity）。ローカル DB では無視されます。

> **リモート CLI の自動デプロイ（リモートDBのみ・TASK-69）:** リモート DB（`--db <host>:<path>`）接続時、全 RPC の先頭でリモートの `kijuku-cli` バージョンを比較しローカルより古い場合に自動デプロイします（実体 `~/.local/kijuku-db/bin/kijuku-cli` + symlink `~/.local/bin/kijuku-cli`・ダウングレード保護付き）。手動配置は `mise run deploy`（`REMOTE_SSH_HOST` 設定時）も引き続き利用可能です。

### list-backups

バックアップ一覧を表示します。

```bash
kijuku-cli --db ./data/kijuku.db list-backups
```

出力例：

```
[0] 2023-10-26T12-00-00.db scope=auto label=-
[1] 2023-10-26T12-30-00.db scope=manual label=before-migration
```

- `[N]`: インデックス番号（`restore --nth` で指定する番号）
- `scope`: `auto`（自動）/ `manual`（手動）/ `tmp`（一時）
- `label`: ラベル（未設定の場合は `-`）

### list-pre-stashes

pre-stash（即時復旧用ロールバックファイル）一覧を表示します（設計 [§8](../../design/db-protection.md)）。`list-backups` は pre-stash を除外するため、promote/(b)操作が返す `pre_stash_path` を失った場合の発見経路として使います。

```bash
kijuku-cli --db ./data/kijuku.db list-pre-stashes
```

出力例：

```
[0] kijuku.20260724120000-000-pre_promote.db path=/path/to/backup/tmp/kijuku.20260724120000-000-pre_promote.db
```

- `path` はそのまま SDK の `BackupSelector.byPath()`（TS Remote は `restore({ type: 'byPath', path })`）で `restore` に渡せ、prod を即時復旧（§8）できます。
- pre-stash は `backup/tmp/` 配下に `*-pre_{migrate,restore,promote}.db` として保持され（既定で数日）、新しい順に表示されます。

### restore

バックアップから復元します。復元元のバックアップファイルパスを出力します。

```bash
# 最新バックアップから復元
kijuku-cli --db ./data/kijuku.db restore

# N番目のバックアップから復元（list-backupsの[N]に対応）
kijuku-cli --db ./data/kijuku.db restore --nth 1

# バックアップID（タイムスタンプ）を指定して復元
kijuku-cli --db ./data/kijuku.db restore --id 20260707120000-000
```

### diff-backup

バックアップと現在DBの差分を表示します（復元判断用）。

```bash
# 最新バックアップとの差分（件数サマリ）
kijuku-cli --db ./data/kijuku.db diff-backup

# N番目のバックアップとの差分
kijuku-cli --db ./data/kijuku.db diff-backup --nth 2

# バックアップID指定 + 全件表示
kijuku-cli --db ./data/kijuku.db diff-backup --id 20260707120000-000 --detail full

# 各カテゴリ上位10件
kijuku-cli --db ./data/kijuku.db diff-backup --detail limited=10
```

- `added`: バックアップに在り現在に無い（復元で復活）
- `removed`: 現在に在りバックアップに無い（復元で失われる）
- `changed`: 両方に在り内容が異なる（復元で上書き）

### diff-prod-stg

prod（本番・readonly）と現在DB（stg）の差分を表示します（promote 判断用・設計 §4.4・[本番DB保護](../../design/db-protection.md)）。prod を readonly 別接続で開き、stg 編集視点（promote で何が起きるか）で比較します。

```bash
# prod と stg の差分（件数サマリ・既定 limited=20）
kijuku-cli --db ./data/kijuku.stg.db diff-prod-stg --prod ./data/kijuku.db

# 各カテゴリ上位10件
kijuku-cli --db ./data/kijuku.stg.db diff-prod-stg --prod ./data/kijuku.db --detail limited=10

# テーブル別件数・分布（ProdStgDiffSummary）を表示
kijuku-cli --db ./data/kijuku.stg.db diff-prod-stg --prod ./data/kijuku.db --summarize

# LLM レビュー用 explanation prompt を stdout に出力（コピペ可能）
kijuku-cli --db ./data/kijuku.stg.db diff-prod-stg --prod ./data/kijuku.db --prompt
```

- `added`: stg のみ（promote で prod に追加）
- `removed`: prod のみ（promote で prod から削除）
- `changed`: 両方で異なる（promote で prod が上書き）

### set-backup-label / set-backup-note

既存バックアップにラベル・メモを事後付与します（`backup/meta/backup-meta.json` に保存）。

```bash
# ラベル付与
kijuku-cli --db ./data/kijuku.db set-backup-label --id 20260707120000-000 --label "重要"

# メモ付与
kijuku-cli --db ./data/kijuku.db set-backup-note --id 20260707120000-000 --note "作業前の状態"

# ラベル/メモをクリア（--label/--note を省略）
kijuku-cli --db ./data/kijuku.db set-backup-label --id 20260707120000-000
```

付与したラベル・メモは `list-backups` で表示されます。

### sync-db

prod(RO)→stg(RW) の DB フル複製を行います（設計 §4.2・本番DB保護 P1）。LLM 編集用の stg を prod から最新化します。stg 接続を開かず、Online Backup API で prod を読み取り専用コピーして stg を新規生成（既存 stg は完全上書き）します。

```bash
# prod → stg を複製
kijuku-cli --db ./data/kijuku.db sync-db --from ./data/kijuku.db --to ./data/kijuku.stg.db

# または stdin 操作で
echo '{"operation":"sync","params":{"from":"./data/kijuku.db","to":"./data/kijuku.stg.db"}}' | kijuku-cli --db ./data/kijuku.db
```

- `--from`/`--to` を省略した場合は環境変数（`KIJUKU_DB_PATH`/`KIJUKU_STG_DB_PATH`）とデフォルトから解決します
- `--from` と `--to` が同一パスの場合はエラーになります
- **排他前提**: stg（`--to`）に接続中のプロセスがないこと（呼出側の責任）

### check-thumbnail

メディアのサムネイル状態をチェックします。DBの `thumbnail_path` と実ファイルの整合性を確認します。

```bash
# 全メディアを対象に確認
kijuku-cli --db ./data/kijuku.db check-thumbnail

# フィルタを指定して対象を絞り込む
kijuku-cli --db ./data/kijuku.db check-thumbnail --filter '{"media_type":"comic"}'
```

結果（JSON）の主なフィールド：
- `total`: 対象件数
- `ok`: サムネイルが設定されファイルも存在する件数
- `missing`: `thumbnail_path` が未設定または期待パスと不一致の件数
- `file_not_found`: `thumbnail_path` は設定されているがファイルが存在しない件数
- `skipped`: `path` 未設定・`content` コンポーネントなしなど条件を満たさない件数
- `detail_file`: 全件詳細（`CheckThumbnailItemResult[]`）を含む一時JSONファイルパス

### update-thumbnail

サムネイルを生成・更新します。メディアタイプに応じて自動的に処理を切り替えます。

| media_type | 処理 |
|-----------|------|
| `comic` | ImageMagick `convert` で `{path}/001.{ext}` を高さ180pxにリサイズ |
| `video` | ffmpeg で動画の1秒地点からフレームを抽出 |
| `music` | スキップ（サムネイル対象外） |

```bash
# 全メディアを対象に実行
kijuku-cli --db ./data/kijuku.db update-thumbnail

# dry-run（DBを更新せず結果のみ確認）
kijuku-cli --db ./data/kijuku.db update-thumbnail --dry-run

# 既存サムネイルを強制再生成
kijuku-cli --db ./data/kijuku.db update-thumbnail --force

# フィルタを指定して対象を絞り込む
kijuku-cli --db ./data/kijuku.db update-thumbnail --filter '{"media_type":"comic"}'
kijuku-cli --db ./data/kijuku.db update-thumbnail --filter '{"media_type":"video"}'
```

**スキップ条件（以下のいずれかに該当する場合はスキップ）:**
- `path` が未設定
- `path` に `content` コンポーネントが含まれない
- Comic: `{path}/001.{ext}` が存在しない
- Video: 動画ファイルが存在しない（`path` に拡張子がない場合は `{uuid}.*` から自動解決を試行）

**サムネイルパスの決定規則:**
`path` に含まれる最後の `content` コンポーネントを探し、その親ディレクトリに `cover/{uuid}.jpg` を配置します。

```
/media/onepiece/vol1/content  →  /media/onepiece/vol1/cover/{uuid}.jpg
```

### update-exist

メディアの `flag_exist` をファイルの実在状態に基づいて更新します。

```bash
# 全メディアを対象に実行
kijuku-cli --db ./data/kijuku.db update-exist

# dry-run（DBを更新せず結果のみ確認）
kijuku-cli --db ./data/kijuku.db update-exist --dry-run

# フィルタを指定して対象を絞り込む
kijuku-cli --db ./data/kijuku.db update-exist --filter '{"media_type":"comic"}'
```

### file

media root 配下のファイル操作（`cp` / `mv` / `sync`）。dry-run ファーストで、上書き・余分ファイルは trash 経由で退避されます。`--media-root` が必須です。結果は1行JSON（`CommandResponse`）で出力されます。

```bash
# dry-run（デフォルト・変更せず計画のみ出力）
kijuku-cli --db ./data/kijuku.db --media-root /media file cp a/b.jpg c/b.jpg

# 変更を適用
kijuku-cli --db ./data/kijuku.db --media-root /media file cp a/b.jpg c/b.jpg --apply

# 移動・同期も同様
kijuku-cli --db ./data/kijuku.db --media-root /media file mv a/b.jpg c/b.jpg --apply
kijuku-cli --db ./data/kijuku.db --media-root /media file sync src/ dst/ --apply
```

- `src` / `dst`: media root からの相対パス
- `--apply`: 実際に変更を適用（省略時は dry-run）
- `--update-db`: DB の `Media.path` を追従（当面はフラグのみ受付）

パストラバーサル（`../` や絶対パスによる root 外アクセス）や保護パス（`.trash` 直下）への操作は SDK が拒否します。

### trash

trash（論理削除）の操作（`move` / `list` / `restore` / `purge`）。`--media-root` が必須です。結果は1行JSONで出力されます。

```bash
# trash へ移動（論理削除・物理削除はしない）
kijuku-cli --db ./data/kijuku.db --media-root /media trash move old/vid.mp4 --reason "cleanup"

# trash 操作の原因を指定（delete / overwrite / sync_extra。省略時: delete）
kijuku-cli --db ./data/kijuku.db --media-root /media trash move old/vid.mp4 --operation overwrite

# trash エントリ一覧
kijuku-cli --db ./data/kijuku.db --media-root /media trash list

# trash から復元（元の位置へ・衝突時はエラー）
kijuku-cli --db ./data/kijuku.db --media-root /media trash restore <id>

# trash を物理削除（dry-run）
kijuku-cli --db ./data/kijuku.db --media-root /media trash purge --dry-run

# 特定 ID のみ物理削除（複数指定可）・適用
kijuku-cli --db ./data/kijuku.db --media-root /media trash purge --id <id1> --id <id2>
```

- `move`: 論理削除（`file cp/mv/sync` の上書き時にも内部で使われます）。`purge` と違い物理削除しません
- `restore`: trash から元の位置へ復元。同名ファイルが既存だとエラーになります
- `purge`: 物理削除。`--id` 省略時は全エントリ。`--dry-run` で対象のみ確認

### hash

コンテンツハッシュ（SHA256）の操作を行います。ファイル内容ベースでメディアを同定・重複検出できます。

#### hash compute

ハッシュを計算してDBに登録します。

```bash
# 特定作品のハッシュを計算
kijuku-cli --db ./data/kijuku.db hash compute --uuid <item_uuid>

# ハッシュ未計算の全作品を計算
kijuku-cli --db ./data/kijuku.db hash compute --all

# 既存ハッシュがあっても再計算
kijuku-cli --db ./data/kijuku.db hash compute --all --force

# フィルタで対象を絞り込む
kijuku-cli --db ./data/kijuku.db hash compute --all --filter '{"media_type":"comic"}'
```

メディアタイプごとの計算内容:

| media_type | 計算内容 |
|-----------|---------|
| `music` | ファイル全体hash + 先頭30秒hash |
| `video` | ファイル全体hashのみ |
| `comic` | 各ページ画像hash + 全体hash（全ページhash結合） |

#### hash list

特定作品のハッシュ一覧を表示します。

```bash
kijuku-cli --db ./data/kijuku.db hash list --uuid <item_uuid>
```

#### hash find

SHA256ハッシュ値で検索します。

```bash
kijuku-cli --db ./data/kijuku.db hash find --hash <sha256_hex>
```

#### hash duplicates

重複するコンテンツハッシュを検出します。

```bash
kijuku-cli --db ./data/kijuku.db hash duplicates
```

### bulk-load

ローカルDB（`--db`）の全データを D1 へバルクロード（移行）し、件数・内容一致を検証します。`--backend d1` が必要で、source はローカル SQLite、dest は D1 です。

前提: 事前に `wrangler login` を実行し、`D1_ACCOUNT_ID` / `D1_DATABASE_ID` 環境変数を設定しておく必要があります。

```bash
# ローカル → D1 へバルクロード（件数・内容一致を検証）
kijuku-cli --db ./data/kijuku.db --backend d1 bulk-load

# source 読み出しのページサイズを指定（省略時 500）
kijuku-cli --db ./data/kijuku.db --backend d1 bulk-load --chunk-size 1000

# 転送せず、既存の D1 に対する検証のみ行う
kijuku-cli --db ./data/kijuku.db --backend d1 bulk-load --verify-only
```

`--backend d1` を指定すれば、stdin（デフォルト）モードの各 operation（`createMedia` / `findMedia` など）も D1 に対して実行できます。

### server

認証付きWeb GUIサーバーを起動します。

```bash
# デフォルト設定で起動（ポート: 40001、パスワード自動生成）
kijuku-cli --db ./data/kijuku.db server

# ポートとパスワードを指定
kijuku-cli --db ./data/kijuku.db server --port 8080 --password mypassword
```

### docs

ドキュメントを表示します。

```bash
# SDK選択ガイド（デフォルト）
kijuku-cli docs
kijuku-cli docs sdk       # 同上

# TypeScript SDKガイド
kijuku-cli docs ts
kijuku-cli docs typescript  # 同上

# Rust SDKガイド
kijuku-cli docs rust

# API仕様書
kijuku-cli docs api
```

## 関連ドキュメント

- [SDK選択ガイド](../sdk/README.md)
- [Rust SDK利用ガイド](../sdk/rust/README.md)
- [TypeScript SDK利用ガイド](../sdk/ts/README.md)
- [API仕様書](../../api.md)
