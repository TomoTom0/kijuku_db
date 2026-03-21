# Rust SDK利用ガイド

このガイドでは、kijuku-db Rust SDKを外部プロジェクトから利用する方法を説明します。

## インストール

### Cargo.tomlへの追加

現在、kijuku-dbはcrates.ioに公開されていません。
ローカルまたはGit経由での利用方法を以下に示します。

#### 方法1: ローカルパスを指定（開発時）

利用するプロジェクトの`Cargo.toml`に追加：

```toml
[dependencies]
kijuku-db = { path = "../kijuku_db/rust-sdk" }
```

使用例：
```rust
use kijuku_db::{KijukuDB, MediaInput, MediaType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;
    db.migrate()?;

    // メディアを作成
    let media = db.create_media(&MediaInput {
        title: "サンプルコミック".to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    })?;

    println!("作成したメディアID: {}", media.id);
    Ok(())
}
```

#### 方法2: Git URLを指定（推奨）

```toml
[dependencies]
kijuku-db = { git = "https://github.com/TomoTom0/kijuku_db", branch = "main" }
```

Gitリポジトリのサブディレクトリを指定する場合（Cargo 1.80以降が必要）：

```toml
[dependencies]
kijuku-db = { git = "https://github.com/TomoTom0/kijuku_db", directory = "rust-sdk" }
```

**注意:** kijuku-db Rust SDKは`[lib]`セクションでライブラリとして公開されているため、上記の方法でそのまま使用できます。

### ビルド

```bash
cargo build
```

依存関係が自動的に解決され、`kijuku_db`クレートがビルドされます。

### CLIからドキュメントを参照

Rust CLIからもこのガイドを参照できます：

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

## 基本的な使い方

### 1. データベースの初期化

```rust
use kijuku_db::KijukuDB;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // データベース接続を作成
    let db = KijukuDB::open("./data/kijuku.db")?;

    // スキーマを初期化（初回のみ）
    db.migrate()?;

    Ok(())
}
```

### 2. メディアの作成

```rust
use kijuku_db::{KijukuDB, MediaInput, MediaType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;
    db.migrate()?;

    let media = db.create_media(&MediaInput {
        title: "ワンピース 第1巻".to_string(),
        media_type: MediaType::Comic,
        artist: Some("尾田栄一郎".to_string()),
        series: Some("ワンピース".to_string()),
        volume_text: Some("1".to_string()),  // volume_numberは自動計算される
        path: Some("/media/comics/onepiece_v01.cbz".to_string()),
        ..Default::default()
    })?;

    println!("メディアID: {}", media.id);

    Ok(())
}
```

### 2b. メディアの取得・更新・削除

```rust
use kijuku_db::{KijukuDB, MediaUpdateInput};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // IDで1件取得
    if let Some(media) = db.get_media(1) {
        println!("{}", media.title);
    }

    // 部分更新（指定フィールドのみ更新）
    db.update_media(1, &MediaUpdateInput {
        artist: Some(Some("新しい作者名".to_string())),
        flag_exist: Some(false),
        ..Default::default()
    })?;

    // 削除
    db.delete_media(1)?;

    Ok(())
}
```

### 3. メディアの検索

```rust
use kijuku_db::{KijukuDB, MediaFilter, QueryOptions, SortKey, SortOrder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // シリーズで検索
    let filter = MediaFilter {
        series: Some("ワンピース".to_string()),
        ..Default::default()
    };

    let options = QueryOptions {
        sort_keys: vec![SortKey { field: "volume_number".to_string(), order: SortOrder::Asc }],
        ..Default::default()
    };

    let results = db.find_media(&filter, Some(&options))?;

    println!("見つかったメディア: {}件", results.len());
    for media in results {
        println!("- {}", media.title);
    }

    Ok(())
}
```

#### OR条件での検索

`or_filters`を使用すると、複雑なOR条件で検索できます：

```rust
use kijuku_db::{KijukuDB, MediaFilter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // artist="A" OR artist="B"
    let filter = MediaFilter {
        or_filters: Some(vec![
            MediaFilter {
                artist: Some("Author1".to_string()),
                ..Default::default()
            },
            MediaFilter {
                artist: Some("Author2".to_string()),
                ..Default::default()
            },
        ]),
        ..Default::default()
    };

    let results = db.find_media(&filter, None)?;
    println!("見つかったメディア: {}件", results.len());

    // 複雑なOR条件: (artist="A" AND series="X") OR (artist="B")
    let complex_filter = MediaFilter {
        or_filters: Some(vec![
            MediaFilter {
                artist: Some("A".to_string()),
                series: Some("X".to_string()),
                ..Default::default()
            },
            MediaFilter {
                artist: Some("B".to_string()),
                ..Default::default()
            },
        ]),
        ..Default::default()
    };

    let complex_results = db.find_media(&complex_filter, None)?;
    println!("複雑条件の結果: {}件", complex_results.len());

    Ok(())
}
```

