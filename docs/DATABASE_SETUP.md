# データベースセットアップガイド

このドキュメントでは、kijuku-dbのデータベース作成、データインポート、基本的な操作方法を説明します。

## 1. データベース作成とマイグレーション

### 1.1. 新規データベースの作成

Rust CLI（`kijuku-cli`）はデータベースを開く際、書込可能な対象（既定の `stg`）であればスキーマ未作成・旧バージョン時に自動でマイグレーションを実行します。新規 DB は最初の書込コマンド実行時に作成・マイグレーションされるため、明示的な `migrate` コマンドは不要です。

```bash
# 新規 DB は最初の書込コマンド（例: import）の実行時に自動作成・マイグレーションされる
# （import の詳細は §2.1 を参照）
kijuku-cli --db db/my-media.db --target stg import path/to/data.tsv
```

明示的にマイグレーションのみ実行する場合は SDK を使用します：

```typescript
import { KijukuDB } from './src/index.js';

const db = new KijukuDB('db/my-media.db');
db.migrate();
db.close();
```

### 1.2. スキーマバージョンの確認

```typescript
const db = new KijukuDB('db/my-media.db');
const version = db.getSchemaVersion();
console.log(`Current schema version: ${version}`); // 現在: 6
```

#### マイグレーション履歴

| Version | 変更内容 |
|---------|---------|
| 1 | 初期スキーマ |
| 2 | `volume_number` カラム削除 |
| 3 | `volume_number` カラム再追加（volume_textから自動計算） |
| 4 | `uuid` カラム追加（NOT NULL、自動生成） |
| 5 | `media_tags`・`media_attributes` の外部キーに `ON DELETE CASCADE` 追加（media削除時の自動カスケード削除） |
| 6 | `media_hashes` テーブル追加（ファイル内容ベースの同定・重複検出、`content_hash` BLOB + `item_uuid` FK CASCADE） |

## 2. データインポート

### 2.1. TSVファイルからのインポート

TSVファイルには以下のカラムが必要です：

- **必須**: `media_type` (comic/video/music), `title`
- **推奨**: `artist`, `path`, `language`
- **オプション**: その他のメディア属性（`tags` 列はカンマ区切りでタグとして関連付け）

```bash
# TSVファイルをインポート（1行目をヘッダとして扱う）
kijuku-cli --db db/my-media.db import tmp/data.tsv

# 例: 追加カラムをメディア属性（media_attributes）として保存
kijuku-cli --db db/my-media.db import tmp/data.tsv --additional-columns id_old,custom_field
```

### 2.2. JSONファイルからのインポート

```bash
kijuku-cli --db db/my-media.db import data.json
```

JSON形式例：

```json
[
  {
    "title": "サンプル作品",
    "media_type": "comic",
    "artist": "作者名",
    "language": "ja",
    "path": "/path/to/content"
  }
]
```

### 2.3. SDKを使用したインポート

```typescript
import { KijukuDB } from './src/index.js';
import fs from 'fs';
import { parse } from 'csv-parse/sync';

const db = new KijukuDB('db/my-media.db');

// TSVファイルを読み込み
const content = fs.readFileSync('data.tsv', 'utf-8');
const records = parse(content, {
  columns: true,
  delimiter: '\t',
  skip_empty_lines: true,
});

// バルクインポート
const mediaList = records.map(row => ({
  media_type: row.media_type,
  title: row.title,
  artist: row.artist || null,
  path: row.path || null,
  language: row.language || null,
}));

db.bulkCreateMedia(mediaList);
db.close();
```

## 3. パス設定

### 3.1. パスの一括設定

メディアのパスを一括で設定する場合は、SQLまたはSDKを使用します。

#### SQLを使用する場合

```sql
-- 例: id_oldを使ってパスを設定
UPDATE media
SET path = '/path/to/content/' ||
  (SELECT value FROM media_attributes WHERE media_attributes.media_id = media.id AND key = 'id_old')
WHERE media_type = 'comic';
```

