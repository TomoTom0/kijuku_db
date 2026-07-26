# きじゅくDB - Rust SDK

メディア管理のためのSQLiteベースのデータベースSDKのRust実装です。

## 外部プロジェクトから使用する場合

**[Rust SDK利用ガイド](../docs/usage/sdk/rust/README.md)**を参照してください。

- インストール方法（ローカルパス、Git URL）
- 基本的な使い方（CRUD、検索、タグ、トランザクション）
- 高度な機能（バルク挿入、パフォーマンス最適化）
- 型定義
- エラーハンドリング
- テストの書き方
- トラブルシューティング

## 特徴

- ✅ メディア情報の管理（コミック、動画、音楽）
- ✅ CRUD操作（作成、読み取り、更新、削除）
- ✅ タグ管理
- ✅ 検索・フィルタ機能
- ✅ 一括操作
- ✅ トランザクション管理
- ✅ 型安全なAPI
- ✅ エラーハンドリング

## インストール

`Cargo.toml`に以下を追加:

```toml
[dependencies]
kijuku-db = "0.2.3"
```

## 実装状況

| 機能 | 状態 |
|------|------|
| マイグレーション | ✅ 完了 |
| CRUD操作 | ✅ 完了 |
| 検索・フィルタ | ✅ 完了 |
| タグ管理 | ✅ 完了 |
| バルク操作 | ✅ 完了 |
| テスト | ✅ 完了（23テスト） |

## 基本的な使い方

### データベースの初期化

```rust
use kijuku_db::KijukuDB;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // データベースを開く
    let db = KijukuDB::open("my_media.db")?;

    // マイグレーションを実行
    db.migrate()?;

    Ok(())
}
```

### メディアの作成

```rust
use kijuku_db::{MediaInput, MediaType};

let input = MediaInput {
    title: "サンプルコミック".to_string(),
    media_type: MediaType::Comic,
    artist: Some("サンプル作者".to_string()),
    series: Some("人気シリーズ".to_string()),
    ..Default::default()
};

let media = db.create_media(&input)?;
println!("作成されたメディアID: {}", media.id);
```

### メディアの検索

```rust
use kijuku_db::{MediaFilter, QueryOptions, SortOrder};

// メディアタイプでフィルタ
let filter = MediaFilter {
    media_type: Some(MediaType::Comic),
    ..Default::default()
};

let results = db.find_media(&filter, None)?;

// ソートとページネーション
let options = QueryOptions {
    order_by: Some("title".to_string()),
    order: Some(SortOrder::Asc),
    limit: Some(10),
    offset: Some(0),
    ..Default::default()
};

let results = db.find_media(&filter, Some(&options))?;
```

### タグ管理

```rust
// タグを作成
let tag = db.create_tag("アクション")?;

// メディアにタグを追加
db.add_tag_to_media(media.id, tag.id)?;

// メディアのタグを取得
let tags = db.get_media_tags(media.id)?;
```

### 一括操作

```rust
let bulk_data = vec![
    MediaInput {
        title: "メディア1".to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    },
    MediaInput {
        title: "メディア2".to_string(),
        media_type: MediaType::Video,
        ..Default::default()
    },
];

let results = db.bulk_create_media(&bulk_data)?;
```

## データベーススキーマ

データベースのスキーマは`schema.sql`に定義されています。

## CLIツールとして使用

```bash
# ビルド
cargo build --release

# SDK利用ガイドを表示
./target/release/kijuku-cli docs          # 概要
./target/release/kijuku-cli docs ts       # TypeScript SDK
./target/release/kijuku-cli docs rust     # Rust SDK
./target/release/kijuku-cli docs api      # API仕様書

# Web GUIサーバー起動
./target/release/kijuku-cli --db ./data/kijuku.db server --port 40001
```

**注意:** CLIは主にSSH経由で使用されることを想定しています（TypeScript SDKのRemoteKijukuDB経由）。

## 開発

```bash
# ビルド
cargo build

# ユニットテスト実行
cargo test --lib

# 統合テスト実行
cargo test --test integration_test

# 全テスト実行
cargo test

# ドキュメント生成
cargo doc --open
```

## テスト

全23テストが実装されています：
- ユニットテスト: 21テスト
- 統合テスト: 2テスト

```bash
cargo test
```

## エラーハンドリング

```rust
use kijuku_db::KijukuError;

match db.get_media(id) {
    Some(media) => println!("Found: {}", media.title),
    None => println!("Not found"),
}

match db.delete_media(9999) {
    Ok(_) => println!("Deleted"),
    Err(KijukuError::NotFound(msg)) => println!("Error: {}", msg),
    Err(e) => println!("Other error: {}", e),
}
```

## ライセンス

MIT