**セマンティクス:**
- 同一フィルタ内の条件: AND結合
- `or_filters`間: OR結合
- ネスト可能（`or_filters`の中にさらに`or_filters`）

### 4. タグの管理

```rust
use kijuku_db::KijukuDB;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // タグを作成
    let tag = db.create_tag("お気に入り")?;

    // タグ名でタグを取得
    if let Some(existing) = db.get_tag_by_name("お気に入り") {
        println!("タグID: {}", existing.id);
    }

    // 全タグを取得
    let all_tags = db.get_all_tags()?;
    println!("全タグ数: {}", all_tags.len());

    // メディアにタグを追加
    let media_id = 1;
    db.add_tag_to_media(media_id, tag.id)?;

    // メディアからタグを削除
    db.remove_tag_from_media(media_id, tag.id)?;

    // メディアのタグを取得
    let tags = db.get_media_tags(media_id)?;
    let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
    println!("タグ: {}", tag_names.join(", "));

    // タグの使用数統計を取得
    let stats = db.get_tag_usage_stats()?;
    for s in stats {
        println!("{}: {}件", s.name, s.count);
    }

    // 未使用タグを取得
    let unused = db.find_unused_tags()?;
    println!("未使用タグ数: {}", unused.len());

    Ok(())
}
```

### 5. トランザクション

```rust
use kijuku_db::{KijukuDB, MediaInput, MediaType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    db.transaction(|db| {
        let media = db.create_media(&MediaInput {
            title: "メディア1".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })?;

        let tag = db.create_tag("新着")?;
        db.add_tag_to_media(media.id, tag.id)?;

        Ok(())
    })?;

    Ok(())
}
```

## 高度な機能

### 属性管理

メディアに任意のキー・バリューペアで拡張属性を付与できます：

```rust
use kijuku_db::{KijukuDB, AttributeValueType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;
    let media_id = 1;

    // 属性を設定（上書き）
    db.set_media_attribute(media_id, "rating", Some("5"), None)?;
    db.set_media_attribute(media_id, "note", Some("お気に入り"), Some(AttributeValueType::Text))?;

    // 属性を1件取得
    if let Some(attr) = db.get_media_attribute(media_id, "rating")? {
        println!("rating: {:?}", attr.value);
    }

    // 全属性を取得
    let attrs = db.get_media_attributes(media_id)?;
    for a in attrs {
        println!("{}: {:?}", a.key, a.value);
    }

    // 属性を削除
    db.delete_media_attribute(media_id, "rating")?;

    // 全属性を削除
    db.delete_all_media_attributes(media_id)?;

    Ok(())
}
```

### ファイル存在チェック（update_exist）

メディアの `path` に実ファイルが存在するかチェックし、`flag_exist` を更新します：

```rust
use kijuku_db::{KijukuDB, MediaFilter, UpdateExistOptions, MediaType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // 全メディアを対象に実行
    let result = db.update_exist(
        &MediaFilter::default(),
        None,
        &UpdateExistOptions { dry_run: false },
    )?;
    println!("対象: {}件, 更新: {}件", result.total, result.updated);

    // dry_run: DBを更新せず結果のみ確認
    let dry_result = db.update_exist(
        &MediaFilter { media_type: Some(MediaType::Comic), ..Default::default() },
        None,
        &UpdateExistOptions { dry_run: true },
    )?;
    for item in dry_result.items {
        if item.flag_exist_before != item.flag_exist_after {
            println!("[{}] {}: {} -> {}", item.id, item.title, item.flag_exist_before, item.flag_exist_after);
        }
        if let Some(ext) = item.found_extension {
            println!("  代替拡張子: {}", ext);
        }
        if let Some(warn) = item.page_count_warning {
            eprintln!("  警告: {}", warn);
        }
    }

    Ok(())
}
```

**ファイル存在チェックのロジック:**
- `path` が NULL → `flag_exist = false`
- `media_type = comic`: `{path}/001.{ext}` が存在すれば `flag_exist = true`。存在しない場合はフォルダ内で代替拡張子を検索
- `media_type = video / music`: `path` のファイルが存在すれば `flag_exist = true`。存在しない場合は同ディレクトリ内で `{uuid}.{任意拡張子}` を検索
- 代替拡張子が見つかった場合は `extension` も自動更新（`dry_run = false` のとき）
- `comic` で `page_count = null` の場合、実ページ数を自動設定

