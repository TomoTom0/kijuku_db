# パフォーマンスベンチマーク

きじゅくDB TypeScript SDK（ローカルSQLite / better-sqlite3）のパフォーマンス測定結果です。Rust SDK、D1（Cloudflare）、リモート接続（RemoteKijukuDB）は本ベンチマークの対象外です。

## 実行環境

- Node.js v22.19.0
- better-sqlite3 ^11系
- OS: Linux x64

## ベンチマーク結果サマリ

### 大量データ挿入

トランザクションを使用することで大幅なパフォーマンス改善が見られます。

| データ件数 | 通常の挿入 | トランザクション付き | bulkCreateMedia | TX速度改善 | Bulk速度改善 |
|-----------|-----------|---------------------|-----------------|-----------|-------------|
| 100件     | 1,221ms   | 14ms                | 21ms            | 86.3x     | 59.5x       |
| 1,000件   | 12,842ms  | 23ms                | 66ms            | 570.4x    | 195.0x      |
| 10,000件  | 127,680ms | 70ms                | 487ms           | 1,821.6x  | 262.4x      |

#### 重要な知見

- トランザクションを使用すると、1,000件以上のデータ挿入で500倍以上の速度改善
- 10,000件のデータでは通常の挿入で約2分かかるが、トランザクション使用で0.07秒に短縮
- `bulkCreateMedia`関数は内部で500件ごとにトランザクションを分割して処理し、長時間のDBロック占有を防ぎながら高速な一括挿入を実現

### 検索クエリ

データ件数が増えても検索速度は非常に高速で、適切なインデックスが効いています。

| データ件数 | ID検索 | タイトル検索 | 作者検索 | 複合条件検索 | ページネーション(100件) |
|-----------|--------|-------------|---------|-------------|----------------------|
| 100件     | 0.26ms | 0.59ms      | 0.09ms  | 0.09ms      | 0.78ms               |
| 1,000件   | 0.09ms | 0.17ms      | 0.18ms  | 0.21ms      | 0.64ms               |
| 10,000件  | 0.14ms | 0.92ms      | 1.33ms  | 1.57ms      | 0.71ms               |

#### 重要な知見

- ID検索は常に1ms未満で、PRIMARY KEYインデックスが効いている
- 10,000件のデータでも全ての検索が2ms以下で完了
- ページネーション（LIMIT/OFFSET）も非常に高速で、0.7ms程度

### 更新操作

更新操作でもトランザクションの使用が効果的です。

| データ件数 | 単一更新(100件) | トランザクション付き更新(100件) | 速度改善 |
|-----------|----------------|-------------------------------|---------|
| 100件     | 1,226ms        | 13ms                          | 97.7x   |
| 1,000件   | 1,303ms        | 15ms                          | 89.7x   |

#### 重要な知見

- 100件の更新でトランザクションを使うと約90倍高速化
- データベース全体のサイズは更新性能にほとんど影響しない

### 削除操作

| データ件数 | 単一削除(100件) |
|-----------|----------------|
| 100件     | 1,248ms        |
| 1,000件   | 1,209ms        |

## パフォーマンス最適化のベストプラクティス

### 1. トランザクションの使用

複数の操作を行う場合は、必ずトランザクションでラップしてください。

```typescript
// 悪い例
for (const item of items) {
  db.createMedia(item);
}

// 良い例
db.bulkCreateMedia(items);

// または手動でトランザクションを使用
db.transaction(() => {
  for (const item of items) {
    db.createMedia(item);
  }
});
```

### 2. インデックスの活用

スキーマ（正本は `rust-sdk/schema.sql`、`ts-sdk/src/migration.ts` も同一内容）には以下のインデックスが定義されています：

**media テーブル**:
- PRIMARY KEY (id)
- UNIQUE (uuid)
- UNIQUE (path)
- idx_media_title_id (title_id)
- idx_media_artist_id (artist_id)
- idx_media_media_type (media_type)
- idx_media_series (series)
- idx_media_source (source)
- idx_media_type_created (media_type, created_at DESC)

**media_tags テーブル**:
- PRIMARY KEY (media_id, tag_id)
- idx_media_tags_media_id (media_id)
- idx_media_tags_tag_id (tag_id)

**media_attributes テーブル**:
- PRIMARY KEY (media_id, key)

**media_hashes テーブル**:
- PRIMARY KEY (item_uuid, filename, time_range)
- idx_media_hashes_content (content_hash)

**tags テーブル**:
- PRIMARY KEY (id)
- UNIQUE (name)

これらのカラムでの検索は高速です。なお `created_at` 単体のインデックスは存在せず、複合インデックス `(media_type, created_at DESC)` でカバーされます。

### 3. ページネーションの活用

大量のデータを扱う場合は、LIMIT/OFFSETを使用してページネーションを実装してください。

```typescript
const results = db.findMedia({}, { limit: 100, offset: 0 });
```

### 4. 適切なフィルタ条件の使用

複合条件検索でも高速ですが、可能な限り絞り込み条件を指定することで、さらなる高速化が期待できます。

## ベンチマークの実行方法

```bash
cd ts-sdk
pnpm run benchmark
```

## 結論

きじゅくDB TypeScript SDKは、SQLiteの特性を活かし、適切なトランザクション管理とインデックス設計により、以下の性能を実現しています：

- 大量データ挿入: トランザクション使用で最大1,800倍の速度改善
- 検索: 10,000件のデータでも全ての検索が2ms以下
- 更新/削除: トランザクション使用で90倍以上の速度改善

実際のアプリケーションでは、適切なトランザクション管理を行うことで、優れたパフォーマンスを発揮できます。
