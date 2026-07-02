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

**stdin操作一覧（43種類）:**

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
| | `restore` | バックアップ復元 | `selector?` |

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
```

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

### restore

バックアップから復元します。復元元のバックアップファイルパスを出力します。

```bash
# 最新バックアップから復元
kijuku-cli --db ./data/kijuku.db restore

# N番目のバックアップから復元（list-backupsの[N]に対応）
kijuku-cli --db ./data/kijuku.db restore --nth 1
```

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
