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
use kijuku_db::{KijukuDB, MediaInput};

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

### 3. メディアの検索

```rust
use kijuku_db::{KijukuDB, MediaFilter, QueryOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // シリーズで検索
    let filter = MediaFilter {
        series: Some("ワンピース".to_string()),
        ..Default::default()
    };

    let options = QueryOptions {
        order_by: Some("volume_number".to_string()),
        order: Some("ASC".to_string()),
        ..Default::default()
    };

    let results = db.find_media(&filter, &options)?;

    println!("見つかったメディア: {}件", results.len());
    for media in results {
        println!("- {}", media.title);
    }

    Ok(())
}
```

### 4. タグの管理

```rust
use kijuku_db::KijukuDB;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // タグを作成
    let tag = db.create_tag("お気に入り")?;

    // メディアにタグを追加
    let media_id = 1;
    db.add_tag_to_media(media_id, tag.id)?;

    // メディアのタグを取得
    let tags = db.get_media_tags(media_id)?;
    let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
    println!("タグ: {}", tag_names.join(", "));

    Ok(())
}
```

### 5. トランザクション

```rust
use kijuku_db::{KijukuDB, MediaInput};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    db.connection().transaction(|| {
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

### バルク挿入

大量のメディアを効率的に登録：

```rust
use kijuku_db::{KijukuDB, MediaInput};

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
use kijuku_db::{KijukuDB, MediaInput};

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
use kijuku_db::{KijukuDB, MediaInput};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    db.connection().transaction(|| {
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
        db.connection().transaction(|| {
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
// メディア情報
pub struct Media {
    pub id: i64,
    pub title: String,
    pub media_type: String,
    pub artist: Option<String>,
    // ... その他のフィールド
}

// メディア作成用の入力データ
pub struct MediaInput {
    pub title: String,
    pub media_type: String,
    pub artist: Option<String>,
    // ... その他のフィールド
}

// 検索フィルタ
pub struct MediaFilter {
    pub title: Option<String>,
    pub media_type: Option<String>,
    pub series: Option<String>,
    // ... その他のフィールド
}

// クエリオプション
pub struct QueryOptions {
    pub order_by: Option<String>,
    pub order: Option<String>,
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
- リモートDB操作はRust SDKでは未対応（TypeScript SDKを使用してください）
- Web GUIサーバーはCLIツールとしてのみ利用可能
