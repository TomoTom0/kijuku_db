# Basic CRUD Operations - CRUD操作

Create（作成）、Read（読取）、Update（更新）、Delete（削除）の基本操作を詳しく解説します。

## Create - メディアの作成

### 基本的な作成

```typescript
import { KijukuDB } from 'kijuku-db';

const db = new KijukuDB('./data/myapp.db');
db.migrate();

const media = db.createMedia({
  title: 'ワンピース 1巻',
  media_type: 'comic',
  artist: '尾田栄一郎',
  series: 'ワンピース',
});

console.log(`作成されました: ID=${media.id}`);
// => 作成されました: ID=1
```

### 詳細情報を含む作成

```typescript
const media = db.createMedia({
  // 必須フィールド
  title: 'ワンピース 1巻',
  media_type: 'comic',
  
  // 作者情報
  artist: '尾田栄一郎',
  artist_id: 'oda-eiichiro',
  
  // シリーズ情報
  series: 'ワンピース',
  volume_text: '1',          // 巻数（文字列）
  volume_title: '冒険の夜明け', // 巻のサブタイトル
  
  // コンテンツ情報
  page_count: 200,
  description: '海賊王を目指す少年の冒険物語',
  
  // ファイル情報
  path: '/path/to/onepiece-vol1.cbz',
  thumbnail_path: '/path/to/thumbnails/onepiece-vol1.jpg',
  file_size: 45678900, // バイト
  
  // メタデータ
  language: 'ja',
  source: 'bookwalker',
  external_id: 'bw-12345',
});
```

### volume_numberの自動計算

`volume_text`に整数を設定すると、自動的に`volume_number`が計算されます：

```typescript
const media1 = db.createMedia({
  title: 'ワンピース 1巻',
  media_type: 'comic',
  volume_text: '1',  // 文字列
});

const retrieved = db.getMedia(media1.id);
if (retrieved) {
  console.log(retrieved.volume_number); // => 1（数値に変換されている）
}

// ソートやフィルタで使える
const sorted = db.findMedia(
  { series: 'ワンピース' },
  { orderBy: 'volume_number', order: 'ASC' }
);
```

**注意:** `volume_number`フィールドを直接指定しても無視されます。常に`volume_text`から自動計算されます。

## Read - メディアの取得

### IDで取得

```typescript
const media = db.getMedia(1);

if (media) {
  console.log(media.title);
  console.log(media.artist);
} else {
  console.log('メディアが見つかりません');
}
```

### 検索で取得

```typescript
// すべてのコミックを取得
const comics = db.findMedia({ media_type: 'comic' });

// シリーズで取得
const onePiece = db.findMedia({ series: 'ワンピース' });

// 作者で取得
const odaWorks = db.findMedia({ artist: '尾田栄一郎' });

// 複数条件で取得
const results = db.findMedia({
  media_type: 'comic',
  series: 'ワンピース',
  artist: '尾田栄一郎',
});
```

### すべてのメディアを取得

```typescript
const allMedia = db.findMedia({});
console.log(`全メディア数: ${allMedia.length}`);
```

## Update - メディアの更新

### 部分的な更新

```typescript
// 説明文だけを更新
db.updateMedia(1, {
  description: '新しい説明文',
});

// 複数フィールドを更新
db.updateMedia(1, {
  page_count: 250,
  description: '更新された説明',
  thumbnail_path: '/new/path/to/thumbnail.jpg',
});
```

### 更新して結果を確認

```typescript
const mediaId = 1;

// 更新前
const before = db.getMedia(mediaId);
console.log('更新前:', before?.description);

// 更新
db.updateMedia(mediaId, {
  description: '新しい説明文',
});

// 更新後
const after = db.getMedia(mediaId);
console.log('更新後:', after?.description);
```

### 複数メディアの更新（トランザクション使用）

```typescript
db.transaction(() => {
  const media = db.findMedia({ series: 'ワンピース' });
  
  media.forEach(m => {
    db.updateMedia(m.id, {
      language: 'ja',
      source: 'updated-source',
    });
  });
});

console.log('一括更新完了');
```

## Delete - メディアの削除

### IDで削除

```typescript
db.deleteMedia(1);
console.log('削除しました');

// 削除を確認
const deleted = db.getMedia(1);
console.log(deleted === null); // => true
```

### 存在確認してから削除

```typescript
const mediaId = 1;
const media = db.getMedia(mediaId);

if (media) {
  db.deleteMedia(mediaId);
  console.log(`"${media.title}" を削除しました`);
} else {
  console.log('メディアが見つかりません');
}
```

### 条件に一致するメディアを削除

```typescript
// 一括削除APIを使用
const oldMedia = db.findMedia({ source: 'old-source' });
db.bulkDeleteMedia(oldMedia.map(m => m.id));
console.log(`${oldMedia.length}件のメディアを削除しました`);
```

## 完全な例

```typescript
import { KijukuDB } from 'kijuku-db';

const db = new KijukuDB('./data/myapp.db');
db.migrate();

// CREATE
console.log('=== Create ===');
const media = db.createMedia({
  title: 'サンプルコミック',
  media_type: 'comic',
  artist: 'サンプル作者',
  page_count: 200,
});
console.log(`作成: ID=${media.id}, Title="${media.title}"`);

// READ
console.log('\n=== Read ===');
const retrieved = db.getMedia(media.id);
if (retrieved) {
  console.log(`取得: ${retrieved.title} by ${retrieved.artist}`);
  console.log(`ページ数: ${retrieved.page_count}`);
}

// UPDATE
console.log('\n=== Update ===');
db.updateMedia(media.id, {
  page_count: 250,
  description: '更新された説明',
});
const updated = db.getMedia(media.id);
console.log(`更新後: ページ数=${updated?.page_count}`);
console.log(`説明: ${updated?.description}`);

// DELETE
console.log('\n=== Delete ===');
db.deleteMedia(media.id);
const deleted = db.getMedia(media.id);
console.log(`削除確認: ${deleted === null ? '削除成功' : '削除失敗'}`);

db.close();
```

**出力:**
```
=== Create ===
作成: ID=1, Title="サンプルコミック"

=== Read ===
取得: サンプルコミック by サンプル作者
ページ数: 200

=== Update ===
更新後: ページ数=250
説明: 更新された説明

=== Delete ===
削除確認: 削除成功
```

## ベストプラクティス

### 1. エラーハンドリング

```typescript
try {
  const media = db.createMedia({
    title: '作品名',
    media_type: 'comic',
  });
  console.log('作成成功');
} catch (error) {
  console.error('作成失敗:', error);
}
```

### 2. 存在確認

```typescript
const id = 1; // 取得したいメディアのID
const media = db.getMedia(id);
if (!media) {
  console.log('メディアが見つかりません');
  return;
}

// 存在する場合の処理
console.log(media.title);
```

### 3. トランザクションの活用

```typescript
// 複数の操作をアトミックに実行
db.transaction(() => {
  const media1 = db.createMedia({ title: 'A', media_type: 'comic' });
  const media2 = db.createMedia({ title: 'B', media_type: 'comic' });
  db.updateMedia(media1.id, { description: 'Updated' });
});
```

## 次のステップ

- [03-search-and-filter.md](./03-search-and-filter.md) - 検索とフィルタリング
- [04-tag-management.md](./04-tag-management.md) - タグ管理
- [05-bulk-operations.md](./05-bulk-operations.md) - バルク操作
