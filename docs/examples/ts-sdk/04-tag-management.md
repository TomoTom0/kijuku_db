# Tag Management - タグ管理

メディアにタグを付けて分類・整理する方法を解説します。

## タグの基本操作

### タグの作成

```typescript
import { KijukuDB } from 'kijuku-db';

const db = new KijukuDB('./data/myapp.db');
db.migrate();

// タグを作成
const tag = db.createTag('お気に入り');
console.log(`作成されたタグ: ID=${tag.id}, Name="${tag.name}"`);
// => 作成されたタグ: ID=1, Name="お気に入り"
```

### タグの取得

```typescript
// 名前で取得
const tag = db.getTagByName('お気に入り');

if (tag) {
  console.log(`タグID: ${tag.id}`);
} else {
  console.log('タグが見つかりません');
}
```

### すべてのタグを取得

```typescript
const allTags = db.getAllTags();

console.log(`タグ数: ${allTags.length}`);
allTags.forEach(tag => {
  console.log(`- ${tag.name} (ID: ${tag.id})`);
});
```

## メディアへのタグ付け

### タグを追加

```typescript
// メディアを作成
const media = db.createMedia({
  title: 'ワンピース 1巻',
  media_type: 'comic',
});

// タグを作成または取得
const favoriteTag = db.createTag('お気に入り');
const readTag = db.createTag('既読');

// メディアにタグを付与
db.addTagToMedia(media.id, favoriteTag.id);
db.addTagToMedia(media.id, readTag.id);

console.log('タグを付与しました');
```

### 重複を気にせずタグ追加

```typescript
// 前提：メディアとタグが既に存在する
const media = db.createMedia({ title: 'サンプル作品', media_type: 'comic' });
const favoriteTag = db.createTag('お気に入り');

// 同じタグを複数回追加しても問題ない
db.addTagToMedia(media.id, favoriteTag.id);
db.addTagToMedia(media.id, favoriteTag.id); // 2回目は何もしない
```

### タグを削除

```typescript
// メディアからタグを削除
db.removeTagFromMedia(media.id, favoriteTag.id);
console.log('タグを削除しました');
```

### メディアのタグを取得

```typescript
const media = db.getMedia(1);
if (media) {
  const tags = db.getMediaTags(media.id);
  
  console.log(`"${media.title}" のタグ:`);
  tags.forEach(tag => {
    console.log(`- ${tag.name}`);
  });
}
```

## タグでの検索

### 特定のタグを持つメディアを検索

```typescript
const favoriteTag = db.getTagByName('お気に入り');

if (favoriteTag) {
  const favorites = db.findMedia({ tag_ids: [favoriteTag.id] });
  
  console.log(`お気に入り作品: ${favorites.length}件`);
  favorites.forEach(m => {
    console.log(`- ${m.title}`);
  });
}
```

### 複数タグでの検索（AND条件）

```typescript
const favoriteTag = db.getTagByName('お気に入り');
const readTag = db.getTagByName('既読');

if (favoriteTag && readTag) {
  // 両方のタグを持つメディアのみ
  const results = db.findMedia({
    tag_ids: [favoriteTag.id, readTag.id],
  });
  
  console.log(`お気に入り AND 既読: ${results.length}件`);
}
```

### タグと他の条件を組み合わせる

```typescript
const favoriteTag = db.getTagByName('お気に入り');

if (favoriteTag) {
  const comicFavorites = db.findMedia({
    media_type: 'comic',
    tag_ids: [favoriteTag.id],
  });
  
  console.log(`お気に入りのコミック: ${comicFavorites.length}件`);
}
```

## 実用的な例

### メディアにタグを一括追加

```typescript
function addTagToMultipleMedia(mediaIds: number[], tagName: string) {
  // タグを作成または取得
  let tag = db.getTagByName(tagName);
  if (!tag) {
    tag = db.createTag(tagName);
  }
  
  // トランザクションで一括追加
  db.transaction(() => {
    mediaIds.forEach(mediaId => {
      db.addTagToMedia(mediaId, tag.id);
    });
  });
  
  console.log(`${mediaIds.length}件のメディアに"${tagName}"タグを追加しました`);
}

// 使用例
const comicIds = db.findMedia({ media_type: 'comic' })
  .slice(0, 10)
  .map(m => m.id);

addTagToMultipleMedia(comicIds, '未読');
```

### 条件に合うメディアにタグを付ける

```typescript
// 「ワンピース」シリーズすべてに「完結済み」タグを付ける
const onePiece = db.findMedia({ series: 'ワンピース' });
const completedTag = db.createTag('完結済み');

db.transaction(() => {
  onePiece.forEach(media => {
    db.addTagToMedia(media.id, completedTag.id);
  });
});

console.log(`${onePiece.length}件に「完結済み」タグを付与しました`);
```

### タグの使用状況を調べる

**推奨: 組み込みAPIを使用**

```typescript
const stats = db.getTagUsageStats();

console.log('タグの使用状況:');
stats.forEach(({ tag_name, count }) => {
  console.log(`- ${tag_name}: ${count}件`);
});
```

`getTagUsageStats()`は効率的なSQLクエリでタグの使用数を集計し、使用数の降順で返します。

**参考: 手動での集計方法（非推奨）**

> **⚠️ パフォーマンス注意**: この実装はタグの数だけクエリを発行するため、タグ数が多い場合はパフォーマンスが低下します（N+1クエリ問題）。上記の`getTagUsageStats()`を使用してください。

