# Bulk Operations - バルク操作

大量のデータを効率的に処理する方法を解説します。

> **リモートDBを使う場合は必読**: `RemoteKijukuDB` では1操作ごとにリモート CLI プロセスが1回起動されます（SSH 接続自体は接続プールで再利用されるため接続数は増えません）。
> 個別操作のループは RPC 往復とリモートプロセス起動が繰り返され大幅に遅くなります。
> 必ずバルク操作（`bulkCreateMedia` / `bulkUpdateMedia` / `bulkDeleteMedia`）を使ってください。

## 複数メディアの一括作成

### bulkCreateMediaの使用

```typescript
import { KijukuDB } from 'kijuku-db';

const db = new KijukuDB('./data/myapp.db');
db.migrate();

// 複数のメディアを一括作成
const mediaList = [
  {
    title: 'ワンピース 1巻',
    media_type: 'comic' as const,
    artist: '尾田栄一郎',
    series: 'ワンピース',
  },
  {
    title: 'ワンピース 2巻',
    media_type: 'comic' as const,
    artist: '尾田栄一郎',
    series: 'ワンピース',
  },
  {
    title: 'ワンピース 3巻',
    media_type: 'comic' as const,
    artist: '尾田栄一郎',
    series: 'ワンピース',
  },
];

const created = db.bulkCreateMedia(mediaList);
console.log(`${created.length}件のメディアを作成しました`);

created.forEach(media => {
  console.log(`- ID:${media.id} ${media.title}`);
});
```

### パフォーマンス比較

```typescript
// ❌ 遅い: 1件ずつ作成
console.time('Individual Create');
for (let i = 0; i < 100; i++) {
  db.createMedia({
    title: `メディア ${i}`,
    media_type: 'comic',
  });
}
console.timeEnd('Individual Create');
// => Individual Create: ~250ms

// ✅ 速い: 一括作成
console.time('Bulk Create');
const bulkData = Array.from({ length: 100 }, (_, i) => ({
  title: `メディア ${i}`,
  media_type: 'comic' as const,
}));
db.bulkCreateMedia(bulkData);
console.timeEnd('Bulk Create');
// => Bulk Create: ~50ms（約5倍速い）
```

## トランザクションの使用

### 基本的なトランザクション

```typescript
// トランザクション内で複数操作を実行
db.transaction(() => {
  const media1 = db.createMedia({
    title: 'メディアA',
    media_type: 'comic',
  });
  
  const media2 = db.createMedia({
    title: 'メディアB',
    media_type: 'comic',
  });
  
  const tag = db.createTag('新着');
  db.addTagToMedia(media1.id, tag.id);
  db.addTagToMedia(media2.id, tag.id);
});

console.log('トランザクション完了');
```

### トランザクションのメリット

```typescript
try {
  db.transaction(() => {
    const media = db.createMedia({
      title: 'メディア1',
      media_type: 'comic',
    });
    
    // エラーが発生
    throw new Error('エラー発生！');
    
    // この操作は実行されない
    db.createMedia({
      title: 'メディア2',
      media_type: 'comic',
    });
  });
} catch (error) {
  if (error instanceof Error) {
    console.log('エラー:', error.message);
  }
  // トランザクション全体がロールバックされる
  // 「メディア1」も作成されていない
}
```

### 戻り値のあるトランザクション

```typescript
const result = db.transaction(() => {
  const media1 = db.createMedia({
    title: 'A',
    media_type: 'comic',
  });
  
  const media2 = db.createMedia({
    title: 'B',
    media_type: 'comic',
  });
  
  // 戻り値を返せる
  return { media1, media2 };
});

console.log(`作成したID: ${result.media1.id}, ${result.media2.id}`);
```

## 大量データの処理

### バッチ処理

```typescript
function processBatches<T>(
  items: T[],
  batchSize: number,
  processor: (batch: T[]) => void
) {
  for (let i = 0; i < items.length; i += batchSize) {
    const batch = items.slice(i, i + batchSize);
    processor(batch);
  }
}

// 使用例: 10000件のデータを100件ずつ処理
const data = Array.from({ length: 10000 }, (_, i) => ({
  title: `メディア ${i}`,
  media_type: 'comic' as const,
}));

processBatches(data, 100, (batch) => {
  db.bulkCreateMedia(batch);
  console.log(`${batch.length}件処理完了`);
});
```

### 進捗表示付き処理

```typescript
function bulkCreateWithProgress(data: any[]) {
  const total = data.length;
  const batchSize = 100;
  let processed = 0;
  
  for (let i = 0; i < total; i += batchSize) {
    const batch = data.slice(i, i + batchSize);
    db.bulkCreateMedia(batch);
    
    processed += batch.length;
    const percent = ((processed / total) * 100).toFixed(1);
    console.log(`進捗: ${processed}/${total} (${percent}%)`);
  }
}

const largeData = Array.from({ length: 5000 }, (_, i) => ({
  title: `メディア ${i}`,
  media_type: 'comic' as const,
}));

bulkCreateWithProgress(largeData);
```

