# update-exist 詳細設計書

## モジュール構成

### Rust SDK

#### `rust-sdk/src/update_exist.rs`（新規）

公開型・関数:

```rust
pub struct UpdateExistOptions {
    pub dry_run: bool,
}

pub struct UpdateExistItemResult {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    pub flag_exist_before: bool,
    pub flag_exist_after: bool,
    pub extension_used: Option<String>,
    pub found_extension: Option<String>,
    pub page_count_warning: Option<String>,
    pub page_count_set: Option<i32>,
}

pub struct UpdateExistResult {
    pub total: usize,
    pub updated: usize,
    pub items: Vec<UpdateExistItemResult>,
}

pub fn update_exist(
    conn: &Connection,
    filter: &MediaFilter,
    query_options: Option<&QueryOptions>,
    options: &UpdateExistOptions,
) -> Result<UpdateExistResult>
```

内部関数:

| 関数名 | 説明 |
| :--- | :--- |
| `default_extension(media_type) -> &str` | media_typeごとのデフォルト拡張子を返す |
| `count_pages_in_dir(dir, ext) -> i32` | `{数字}.{ext}` 形式のファイル数を返す |
| `find_dominant_extension_in_dir(dir) -> Option<String>` | フォルダ内で最も多い `{数字}.*` の拡張子を返す |
| `find_alternative_extension_for_file(path, uuid) -> Option<String>` | 同ディレクトリ内の `{uuid}.*` を検索し拡張子を返す |
| `process_media(conn, media, options) -> Result<UpdateExistItemResult>` | 1件のメディアに対してチェック・更新を行う |

#### `rust-sdk/src/lib.rs`（変更）

- `pub mod update_exist;` を追加
- `pub use update_exist::{UpdateExistItemResult, UpdateExistOptions, UpdateExistResult};` を追加
- `KijukuDB::update_exist()` メソッドを追加

#### `rust-sdk/src/bin/cli.rs`（変更）

- `Commands` enumに `UpdateExist` バリアントを追加
- `handle_update_exist()` 関数を追加
- stdinコマンドディスパッチに `"updateExist"` を追加

### TypeScript SDK

#### `ts-sdk/src/update_exist.ts`（新規）

Rust SDKと同等のロジックをTypeScriptで実装する。

#### `ts-sdk/src/index.ts`（変更）

- `updateExist()` メソッドを追加
- 関連型をexport

## 処理フロー詳細

### `update_exist()` 全体フロー

```
1. find_media(conn, filter, query_options) で対象メディアを取得
2. 各メディアに対して process_media() を実行
3. 警告・INFOをstderrに出力
4. UpdateExistResult を返す
```

### `process_media()` フロー

```
1. extension = media.extension ?? default_extension(media.media_type)
2. path が None → flag_exist_after = false
3. path が Some(path_str) の場合:
   a. media_type == Comic:
      - first_page = "{path}/001.{ext}" が存在するか
      - 存在する → flag_exist_after = true
        - page_count チェック（後述）
      - 存在しない → find_dominant_extension_in_dir(path) で代替拡張子を探す
        - 見つかれば found_extension に設定、flag_exist_after = false（DB更新時にtrue）
   b. media_type == Video / Music:
      - path のファイルが存在するか
      - 存在する → flag_exist_after = true
      - 存在しない → find_alternative_extension_for_file(path, uuid) で代替拡張子を探す
        - 見つかれば found_extension に設定、flag_exist_after = false（DB更新時にtrue）
4. dry_run == false の場合:
   a. flag_exist を更新
   b. found_extension がある場合: extension を更新、flag_exist = true
   c. page_count_set がある場合: page_count を更新
```

### page_countチェックフロー（comic、flag_exist_after = trueのとき）

```
actual_count = count_pages_in_dir(path, ext)

if media.page_count == None:
    page_count_set = actual_count  （DB更新対象）
else if media.page_count != actual_count:
    page_count_warning = "page_count不一致: DB={db}, 実際={actual}"  （警告のみ）
```

### `find_dominant_extension_in_dir()` フロー

```
1. ディレクトリを読み込む
2. ファイル名が "{数字}.{拡張子}" 形式のものを抽出
3. 拡張子ごとにカウント
4. 最もカウントの多い拡張子を返す
```

### `find_alternative_extension_for_file()` フロー

```
1. path の親ディレクトリを取得
2. ディレクトリ内で "{uuid}.{拡張子}" 形式のファイルを探す
3. 見つかれば拡張子を返す
```

## CLIサブコマンド詳細

### コマンド形式

```
kijuku-cli --db <DB> update-exist [--dry-run] [--filter <JSON>]
```

### 引数

| 引数 | 型 | デフォルト | 説明 |
| :--- | :--- | :--- | :--- |
| `--dry-run` | flag | false | DBを更新せず出力のみ |
| `--filter` | String | `{}` | MediaFilterのJSON文字列 |

### 出力形式

stdoutにJSON:

```json
{
  "success": true,
  "data": {
    "total": 10,
    "updated": 3,
    "items": [
      {
        "id": 1,
        "uuid": "...",
        "title": "...",
        "flag_exist_before": false,
        "flag_exist_after": true,
        "extension_used": "jpg",
        "found_extension": null,
        "page_count_warning": null,
        "page_count_set": 42
      }
    ]
  }
}
```

stderrに警告・INFO:

```
WARNING [1] タイトル: page_count不一致: DB=50, 実際=42
INFO [2] タイトル: extension を png に更新
INFO [3] タイトル: extension=mp3 で発見（dry_run: 更新なし）
```

## stdinコマンド（既存のJSON入力方式）

```json
{
  "operation": "updateExist",
  "params": {
    "filter": { "media_type": "comic" },
    "options": { "limit": 100 },
    "update_options": { "dry_run": false }
  }
}
```

## テスト方針

### Rust SDK（`update_exist.rs` 内 `#[cfg(test)]`）

| テストケース | 確認内容 |
| :--- | :--- |
| `test_path_none_returns_false` | pathがNullのときflag_exist=false |
| `test_music_file_exists` | musicファイルが存在するときtrue |
| `test_music_file_not_exists` | musicファイルが存在しないときfalse |
| `test_music_alternative_extension_found` | 代替拡張子が見つかりextensionが更新される |
| `test_comic_first_page_exists` | comicの001.jpgが存在するときtrue・page_count設定 |
| `test_comic_page_count_mismatch_warning` | page_count不一致で警告 |
| `test_comic_alternative_extension` | comicの代替拡張子が検出される |
| `test_dry_run_does_not_update_db` | dry_run時はDBを更新しない |
| `test_default_extension_used_when_null` | extensionがNullのときデフォルト拡張子を使用 |