#### SDKを使用する場合

```typescript
const db = new KijukuDB('db/my-media.db');

// 全メディアを取得
const allMedia = db.findMedia({}, { limit: 100000 });

// パスを設定
for (const media of allMedia) {
  const basePath = '/path/to/content';
  const newPath = `${basePath}/${media.id}`;

  db.updateMedia(media.id, {
    path: newPath,
    thumbnail_path: `${newPath}/thumb.jpg`,
  });
}

db.close();
```

## 4. データの検証

### 4.1. メディア数の確認

```bash
sqlite3 db/my-media.db "SELECT media_type, COUNT(*) FROM media GROUP BY media_type;"
```

または：

```typescript
const db = new KijukuDB('db/my-media.db');

const comics = db.findMedia({ media_type: 'comic' });
const videos = db.findMedia({ media_type: 'video' });
const music = db.findMedia({ media_type: 'music' });

console.log(`Comics: ${comics.length}`);
console.log(`Videos: ${videos.length}`);
console.log(`Music: ${music.length}`);
```

### 4.2. データの整合性確認

```typescript
// パスが設定されていないメディアを検索
const withoutPath = db.findMedia({}).filter(m => !m.path);
console.log(`Missing path: ${withoutPath.length} items`);

// 言語が設定されていないメディアを検索
const withoutLang = db.findMedia({}).filter(m => !m.language);
console.log(`Missing language: ${withoutLang.length} items`);
```

## 5. volume_number の自動計算

`volume_number` フィールドは `volume_text` から自動的に計算されます。

### 5.1. 動作仕様

- `volume_text` が整数（例: "5"）の場合、`volume_number` にその値が設定されます
- `volume_text` が整数でない場合、`volume_number` は `null` になります
- この計算は保存時に自動的に行われます

### 5.2. 使用例

```typescript
// volume_textを設定すると、volume_numberが自動計算される
db.createMedia({
  media_type: 'comic',
  title: 'Example Comic',
  volume_text: '5',  // volume_number は 5 に設定される
});

db.createMedia({
  media_type: 'comic',
  title: 'Another Comic',
  volume_text: '5-6',  // volume_number は null（整数でないため）
});

// volume_numberでソート可能
const results = db.findMedia(
  { media_type: 'comic' },
  { sortKeys: [{ field: 'volume_number', order: 'ASC' }] }
);
```

## 6. バックアップとメンテナンス

### 6.1. データベースのバックアップ

```bash
# シンプルなコピー
cp db/my-media.db db/my-media.db.backup

# 日付付きバックアップ
cp db/my-media.db "db/my-media.db.$(date +%Y%m%d_%H%M%S)"
```

### 6.2. VACUUMの実行

データベースを最適化してファイルサイズを削減：

```bash
sqlite3 db/my-media.db "VACUUM;"
```

## トラブルシューティング

### TSVファイルのインポートエラー

- **エラー**: カラム数が一致しない
  - **対処**: TSVファイルに必須カラム（`media_type`, `title`）が含まれているか確認してください
  - **対処**: 複数行フィールドを含むTSVは `csv-parse` ライブラリの `relax_column_count: true` オプションを使用してください

### データベースロックエラー

- **エラー**: `database is locked`
  - **対処**: 他のプロセスがデータベースを使用していないか確認してください
  - **対処**: ネットワークファイルシステム経由でのアクセスは避けてください

### パフォーマンスの問題

- 大量データのインポート時は `bulkCreateMedia()` を使用してください
- インデックスが適切に作成されているか確認してください（マイグレーションで自動作成されます）

## 参考資料

- [テストガイド](TESTING.md) - テスト実行方法
- [パフォーマンスガイド](PERFORMANCE.md) - パフォーマンステストとベンチマーク
- [API仕様書](api.md) - 詳細なAPI仕様
- [リモートテスト手順](manual-testing-remote.md) - リモート環境でのテスト方法