## 一括更新

### 複数メディアの更新

```typescript
// シリーズ内のすべてのメディアに言語情報を追加
const onePiece = db.findMedia({ series: 'ワンピース' });

// ✅ bulkUpdateMediaを使う（ローカル・リモート共通で推奨）
db.bulkUpdateMedia(
  onePiece.map(m => ({
    id: m.id,
    data: { language: 'ja', source: 'bookwalker' },
  }))
);

console.log(`${onePiece.length}件を更新しました`);
```

### 条件付き一括更新

```typescript
// 古いパスを新しいパスに置き換える
const allMedia = db.findMedia({});

const updates = allMedia
  .filter(m => m.path?.startsWith('/old/path/'))
  .map(m => ({
    id: m.id,
    data: { path: m.path!.replace('/old/path/', '/new/path/') },
  }));

db.bulkUpdateMedia(updates);

console.log(`${updates.length}件のパスを更新しました`);
```

## 一括削除

### 条件に合うメディアを削除

```typescript
// テストデータを削除
const testMedia = db.findMedia({ source: 'test' });

// ✅ bulkDeleteMediaを使う（ローカル・リモート共通で推奨）
db.bulkDeleteMedia(testMedia.map(m => m.id));

console.log(`${testMedia.length}件のテストデータを削除しました`);
```

### 重複データの削除

```typescript
function removeDuplicates() {
  const allMedia = db.findMedia({});
  
  // title + artist でグループ化
  const groups = new Map<string, number[]>();
  
  allMedia.forEach(media => {
    const key = `${media.title}|${media.artist}`;
    if (!groups.has(key)) {
      groups.set(key, []);
    }
    groups.get(key)!.push(media.id);
  });
  
  // 重複を削除（最初の1件を残す）
  let deletedCount = 0;
  
  db.transaction(() => {
    groups.forEach((ids, key) => {
      if (ids.length > 1) {
        // 最初以外を削除
        ids.slice(1).forEach(id => {
          db.deleteMedia(id);
          deletedCount++;
        });
      }
    });
  });
  
  console.log(`${deletedCount}件の重複データを削除しました`);
}

removeDuplicates();
```

> **パフォーマンス注意**: この実装は全データをメモリにロードするため、大量のデータがある場合はメモリ消費が大きくなります。データ量が多い場合は、GROUP BY句とHAVING句を使ったSQLクエリで重複を特定し、バッチ処理で削除することを検討してください。

## JSONからのインポート

### 基本的なインポート

```typescript
import * as fs from 'fs';

// JSONファイルを読み込み
const jsonData = JSON.parse(fs.readFileSync('./data.json', 'utf8'));

// 一括作成
const created = db.bulkCreateMedia(jsonData);
console.log(`${created.length}件をインポートしました`);
```

### エラーハンドリング付きインポート

```typescript
function importFromJSON(filePath: string) {
  try {
    const data = JSON.parse(fs.readFileSync(filePath, 'utf8'));
    
    // バリデーション
    if (!Array.isArray(data)) {
      throw new Error('JSONはメディアの配列である必要があります');
    }
    
    // インポート
    const created = db.bulkCreateMedia(data);
    console.log(`✓ ${created.length}件をインポートしました`);
    
    return created;
  } catch (error) {
    console.error(`✗ インポート失敗: ${error.message}`);
    throw error;
  }
}

importFromJSON('./media.json');
```

## データのエクスポート

### すべてのメディアをエクスポート

```typescript
import * as fs from 'fs';

function exportToJSON(outputPath: string) {
  const allMedia = db.findMedia({});
  
  // JSONに変換
  const json = JSON.stringify(allMedia, null, 2);
  
  // ファイルに書き込み
  fs.writeFileSync(outputPath, json, 'utf8');
  
  console.log(`${allMedia.length}件を ${outputPath} にエクスポートしました`);
}

exportToJSON('./export.json');
```

### 条件付きエクスポート

```typescript
function exportComicsByArtist(artist: string, outputPath: string) {
  const media = db.findMedia({
    media_type: 'comic',
    artist: artist,
  });
  
  const json = JSON.stringify(media, null, 2);
  fs.writeFileSync(outputPath, json, 'utf8');
  
  console.log(`${artist}の作品 ${media.length}件をエクスポートしました`);
}

exportComicsByArtist('尾田栄一郎', './oda-works.json');
```

## 実用的な例

### CSVからのインポート

まず、csv-parseパッケージをインストールします:

