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

    // インメモリDB（テスト用途）
    let mem_db = KijukuDB::open_in_memory()?;

    // スキーマを初期化（初回のみ）
    db.migrate()?;

    // スキーマバージョンの確認
    let version = db.get_schema_version()?;
    println!("Schema version: {}", version);

    // テーブル一覧の取得
    let tables = db.get_tables()?;
    println!("テーブル: {:?}", tables);

    // テーブル定義の確認
    let columns = db.get_table_info("media")?;
    for col in &columns {
        println!("{} ({})", col.name, col.type_);
    }

    // 明示的に接続を閉じる（省略可、スコープを抜けると自動的に閉じる）
    db.close();

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

#### 特定IDの除外（exclude_ids）

`exclude_ids` を使うと、指定したIDを NOT IN で除外して検索できます。`id_in` の逆で、未視聴メディア取得などで「既知のIDを差し引く」用途に使います。999件超は `id_in` と同様にチャンク分割されます。

```rust
use kijuku_db::{KijukuDB, MediaFilter, MediaType, QueryOptions, SortKey, SortOrder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // 視聴済みIDを除外して未視聴メディアを取得
    let filter = MediaFilter {
        media_type: Some(MediaType::Comic),
        exclude_ids: Some(vec![1, 2, 3]),
        ..Default::default()
    };
    let options = QueryOptions {
        sort_keys: vec![SortKey { field: "title".to_string(), order: SortOrder::Asc }],
        ..Default::default()
    };
    let unwatched = db.find_media(&filter, Some(&options))?;
    println!("未視聴: {}件", unwatched.len());

    Ok(())
}
```

### 3b. フィールドのユニーク値取得

`get_distinct_values` を使うと、特定フィールドの重複なし値一覧や、複数フィールドの組み合わせ一覧を取得できます。

```rust
use kijuku_db::{KijukuDB, MediaFilter, ALLOWED_DISTINCT_FIELDS};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // 全artistの一覧（重複なし、昇順）
    let rows = db.get_distinct_values(&["artist"], &MediaFilter::default())?;
    for row in &rows {
        println!("{:?}", row[0]); // Some("Author1") or None
    }

    // コミックのシリーズ一覧（フィルタあり）
    use kijuku_db::MediaType;
    let filter = MediaFilter {
        media_type: Some(MediaType::Comic),
        ..Default::default()
    };
    let series_rows = db.get_distinct_values(&["series"], &filter)?;

    // artist × series の組み合わせ一覧
    let combos = db.get_distinct_values(&["artist", "series"], &MediaFilter::default())?;
    // => [[Some("Author1"), Some("Series1")], [Some("Author1"), None], ...]

    Ok(())
}
```

