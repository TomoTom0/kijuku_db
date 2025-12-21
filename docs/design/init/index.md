# intro

nasにあるmedia情報を蓄積するdbおよびそれを利用するためのsdkを提供する

dbのパスは.envまたは引数で指定して、sdk内でdbに接続して、関連プロジェクトはsdkを経由してdbを操作する

# tech

## language

- rust
- ts

(web guiは別で実装する。今は考えない)

## db

sqlite

# schema

mediaとしては以下に対応する
- comics
- videos
- musics

共通して持つ構造と、固有の追加の構造を持つ

## media


| Column Name | Type | Constraints | Description |
| :--- | :--- | :--- | :--- |
| `id` | INTEGER | PK, AUTOINCREMENT | メディアID |
| `title` | TEXT | NOT NULL | 作品タイトル |
| `title_id` | TEXT | NOT NULL | 作品タイトル id |
| `path` | TEXT |  | ファイル/ディレクトリの絶対パス |
| `media_type` | TEXT | NOT NULL | `comic`, `video`, `music` のいずれか |
| `thumbnail_path` | TEXT | | サムネイル画像パス |
| `artist` | TEXT | | 作者/アーティスト名 |
| `artist_id` | TEXT | | 作者/アーティスト名id |
| `description` | TEXT | | 説明文 |
| `file_size` | INTEGER | | ファイルサイズ |
| `duration_sec` | INTEGER | | 動画/音楽の再生時間（秒） |
| `page_count` | INTEGER | | コミックのページ数 |
| `series` | TEXT | | シリーズタイトル |
| `volume_number` | INTEGER | | 巻数 (コミック) |
| `volume_text` | TEXT | | 巻数 (整数またはアルファベット) |
| `volume_title` | TEXT | | 巻タイトル |
| `magazine` | TEXT | | 掲載誌 |
| `magazine_id` | TEXT | | 掲載誌_id |
| `language` | TEXT | | 言語 (ja, en, etc) |
| `source` | TEXT | | データソース |
| `external_id` | TEXT | | 外部ID |
| `artist_en` | TEXT | | 作者名 (英) |
| `title_en` | TEXT | | title(en) |
| `chapters` | TEXT | | チャプター情報 |
| `extension` | TEXT | | 拡張子 |
| `flag_exist` | BOOLEAN | DEFAULT 1 | 存在フラグ |
| `created_at` | DATETIME | DEFAULT CURRENT_TIMESTAMP | 登録日時 |
| `updated_at` | DATETIME | DEFAULT CURRENT_TIMESTAMP | 更新日時 |
| `title_pron` | TEXT | | 作品タイトル読み |
| `artist_pron` | TEXT | | 作者/アーティスト名読み |
| `series_pron` | TEXT | | シリーズタイトル読み |

他にも一時的に必要になるカラムなどもあるかもしれない。
追加カラム用のtableも用意したい。(フラグを与えた時のみそれらも利用する)

### `tags` Table
タグの定義を格納します。

| Column Name | Type | Constraints | Description |
| :--- | :--- | :--- | :--- |
| `id` | INTEGER | PK, AUTOINCREMENT | タグID |
| `name` | TEXT | UNIQUE | タグ名 |

### `media_tags` Table
メディアとタグの多対多リレーションを管理します。

| Column Name | Type | Constraints | Description |
| :--- | :--- | :--- | :--- |
| `media_id` | INTEGER | PK | `media.id` への参照 |
| `tag_id` | INTEGER | PK | `tags.id` への参照 |