```bash
npm install csv-parse
```

```typescript
import { parse } from 'csv-parse/sync';
import * as fs from 'fs';

function importFromCSV(filePath: string) {
  // CSVを読み込み
  const content = fs.readFileSync(filePath, 'utf8');
  const records = parse(content, {
    columns: true,
    skip_empty_lines: true,
  });
  
  // MediaInput形式に変換
  const mediaData = records.map((record: any) => ({
    title: record.title,
    media_type: record.media_type,
    artist: record.artist,
    series: record.series,
    volume_text: record.volume,
  }));
  
  // 一括作成
  const created = db.bulkCreateMedia(mediaData);
  console.log(`${created.length}件をインポートしました`);
  
  return created;
}

importFromCSV('./media.csv');
```

### データの移行

```typescript
function migrateFromOldDB(oldDbPath: string) {
  const oldDb = new KijukuDB(oldDbPath);
  const newDb = new KijukuDB('./data/new.db');
  newDb.migrate();
  
  // 古いDBからすべてのメディアを取得
  const allMedia = oldDb.findMedia({});
  
  console.log(`${allMedia.length}件を移行します...`);
  
  // 100件ずつ処理
  processBatches(allMedia, 100, (batch) => {
    // IDを除外して新しいDBに挿入
    const cleanBatch = batch.map(({ id, created_at, updated_at, ...rest }) => rest);
    newDb.bulkCreateMedia(cleanBatch);
  });
  
  console.log('移行完了');
  
  oldDb.close();
  newDb.close();
}
```

### バックアップとリストア

```typescript
function backupDatabase(dbPath: string, backupPath: string) {
  const db = new KijukuDB(dbPath);
  const allMedia = db.findMedia({});
  const allTags = db.getAllTags();
  
  // タグ情報も含めてバックアップ
  const mediaWithTags = allMedia.map(media => ({
    ...media,
    tags: db.getMediaTags(media.id).map(t => t.name),
  }));
  
  const backup = {
    version: 1,
    timestamp: new Date().toISOString(),
    media: mediaWithTags,
    tags: allTags,
  };
  
  fs.writeFileSync(backupPath, JSON.stringify(backup, null, 2), 'utf8');
  console.log(`バックアップ完了: ${backupPath}`);
  
  db.close();
}

function restoreDatabase(backupPath: string, dbPath: string) {
  const backup = JSON.parse(fs.readFileSync(backupPath, 'utf8'));
  
  const db = new KijukuDB(dbPath);
  db.migrate();
  
  db.transaction(() => {
    // メディアを復元
    const created = db.bulkCreateMedia(
      backup.media.map(({ tags, ...media }) => media)
    );
    
    // タグを復元
    backup.media.forEach((media: any, index: number) => {
      media.tags?.forEach((tagName: string) => {
        let tag = db.getTagByName(tagName);
        if (!tag) {
          tag = db.createTag(tagName);
        }
        db.addTagToMedia(created[index].id, tag.id);
      });
    });
  });
  
  console.log(`リストア完了: ${backup.media.length}件`);
  db.close();
}

// 使用例
backupDatabase('./data/myapp.db', './backup.json');
restoreDatabase('./backup.json', './data/restored.db');
```

## パフォーマンスのベストプラクティス

### 1. バルク操作を使う

```typescript
// ✅ 推奨
db.bulkCreateMedia(dataArray);

// ❌ 非推奨
dataArray.forEach(data => db.createMedia(data));
```

### 2. トランザクションでまとめる

```typescript
// ✅ 推奨（1回のトランザクション）
db.transaction(() => {
  for (let i = 0; i < 100; i++) {
    db.createMedia({ title: `${i}`, media_type: 'comic' });
  }
});

// ❌ 非推奨（100回のトランザクション）
for (let i = 0; i < 100; i++) {
  db.createMedia({ title: `${i}`, media_type: 'comic' });
}
```

### 3. 適切なバッチサイズ

```typescript
// 大量データの例
const largeDataset: MediaInput[] = Array.from({ length: 10000 }, (_, i) => ({
  title: `Title ${i}`,
  media_type: 'comic',
}));

// バッチ処理用のヘルパー関数
function processBatches<T>(
  data: T[],
  batchSize: number,
  callback: (batch: T[]) => void
) {
  for (let i = 0; i < data.length; i += batchSize) {
    const batch = data.slice(i, i + batchSize);
    callback(batch);
  }
}

// メモリとパフォーマンスのバランスが重要
const BATCH_SIZE = 100; // 100〜1000が推奨

processBatches(largeDataset, BATCH_SIZE, (batch) => {
  db.bulkCreateMedia(batch);
});
```

## 次のステップ

- [API仕様書](../../api.md) - 全メソッドの詳細仕様
