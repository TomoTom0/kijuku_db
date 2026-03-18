# update-exist 機能設計書

## 概要

フィルタで絞り込んだメディアの `path` に実ファイルが存在するかを確認し、`flag_exist` を更新するSDKメソッドおよびCLIサブコマンドを追加する。

## SDKメソッド

```rust
pub fn update_exist(
    &self,
    filter: &MediaFilter,
    options: Option<&QueryOptions>,
    update_options: &UpdateExistOptions,
) -> Result<UpdateExistResult>
```

### UpdateExistOptions

| フィールド | 型 | 説明 |
| :--- | :--- | :--- |
| `dry_run` | `bool` | trueの場合、DBを更新せず結果を出力のみ |

### UpdateExistResult

| フィールド | 型 | 説明 |
| :--- | :--- | :--- |
| `total` | `usize` | 対象メディア総数 |
| `updated` | `usize` | 更新されたメディア数 |
| `items` | `Vec<UpdateExistItemResult>` | 各メディアの結果 |

### UpdateExistItemResult

| フィールド | 型 | 説明 |
| :--- | :--- | :--- |
| `id` | `i64` | メディアID |
| `uuid` | `String` | UUID |
| `title` | `String` | タイトル |
| `flag_exist_before` | `bool` | 更新前のflag_exist |
| `flag_exist_after` | `bool` | 更新後のflag_exist |
| `extension_used` | `Option<String>` | チェックに使用した拡張子 |
| `found_extension` | `Option<String>` | 代替拡張子が見つかった場合 |
| `page_count_warning` | `Option<String>` | page_count不一致の警告メッセージ |
| `page_count_set` | `Option<i32>` | page_countを新規設定した値 |

## CLIサブコマンド

```
kijuku-cli --db <DB> update-exist [OPTIONS]
```

### オプション

| オプション | 説明 |
| :--- | :--- |
| `--dry-run` | DBを更新せず結果を出力のみ |
| `--filter <JSON>` | MediaFilterのJSON文字列 |

## ファイル存在チェックロジック

### pathがNULLの場合

`flag_exist = false`

### media_type = comic

- `path` はフォルダの絶対パス
- `{path}/001.{ext}` が存在すれば `flag_exist = true`
- 存在しない場合、フォルダ内の `{数字}.{任意拡張子}` を検索し代替拡張子を取得

### media_type = video / music

- `path` はファイルの絶対パス（例: `{uuid}.mp4`）
- `path` のファイルが存在すれば `flag_exist = true`
- 存在しない場合、同ディレクトリ内の `{uuid}.{任意拡張子}` を検索し代替拡張子を取得

## デフォルト拡張子（extensionがNULLの場合）

| media_type | デフォルト拡張子 |
| :--- | :--- |
| comic | `jpg` |
| video | `mp4` |
| music | `m4a` |

## 代替拡張子が見つかった場合の動作

- `dry_run = false`: `extension` および `flag_exist = true` をDBに更新し、stderrにINFOを出力
- `dry_run = true`: DBを更新せず、stderrにINFOを出力

## comicのページ数チェック

- `flag_exist = true` のとき、フォルダ内の `{数字}.{ext}` 形式ファイルを数えて実ページ数を取得
- `page_count = NULL`: 実ページ数をDBに設定する
- `page_count != NULL` かつ実ページ数と不一致: stderrに警告を出力（DBは更新しない）

## 実装ファイル

- `rust-sdk/src/update_exist.rs` - SDK実装
- `rust-sdk/src/lib.rs` - モジュール登録・公開
- `rust-sdk/src/bin/cli.rs` - CLIサブコマンド追加
- `ts-sdk/src/update_exist.ts` - TypeScript SDK実装（別途）
