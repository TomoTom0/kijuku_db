# Search and Filter - 検索とフィルタリング

メディアを効率的に検索・フィルタリングする方法を解説します。

## 基本的な検索

### すべてのメディアを取得

```typescript
import { KijukuDB } from 'kijuku-db';

const db = new KijukuDB('./data/myapp.db');

// 空のフィルタですべて取得
const allMedia = db.findMedia({});
console.log(`全メディア数: ${allMedia.length}`);
```

### メディアタイプで絞り込み

```typescript
// コミックのみ
const comics = db.findMedia({ media_type: 'comic' });
console.log(`コミック: ${comics.length}件`);

// ビデオのみ
const videos = db.findMedia({ media_type: 'video' });
console.log(`ビデオ: ${videos.length}件`);

// 音楽のみ
const music = db.findMedia({ media_type: 'music' });
console.log(`音楽: ${music.length}件`);
```

## フィールドでの検索

### タイトルで検索

```typescript
// 部分一致検索（LIKE検索）
const results = db.findMedia({ title: 'ワンピース' });

results.forEach(m => {
  console.log(m.title);
});
// => ワンピース 1巻
// => ワンピース 2巻
// => ...
```

### 作者で検索

```typescript
const odaWorks = db.findMedia({ artist: '尾田栄一郎' });
console.log(`尾田栄一郎の作品: ${odaWorks.length}件`);

odaWorks.forEach(m => {
  console.log(`- ${m.title}`);
});
```

### シリーズで検索

```typescript
const onePiece = db.findMedia({ series: 'ワンピース' });

console.log(`シリーズ "ワンピース": ${onePiece.length}件`);
```

### データソースで検索

```typescript
const bookwalkerMedia = db.findMedia({ source: 'bookwalker' });
console.log(`BookWalkerからの作品: ${bookwalkerMedia.length}件`);
```

## 複合条件での検索

### 複数条件を組み合わせる

```typescript
// コミック かつ シリーズ「ワンピース」
const results = db.findMedia({
  media_type: 'comic',
  series: 'ワンピース',
});

// コミック かつ 作者「尾田栄一郎」 かつ シリーズ「ワンピース」
const specific = db.findMedia({
  media_type: 'comic',
  artist: '尾田栄一郎',
  series: 'ワンピース',
});
```

### IDでの検索

```typescript
// title_idで検索（完全一致）
const byTitleId = db.findMedia({ title_id: 'one-piece-vol1' });

// artist_idで検索（完全一致）
const byArtistId = db.findMedia({ artist_id: 'oda-eiichiro' });
```

## ソート

### 作成日時でソート

```typescript
// 新しい順
const newest = db.findMedia(
  { media_type: 'comic' },
  { orderBy: 'created_at', order: 'DESC' }
);

console.log('最新のコミック:');
newest.slice(0, 5).forEach(m => {
  console.log(`- ${m.title} (${m.created_at})`);
});

// 古い順
const oldest = db.findMedia(
  { media_type: 'comic' },
  { orderBy: 'created_at', order: 'ASC' }
);
```

### タイトルでソート

```typescript
// タイトル昇順（あいうえお順）
const sortedByTitle = db.findMedia(
  { media_type: 'comic' },
  { orderBy: 'title', order: 'ASC' }
);

sortedByTitle.forEach(m => {
  console.log(m.title);
});
```

### 作者でソート

```typescript
const sortedByArtist = db.findMedia(
  {},
  { orderBy: 'artist', order: 'ASC' }
);

sortedByArtist.forEach(m => {
  console.log(`${m.artist} - ${m.title}`);
});
```

### 巻数でソート

```typescript
// volume_numberでソート（数値として正しくソートされる）
const volumes = db.findMedia(
  { series: 'ワンピース' },
  { orderBy: 'volume_number', order: 'ASC' }
);

volumes.forEach(m => {
  console.log(`${m.volume_text}巻 - ${m.title}`);
});
// => 1巻 - ワンピース 1巻
// => 2巻 - ワンピース 2巻
// => ...
// => 10巻 - ワンピース 10巻  （文字列ソートだと"2巻"の後に来てしまう）
```

## ページネーション

### limit - 件数制限

```typescript
// 最初の10件を取得
const first10 = db.findMedia(
  { media_type: 'comic' },
  { limit: 10 }
);

console.log(`取得件数: ${first10.length}`); // => 10
```

### offset - スキップ

```typescript
// 最初の10件をスキップして次の10件を取得
const next10 = db.findMedia(
  { media_type: 'comic' },
  { limit: 10, offset: 10 }
);
```

### ページング実装