### バルク挿入

大量のメディアを効率的に登録：

```rust
use kijuku_db::{KijukuDB, MediaInput, MediaType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    let media_list = vec![
        MediaInput {
            title: "メディア1".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        },
        MediaInput {
            title: "メディア2".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        },
        // ... 大量のデータ
    ];

    let created = db.bulk_create_media(&media_list)?;
    println!("{}件のメディアを作成しました", created.len());

    Ok(())
}
```

### バックアップ

```rust
use kijuku_db::{KijukuDB, BackupOptions, BackupSelector, DBOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open_with_options("./data/kijuku.db", DBOptions {
        backup: Some(BackupOptions {
            enabled: true,
            interval_ms: 3_600_000, // 1時間ごと
            backup_dir: "./backups".to_string(),
        }),
        ..Default::default()
    })?;
    db.migrate()?;

    // 手動バックアップ
    if let Some(path) = db.backup()? {
        println!("バックアップ作成: {}", path);
    }

    // バックアップ一覧を取得
    let backups = db.list_backups()?;
    for b in backups {
        println!("{} ({:?})", b.name, b.created_at);
    }

    // 最新バックアップからメディアを取得（読み取り専用）
    let selector = BackupSelector::latest();
    let media = db.get_media_from_backup(1, &selector)?;
    let results = db.find_media_from_backup(&MediaFilter::default(), None, &selector)?;

    // タグをバックアップから取得
    let tag = db.get_tag_by_name_from_backup("お気に入り", &selector)?;
    let all_tags = db.get_all_tags_from_backup(&selector)?;
    let media_tags = db.get_media_tags_from_backup(1, &selector)?;

    // 属性をバックアップから取得
    let attr = db.get_media_attribute_from_backup(1, "rating", &selector)?;
    let attrs = db.get_media_attributes_from_backup(1, &selector)?;

    Ok(())
}
```

### Web GUIサーバー（CLIのみ）

Rust SDKでは、Web GUIサーバー機能はCLIツールとして提供されています：

```bash
# リリースビルド
cargo build --release

# サーバー起動
./target/release/kijuku-cli --db ./data/kijuku.db server --port 40001 --password mypassword
```

プログラムから起動する場合は、`std::process::Command`を使用してください：

```rust
use std::process::Command;

fn start_server() -> Result<(), Box<dyn std::error::Error>> {
    Command::new("./target/release/kijuku-cli")
        .args(&["--db", "./data/kijuku.db", "server", "--port", "40001"])
        .spawn()?;

    Ok(())
}
```

## エラーハンドリング

```rust
use kijuku_db::{KijukuDB, MediaInput, MediaType};

fn main() {
    let db = match KijukuDB::open("./data/kijuku.db") {
        Ok(db) => db,
        Err(e) => {
            eprintln!("データベース接続エラー: {}", e);
            return;
        }
    };

    let result = db.create_media(&MediaInput {
        title: "サンプル".to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    });

    match result {
        Ok(media) => println!("作成成功: {}", media.id),
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                eprintln!("重複したメディアです");
            } else {
                eprintln!("エラーが発生しました: {}", e);
            }
        }
    }
}
```

## パフォーマンス最適化

### トランザクションの活用

大量のデータ操作はトランザクション内で実行してください：

```rust
use kijuku_db::{KijukuDB, MediaInput, MediaType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    db.transaction(|db| {
        for i in 0..1000 {
            db.create_media(&MediaInput {
                title: format!("メディア{}", i),
                media_type: MediaType::Comic,
                ..Default::default()
            })?;
        }
        Ok(())
    })?;

    Ok(())
}
```

### バッチ処理

大量のメディアを処理する場合は、バッチサイズを調整してください：

```rust
fn process_large_dataset(db: &KijukuDB, items: Vec<MediaInput>) -> Result<(), Box<dyn std::error::Error>> {
    const BATCH_SIZE: usize = 1000;

    for chunk in items.chunks(BATCH_SIZE) {
        db.transaction(|db| {
            for item in chunk {
                db.create_media(item)?;
            }
            Ok(())
        })?;
    }

    Ok(())
}
```

## 型定義

Rust SDKは以下の主要な型を提供しています：