**使用可能なフィールド:** `ALLOWED_DISTINCT_FIELDS` 定数にリストされています。

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

    // 複数メディアのタグを一括取得（N+1回避: JOIN 1発）
    let media_tags = db.get_media_tags_bulk(&[1, 2, 3])?;
    for (mid, ts) in &media_tags {
        let names: Vec<&str> = ts.iter().map(|t| t.name.as_str()).collect();
        println!("Media {}: {}", mid, names.join(", "));
    }

    // タグの使用数統計を取得
    let stats = db.get_tag_usage_stats()?;
    for s in stats {
        println!("{}: {}件", s.tag_name, s.count);
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

### コンテンツハッシュ操作

ファイル内容ベースの同定・重複検出を行います。

```rust
use kijuku_db::{KijukuDB, MediaFilter, MediaHashInput, hash::{hex_to_bytes, bytes_to_hex}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("my-media.db")?;
    db.migrate()?;

    // 特定作品のハッシュを計算・登録
    let result = db.compute_media_hash(
        "item-uuid-here",
        "/path/to/media/file.mp3",
        "music",
        Some(180), // duration_sec
    )?;
    if result.skipped {
        println!("Skipped: {}", result.skip_reason.unwrap_or_default());
    } else {
        for hash in &result.hashes {
            println!("{}: {}", hash.time_range, bytes_to_hex(&hash.content_hash));
        }
    }

    // ハッシュ未計算の全作品を一括計算
    let results = db.compute_media_hashes(&MediaFilter::default(), None, false)?;
    println!("Computed {} items", results.len());

    // SHA256で検索
    let hash_bytes = hex_to_bytes("abcdef0123456789...")?;
    let found = db.find_by_content_hash(&hash_bytes)?;
    for hash in &found {
        println!("Found: {} @ {}", hash.item_uuid, hash.filename);
    }

    // 重複検出
    let dupes = db.find_duplicate_hashes()?;
    for (hash, count) in &dupes {
        println!("Duplicate: {} ({} times)", bytes_to_hex(hash), count);
    }

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
    // 変更があったIDはインライン（1000件以下）またはファイルで取得
    if let Some(ids) = &dry_result.updated_ids {
        println!("変更対象ID: {:?}", ids);
    } else if let Some(ids_file) = &dry_result.updated_ids_file {
        println!("変更対象IDファイル: {}", ids_file);
    }
    // 詳細はdetail_fileから取得
    let detail_json = std::fs::read_to_string(&dry_result.detail_file)?;
    let items: Vec<kijuku_db::UpdateExistItemResult> = serde_json::from_str(&detail_json)?;
    for item in items {
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

### サムネイルチェック・生成（check_thumbnail / update_thumbnail）

メディアのサムネイル状態をチェックし、必要に応じて生成します：

```rust
use kijuku_db::{KijukuDB, MediaFilter, ThumbnailOptions, MediaType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // サムネイル状態のチェック
    let check = db.check_thumbnail(&MediaFilter::default(), None)?;
    println!(
        "合計: {}件, OK: {}件, 未設定: {}件, ファイルなし: {}件, スキップ: {}件",
        check.total, check.ok, check.missing, check.file_not_found, check.skipped
    );
    // 詳細はdetailsフィールドから直接取得
    for item in &check.details {
        println!("[{}] {}: {:?}", item.id, item.title, item.status);
    }

    // サムネイルの生成・更新（コミック対象）
    let result = db.update_thumbnail(
        &MediaFilter { media_type: Some(MediaType::Comic), ..Default::default() },
        None,
        &ThumbnailOptions { dry_run: false, force: false },
    )?;
    println!("生成: {}件, 既存: {}件, スキップ: {}件, エラー: {}件",
        result.generated, result.already_exists, result.skipped, result.errors);

    Ok(())
}
```

**サムネイルパスの決定規則:**
- `path` に含まれる最後の `content` コンポーネントを探す
- その親ディレクトリに `cover/{uuid}.jpg` を配置
- 例: `/media/onepiece/vol1/content` → `/media/onepiece/vol1/cover/{uuid}.jpg`

**スキップ条件（以下のいずれかに該当する場合はスキップ）:**
- `path` が未設定
- `path` に `content` コンポーネントが含まれない
- `{path}/001.{ext}` が存在しない

**オプション:**
- `dry_run`: DBを更新せず結果のみ確認（ファイルも生成しない）
- `force`: 既存サムネイルがあっても再生成

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

### バルク更新・削除

```rust
use kijuku_db::{KijukuDB, BulkUpdateItem, MediaUpdateInput};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open("./data/kijuku.db")?;

    // バルク更新
    db.bulk_update_media(&[
        BulkUpdateItem {
            id: 1,
            data: MediaUpdateInput {
                artist: Some(Some("新しい作者".to_string())),
                ..Default::default()
            },
        },
        BulkUpdateItem {
            id: 2,
            data: MediaUpdateInput {
                flag_exist: Some(false),
                ..Default::default()
            },
        },
    ])?;

    // バルク削除
    db.bulk_delete_media(&[1, 2, 3])?;

    Ok(())
}
```

### バックアップ

```rust
use kijuku_db::{KijukuDB, BackupOptions, BackupSelector, DBOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = KijukuDB::open_with_options("./data/kijuku.db", DBOptions {
        backup: Some(BackupOptions {
            enabled: Some(true),
            interval_ms: Some(3_600_000), // 1時間ごと
            backup_dir: Some("./backups".to_string()),
            // DBロック時のリトライ設定（省略時はデフォルト値）
            // busy_timeout_ms: Some(5_000),
            // retry_intervals_ms: Some(vec![5_000, 10_000, 30_000, 60_000]),
            ..Default::default()
        }),
        ..Default::default()
    })?;
    db.migrate()?;

    // 手動バックアップ
    if let Some(path) = db.backup()? {
        println!("バックアップ作成: {}", path);
    }

    // ラベル付き手動バックアップ
    if let Some(path) = db.backup_with_label("before_import")? {
        println!("ラベル付きバックアップ作成: {}", path);
    }

    // バックアップ一覧を取得
    let backups = db.list_backups()?;
    for b in &backups {
        println!("{} [{:?}] ({:?})", b.name, b.scope, b.created_at);
    }

    // バックアップからリストア
    let mut db = db; // restore は &mut self を要求
    let restored = db.restore(&BackupSelector::latest())?;
    println!("リストア完了: {:?}", restored);

    // BackupManagerに直接アクセス（高度な用途）
    if let Some(manager) = db.get_backup_manager() {
        let info = manager.list_backups();
        println!("バックアップ数: {}", info.len());
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

### 復元判断支援（差分・事後ラベル/メモ）

```rust
use kijuku_db::diff::DiffOptions;

// バックアップと現在DBの差分（復元判断）
//   added: 復元で復活 / removed: 復元で失われる / changed: 復元で上書き
let diff = db.diff_with_backup(&BackupSelector::latest(), &DiffOptions::default())?;
println!("media: +{} -{} ~{}",
    diff.summary.media.added, diff.summary.media.removed, diff.summary.media.changed);

// ID（タイムスタンプ）でバックアップを直接指定
let by_id = db.diff_with_backup(&BackupSelector::by_id("20260707120000-000"), &DiffOptions::default())?;

// 既存バックアップにラベル/メモを事後付与（ファイル名は変更せず backup/meta/backup-meta.json に保存）
let id = &db.list_backups()?[0].id;
db.set_backup_label(id, Some("重要"))?;
db.set_backup_note(id, Some("作業前の状態"))?;

// list_backups はサイドカー優先で label/note を返す
for b in &db.list_backups()? {
    println!("{} label={:?} note={:?}", b.id, b.label, b.note);
}
```

### RemoteKijukuDB（SSH経由のリモート操作）

リモートサーバーのDBをSSH経由で操作できます：

```rust
use kijuku_db::{RemoteKijukuDB, RemoteConfig, MediaFilter, MediaType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let remote_db = RemoteKijukuDB::new(RemoteConfig {
        ssh_host: "example.com".to_string(),
        port: Some(22),
        username: "user".to_string(),
        db_path: Some("/path/to/kijuku.db".to_string()),
        private_key_path: Some(std::path::PathBuf::from("~/.ssh/id_rsa")),
        binary_path: None, // 自動検出
    });

    // KijukuDBと同等のAPI（全て非同期）
    remote_db.migrate()?;
    let media = remote_db.create_media(&MediaInput {
        title: "テスト".to_string(),
        media_type: MediaType::Comic,
        ..Default::default()
    })?;
    let results = remote_db.find_media(&MediaFilter::default(), None)?;

    // バックアップ操作
    let backups = remote_db.list_backups()?;
    let backup_path = remote_db.backup(Some("label".to_string()), None)?;

    // リモートのDB情報
    let tables = remote_db.get_tables()?;
    let columns = remote_db.get_table_info("media")?;

    Ok(())
}
```

**RemoteConfig:**

| フィールド | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `ssh_host` | `String` | ✓ | SSH接続先ホスト |
| `port` | `Option<u16>` | | SSHポート（デフォルト: 22） |
| `username` | `String` | ✓ | SSHユーザー名 |
| `private_key_path` | `Option<PathBuf>` | | 秘密鍵ファイルパス |
| `db_path` | `Option<String>` | | リモートのDBファイルパス |
| `binary_path` | `Option<String>` | | リモートのkijuku-cliパス |

**対応メソッド:** KijukuDBと同等の全メソッドが利用可能です（バックアップ読み取り含む）。

---

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
    pub uuid: String,
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
    pub uuid: Option<String>,
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

// タグ使用統計
pub struct TagUsageStats {
    pub tag_id: i64,
    pub tag_name: String,
    pub count: i64,
}

// メディア属性（EAVモデル）
pub struct MediaAttribute {
    pub media_id: i64,
    pub key: String,
    pub value: Option<String>,
    pub value_type: AttributeValueType,
}

// 属性値の型
pub enum AttributeValueType {
    String,
    Integer,
    Boolean,
}

// メディアハッシュ情報
pub struct MediaHash {
    pub item_uuid: String,
    pub filename: String,
    pub time_range: String,
    pub content_hash: Vec<u8>,       // SHA256（32バイト）
    pub alternative_of: Option<String>,
    pub embedding: Option<Vec<u8>>,
    pub created_at: String,
    pub updated_at: String,
}

// メディアハッシュ登録用入力
pub struct MediaHashInput {
    pub item_uuid: String,
    pub filename: String,
    pub time_range: String,
    pub content_hash: Vec<u8>,
    pub alternative_of: Option<String>,
}

// バルク更新アイテム
pub struct BulkUpdateItem {
    pub id: i64,
    pub data: MediaUpdateInput,
}

// メディア部分更新用入力（Rust SDK固有）
// 全フィールドがOptionで、null許容フィールドはOption<Option<String>>
pub struct MediaUpdateInput {
    pub uuid: Option<Option<String>>,
    pub title: Option<String>,
    pub media_type: Option<MediaType>,
    // ... 他のフィールドも同様
    pub flag_exist: Option<bool>,
}

// バックアップ情報
pub struct BackupInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub created_at: SystemTime,
    pub scope: BackupScope,
    pub kind: BackupKind,
    pub label: Option<String>,
}

pub enum BackupScope { Auto, Manual, Tmp }
pub enum BackupKind { Full, Diff }

// サムネイル結果
pub struct CheckThumbnailResult {
    pub total: usize,
    pub ok: usize,
    pub missing: usize,
    pub file_not_found: usize,
    pub skipped: usize,
    pub details: Vec<CheckThumbnailItemResult>,
}

pub struct UpdateThumbnailResult {
    pub total: usize,
    pub generated: usize,
    pub already_exists: usize,
    pub skipped: usize,
    pub errors: usize,
    pub details: Vec<UpdateThumbnailItemResult>,
}

// ファイル存在チェック
pub struct UpdateExistResult {
    pub total: usize,
    pub updated: usize,
    pub updated_ids: Option<Vec<i64>>,
    pub updated_ids_file: Option<String>,
    pub detail_file: String,
}

// テーブルカラム情報
pub struct TableColumnInfo {
    pub cid: i32,
    pub name: String,
    pub type_: String,
    pub notnull: bool,
    pub default_value: Option<String>,
    pub pk: i32,
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
        let db = KijukuDB::open_in_memory()?;
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

- [API仕様書](../../../api.md) - TS/Rust両SDKの全API仕様
- [パフォーマンスガイド](../../../PERFORMANCE.md) - ベンチマークと最適化

## 注意事項

- Rust SDKはTypeScript SDKと同等のAPIを提供しています
- RemoteKijukuDB（SSH経由のリモート操作）も利用可能です
- Web GUIサーバーはCLIツールとしてのみ利用可能