```typescript
function getPage(pageNumber: number, pageSize: number) {
  const offset = (pageNumber - 1) * pageSize;
  
  return db.findMedia(
    { media_type: 'comic' },
    {
      limit: pageSize,
      offset: offset,
      orderBy: 'created_at',
      order: 'DESC',
    }
  );
}

// 1ページ目（1〜20件）
const page1 = getPage(1, 20);

// 2ページ目（21〜40件）
const page2 = getPage(2, 20);

// 3ページ目（41〜60件）
const page3 = getPage(3, 20);
```

### 全件数の取得

```typescript
const allComics = db.findMedia({ media_type: 'comic' });
const totalCount = allComics.length;

const pageSize = 20;
const totalPages = Math.ceil(totalCount / pageSize);

console.log(`全${totalCount}件（全${totalPages}ページ）`);
```

## タグでの検索

### 特定のタグを持つメディアを検索

```typescript
// タグを作成
const favoriteTag = db.createTag('お気に入り');

// メディアにタグを付与
const media1 = db.createMedia({ title: 'A', media_type: 'comic' });
const media2 = db.createMedia({ title: 'B', media_type: 'comic' });
db.addTagToMedia(media1.id, favoriteTag.id);
db.addTagToMedia(media2.id, favoriteTag.id);

// タグで検索
const favorites = db.findMedia({ tag_ids: [favoriteTag.id] });
console.log(`お気に入り: ${favorites.length}件`);
```

### 複数タグでの検索（AND条件）

```typescript
const tag1 = db.createTag('完結');
const tag2 = db.createTag('お気に入り');

// 両方のタグを持つメディアを検索
const results = db.findMedia({ tag_ids: [tag1.id, tag2.id] });
console.log(`完結 AND お気に入り: ${results.length}件`);
```

## 複雑な検索の例

### 最新の人気作品を取得

```typescript
const favoriteTag = db.getTagByName('お気に入り');

if (favoriteTag) {
  const recentFavorites = db.findMedia(
    {
      media_type: 'comic',
      tag_ids: [favoriteTag.id],
    },
    {
      orderBy: 'created_at',
      order: 'DESC',
      limit: 10,
    }
  );
  
  console.log('最近追加されたお気に入り作品:');
  recentFavorites.forEach(m => {
    console.log(`- ${m.title} (${m.created_at})`);
  });
}
```

### シリーズ一覧を取得

```typescript
const allMedia = db.findMedia({ media_type: 'comic' });

// ユニークなシリーズ名を抽出
const seriesSet = new Set(
  allMedia
    .map(m => m.series)
    .filter(s => s) // nullやundefinedを除外
);

const seriesList = Array.from(seriesSet).sort();

console.log('シリーズ一覧:');
seriesList.forEach(series => {
  const count = allMedia.filter(m => m.series === series).length;
  console.log(`- ${series} (${count}件)`);
});
```

### 作者別の作品数

```typescript
const allMedia = db.findMedia({ media_type: 'comic' });

const artistCount: Record<string, number> = {};

allMedia.forEach(m => {
  if (m.artist) {
    artistCount[m.artist] = (artistCount[m.artist] || 0) + 1;
  }
});

// 作品数でソート
const sorted = Object.entries(artistCount)
  .sort((a, b) => b[1] - a[1]);

console.log('作者別作品数（上位10名）:');
sorted.slice(0, 10).forEach(([artist, count]) => {
  console.log(`- ${artist}: ${count}件`);
});
```

## パフォーマンスのヒント

### 1. インデックスが効く検索を優先

```typescript
// ✅ 速い（インデックスあり）
db.findMedia({ media_type: 'comic' });
db.findMedia({ title_id: 'one-piece-vol1' });
db.findMedia({ artist_id: 'oda-eiichiro' });
db.findMedia({ source: 'bookwalker' });

// ⚠️ 遅い可能性（全件スキャン）
db.findMedia({ description: 'keyword' }); // descriptionにはインデックスなし
```

### 2. 必要な件数だけ取得

```typescript
// ✅ 効率的
const top10 = db.findMedia(
  { media_type: 'comic' },
  { limit: 10 }
);

// ❌ 非効率（全件取得してから10件に絞る）
const all = db.findMedia({ media_type: 'comic' });
const top10Bad = all.slice(0, 10);
```

### 3. フィルタを組み合わせる

```typescript
// ✅ データベース側でフィルタ
const results = db.findMedia({
  media_type: 'comic',
  series: 'ワンピース',
  artist: '尾田栄一郎',
});

// ❌ アプリケーション側でフィルタ（非効率）
const all = db.findMedia({});
const filtered = all.filter(m => 
  m.media_type === 'comic' &&
  m.series === 'ワンピース' &&
  m.artist === '尾田栄一郎'
);
```

## 次のステップ

- [04-tag-management.md](./04-tag-management.md) - タグ管理
- [05-bulk-operations.md](./05-bulk-operations.md) - バルク操作
- [06-advanced-queries.md](./06-advanced-queries.md) - 高度なクエリ