```rust
// メディアタイプ
pub enum MediaType {
    Comic,
    Video,
    Music,
}

// ソート順序
pub enum SortOrder {
    Asc,
    Desc,
}

// メディア情報
pub struct Media {
    pub id: i64,
    pub title: String,
    pub title_id: Option<String>,
    pub path: Option<String>,
    pub media_type: MediaType,
    pub thumbnail_path: Option<String>,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub description: Option<String>,
    pub file_size: Option<i64>,
    pub duration_sec: Option<i32>,
    pub page_count: Option<i32>,
    pub series: Option<String>,
    pub volume_number: Option<i32>,  // volume_textから自動計算（ソート・フィルタ可能）
    pub volume_text: Option<String>,
    pub volume_title: Option<String>,
    pub magazine: Option<String>,
    pub magazine_id: Option<String>,
    pub language: Option<String>,
    pub source: Option<String>,
    pub external_id: Option<String>,
    pub artist_en: Option<String>,
    pub title_en: Option<String>,
    pub chapters: Option<String>,
    pub extension: Option<String>,
    pub flag_exist: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub title_pron: Option<String>,
    pub artist_pron: Option<String>,
    pub series_pron: Option<String>,
}

// メディア作成用の入力データ
pub struct MediaInput {
    pub title: String,
    pub media_type: MediaType,
    pub title_id: Option<String>,
    pub path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub description: Option<String>,
    pub file_size: Option<i64>,
    pub duration_sec: Option<i32>,
    pub page_count: Option<i32>,
    pub series: Option<String>,
    pub volume_number: Option<i32>,  // 手動設定は無視される
    pub volume_text: Option<String>,
    pub volume_title: Option<String>,
    pub magazine: Option<String>,
    pub magazine_id: Option<String>,
    pub language: Option<String>,
    pub source: Option<String>,
    pub external_id: Option<String>,
    pub artist_en: Option<String>,
    pub title_en: Option<String>,
    pub chapters: Option<String>,
    pub extension: Option<String>,
    pub flag_exist: Option<bool>,
    pub title_pron: Option<String>,
    pub artist_pron: Option<String>,
    pub series_pron: Option<String>,
}

// 検索フィルタ
pub struct MediaFilter {
    pub title: Option<String>,
    pub title_id: Option<String>,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub media_type: Option<MediaType>,
    pub series: Option<String>,
    pub source: Option<String>,
    pub tag_ids: Option<Vec<i64>>,
    pub flag_exist: Option<bool>,
    pub language: Option<String>,
    pub magazine: Option<String>,
    pub magazine_id: Option<String>,
    pub extension: Option<String>,
    pub external_id: Option<String>,
    pub volume_title: Option<String>,
    pub title_en: Option<String>,
    pub artist_en: Option<String>,
    pub id_in: Option<Vec<i64>>,  // IDのIN句フィルタ（999件超は自動チャンク分割）
    pub or_filters: Option<Vec<MediaFilter>>,  // OR条件（ネスト可能）
}

// ソートキー
pub struct SortKey {
    pub field: String,
    pub order: SortOrder,
}

// クエリオプション
pub struct QueryOptions {
    pub sort_keys: Vec<SortKey>,  // 複数指定で多段ソート
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// タグ
pub struct Tag {
    pub id: i64,
    pub name: String,
}
```

## テスト

単体テストの例：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kijuku_db::{KijukuDB, MediaInput};

    #[test]
    fn test_create_media() -> Result<(), Box<dyn std::error::Error>> {
        let db = KijukuDB::open(":memory:")?;
        db.migrate()?;

        let media = db.create_media(&MediaInput {
            title: "テストメディア".to_string(),
            media_type: MediaType::Comic,
            ..Default::default()
        })?;

        assert_eq!(media.title, "テストメディア");
        assert_eq!(media.media_type, MediaType::Comic);

        Ok(())
    }
}
```

## トラブルシューティング

### SQLiteのビルドエラー

SQLiteのネイティブライブラリが見つからない場合：

```bash
# Ubuntu/Debian
sudo apt-get install libsqlite3-dev

# macOS
brew install sqlite3

# Windows
# rusqliteは組み込みSQLiteを使用するため、通常は不要
```

### クロスコンパイル

異なるプラットフォーム向けにビルドする場合：

```bash
# Linux向け（Windowsから）
cargo build --target x86_64-unknown-linux-gnu --release

# Windows向け（Linuxから）
cargo build --target x86_64-pc-windows-gnu --release
```

## 関連ドキュメント

- [API仕様書](../../../api.md) - TypeScript版ですが、概念は共通
- [パフォーマンスガイド](../../../PERFORMANCE.md) - ベンチマークと最適化

## 注意事項

- Rust SDKはまだTypeScript SDKほど機能が充実していません
- Web GUIサーバーはCLIツールとしてのみ利用可能
