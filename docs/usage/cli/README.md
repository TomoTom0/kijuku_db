# kijuku-cli 利用ガイド

`kijuku-cli` はきじゅくDBをターミナルから操作するためのCLIツールです。

## インストール

```bash
./scripts/dev/deploy-local.sh
```

バイナリは `~/.local/bin/kijuku-cli` にインストールされます。

## 基本的な使い方

```bash
kijuku-cli --db <dbファイルのパス> <サブコマンド> [オプション]
```

`--db` オプションを省略した場合は、カレントディレクトリの `kijuku.db` が使用されます。

## サブコマンド一覧

### stdin（デフォルト）

サブコマンドを省略した場合、標準入力からJSONコマンドを受け取って処理します。
TypeScript SDKの `RemoteKijukuDB` はこのモードを使用してSSH経由でDBを操作します。

```bash
echo '{"operation":"listBackups","params":{}}' | kijuku-cli --db ./data/kijuku.db
```

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

サムネイルを生成・更新します。`{pathのlast content親}/cover/{uuid}.jpg` に ImageMagick `convert` で高さ180px固定のサムネイルを作成し、DBの `thumbnail_path` を更新します。

```bash
# 全メディアを対象に実行
kijuku-cli --db ./data/kijuku.db update-thumbnail

# dry-run（DBを更新せず結果のみ確認）
kijuku-cli --db ./data/kijuku.db update-thumbnail --dry-run

# 既存サムネイルを強制再生成
kijuku-cli --db ./data/kijuku.db update-thumbnail --force

# フィルタを指定して対象を絞り込む
kijuku-cli --db ./data/kijuku.db update-thumbnail --filter '{"media_type":"comic"}'
```

**スキップ条件（以下のいずれかに該当する場合はスキップ）:**
- `path` が未設定
- `path` に `content` コンポーネントが含まれない
- `{path}/001.{ext}` が存在しない

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

# TypeScript SDKガイド
kijuku-cli docs ts

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