<details>
<summary>非推奨の実装例を表示</summary>

```typescript
function getTagUsageStatsManual() {
  const allTags = db.getAllTags();

  const stats = allTags.map(tag => {
    const mediaWithTag = db.findMedia({ tag_ids: [tag.id] });
    return {
      tag: tag.name,
      count: mediaWithTag.length,
    };
  });

  // 使用数でソート
  stats.sort((a, b) => b.count - a.count);

  return stats;
}
```

</details>

### 未使用のタグを見つける

**推奨: 組み込みAPIを使用**

```typescript
const unused = db.findUnusedTags();

console.log(`未使用のタグ: ${unused.length}件`);
unused.forEach(tag => {
  console.log(`- ${tag.name}`);
});
```

`findUnusedTags()`は効率的なSQLクエリで未使用のタグを取得します。

**参考: 手動での検索方法（非推奨）**

> **⚠️ パフォーマンス注意**: この実装はタグの数だけクエリを発行するため、タグ数が多い場合はパフォーマンスが低下します（N+1クエリ問題）。上記の`findUnusedTags()`を使用してください。

<details>
<summary>非推奨の実装例を表示</summary>

```typescript
function findUnusedTagsManual() {
  const allTags = db.getAllTags();

  const unusedTags = allTags.filter(tag => {
    const media = db.findMedia({ tag_ids: [tag.id] });
    return media.length === 0;
  });

  return unusedTags;
}
```

</details>

### メディアのタグを置き換える

```typescript
function replaceMediaTags(mediaId: number, newTagNames: string[]) {
  db.transaction(() => {
    // 既存のタグをすべて削除
    const currentTags = db.getMediaTags(mediaId);
    currentTags.forEach(tag => {
      db.removeTagFromMedia(mediaId, tag.id);
    });

    // 新しいタグを追加
    newTagNames.forEach(tagName => {
      let tag = db.getTagByName(tagName);
      if (!tag) {
        tag = db.createTag(tagName);
      }
      db.addTagToMedia(mediaId, tag.id);
    });
  });
}

// 使用例
replaceMediaTags(1, ['お気に入り', '既読', '完結済み']);
```

## タグ管理のベストプラクティス

### 1. タグ名の正規化

```typescript
function normalizeTagName(name: string): string {
  return name.trim().toLowerCase();
}

function getOrCreateTag(name: string) {
  const normalized = normalizeTagName(name);
  
  let tag = db.getTagByName(normalized);
  if (!tag) {
    tag = db.createTag(normalized);
  }
  
  return tag;
}

// 使用例
const tag1 = getOrCreateTag('お気に入り');
const tag2 = getOrCreateTag(' お気に入り '); // 同じタグ
const tag3 = getOrCreateTag('お気に入り'); // 同じタグ
```

### 2. よく使うタグを事前作成

```typescript
function initializeCommonTags() {
  const commonTags = [
    'お気に入り',
    '未読',
    '既読',
    '完結済み',
    '連載中',
    '積読',
  ];
  
  db.transaction(() => {
    commonTags.forEach(tagName => {
      const existing = db.getTagByName(tagName);
      if (!existing) {
        db.createTag(tagName);
      }
    });
  });
  
  console.log('共通タグを初期化しました');
}

initializeCommonTags();
```

### 3. タグを使った整理

```typescript
// ライブラリ管理の例
const categories = {
  status: ['未読', '既読', '積読'],
  genre: ['少年漫画', '青年漫画', '少女漫画'],
  rating: ['★★★★★', '★★★★', '★★★'],
};

// カテゴリごとにタグを作成
db.transaction(() => {
  Object.values(categories).flat().forEach(tagName => {
    const existing = db.getTagByName(tagName);
    if (!existing) {
      db.createTag(tagName);
    }
  });
});
```

## 完全な例

```typescript
import { KijukuDB } from 'kijuku-db';

const db = new KijukuDB('./data/myapp.db');
db.migrate();

// メディアを作成
const media1 = db.createMedia({
  title: 'ワンピース 1巻',
  media_type: 'comic',
  series: 'ワンピース',
});

const media2 = db.createMedia({
  title: 'ワンピース 2巻',
  media_type: 'comic',
  series: 'ワンピース',
});

// タグを作成
const favoriteTag = db.createTag('お気に入り');
const readTag = db.createTag('既読');
const unreadTag = db.createTag('未読');

// タグを付与
db.addTagToMedia(media1.id, favoriteTag.id);
db.addTagToMedia(media1.id, readTag.id);

db.addTagToMedia(media2.id, favoriteTag.id);
db.addTagToMedia(media2.id, unreadTag.id);

// タグで検索
console.log('=== お気に入り作品 ===');
const favorites = db.findMedia({ tag_ids: [favoriteTag.id] });
favorites.forEach(m => {
  const tags = db.getMediaTags(m.id);
  const tagNames = tags.map(t => t.name).join(', ');
  console.log(`${m.title} [${tagNames}]`);
});

// 既読かつお気に入り
console.log('\n=== 既読のお気に入り ===');
const readFavorites = db.findMedia({
  tag_ids: [favoriteTag.id, readTag.id],
});
readFavorites.forEach(m => console.log(m.title));

db.close();
```

## 次のステップ

- [05-bulk-operations.md](./05-bulk-operations.md) - バルク操作
- [06-advanced-queries.md](./06-advanced-queries.md) - 高度なクエリ
