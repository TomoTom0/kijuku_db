# kijuku-db API仕様書

このドキュメントでは、kijuku-dbのTypeScript SDKおよびRust SDKの詳細なAPI仕様を説明します。
TypeScript SDKを基準に記載し、Rust SDKの相違点は[Rust SDK](#rust-sdk)セクションにまとめています。

## 目次

- [KijukuDBクラス](#kijukudbクラス)
  - [コンストラクタ](#コンストラクタ)
  - [マイグレーション](#マイグレーション)
  - [メディア操作](#メディア操作)
  - [タグ操作](#タグ操作)
  - [属性操作](#属性操作)
  - [トランザクション](#トランザクション)
  - [バックアップ操作](#バックアップ操作)
  - [バックアップ読み取り操作](#バックアップ読み取り操作)
  - [ファイル存在チェック](#ファイル存在チェック)
  - [サムネイル操作](#サムネイル操作)
  - [その他](#その他)
- [RemoteKijukuDB](#remotekijukud)
  - [コンストラクタ](#remotekijukudコンストラクタ)
  - [対応メソッド一覧](#remotekijukud対応メソッド一覧)
  - [バックアップ操作](#remotekijukudバックアップ操作)
- [Rust SDK](#rust-sdk)
- [Web GUIサーバー](#web-guiサーバー)
- [型定義](#型定義)
- [エラーハンドリング](#エラーハンドリング)

---

## KijukuDBクラス

### コンストラクタ

#### `constructor(dbPath: string, options?: DBOptions)`

データベース接続を初期化します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `dbPath` | `string` | ✓ | データベースファイルのパス。`:memory:`を指定するとインメモリDBになります |
| `options` | `DBOptions` | | 接続オプション |

**DBOptions:**

| プロパティ | 型 | デフォルト | 説明 |
|-----------|-----|-----------|------|
| `timeout` | `number` | `5000` | SQLite ビジータイムアウト（ミリ秒）。他プロセスが DB をロック中のとき、この時間待機してから `SQLITE_BUSY` エラーを返す |
| `readonly` | `boolean` | `false` | 読み取り専用モードで開く |
| `verbose` | `boolean` | `false` | SQLログを標準出力に表示 |
| `backup` | `BackupOptions` | `undefined` | 自動バックアップ設定 |

**戻り値:** `KijukuDB`インスタンス

**自動設定:**
- `PRAGMA foreign_keys = ON` - 外部キー制約を有効化
- `PRAGMA journal_mode = WAL` - WALモードで安全性とパフォーマンスを向上

**使用例:**

```typescript
// 基本的な使用
const db = new KijukuDB('./data/kijuku.db');

// インメモリDB
const memDb = new KijukuDB(':memory:');

// オプション指定
const db = new KijukuDB('./data/kijuku.db', {
  timeout: 10000,
  verbose: true,
});

// 読み取り専用モード
const readonlyDb = new KijukuDB('./data/kijuku.db', {
  readonly: true,
});
```

**エラー:**
- ファイルパスが不正な場合: `Error: SQLITE_CANTOPEN`
- 読み取り専用モードで書き込み操作を実行した場合: `Error: SQLITE_READONLY`

---

### マイグレーション

#### `migrate(): void`

データベーススキーマを初期化・更新します。

**パラメータ:** なし

**戻り値:** なし

**動作:**
1. `schema_version`テーブルが存在しない場合、全テーブルを作成
2. 現在のバージョンを確認
3. 必要に応じてマイグレーションを実行

**使用例:**

```typescript
const db = new KijukuDB('./data/kijuku.db');
db.migrate(); // スキーマを初期化
```

**エラー:**
- スキーマ作成に失敗した場合: `Error`（SQLエラーメッセージ）

---

#### `getSchemaVersion(): number`

現在のデータベーススキーマバージョンを取得します。

**パラメータ:** なし

**戻り値:** `number` - スキーマバージョン（現在は`4`）

**使用例:**

```typescript
const version = db.getSchemaVersion();
console.log(`Schema version: ${version}`);
```

---

### メディア操作

#### `createMedia(data: MediaInput): Media`

新しいメディアを作成します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `data` | `MediaInput` | ✓ | メディア情報 |

**MediaInput:**

| プロパティ | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `title` | `string` | ✓ | タイトル |
| `media_type` | `'comic' \| 'video' \| 'music'` | ✓ | メディアタイプ |
| `uuid` | `string` | | UUID（省略時は自動生成） |
| `title_id` | `string` | | タイトルID（外部システム連携用） |
| `path` | `string` | | ファイルパス（UNIQUE制約） |
| `thumbnail_path` | `string` | | サムネイル画像のパス |
| `artist` | `string` | | 作者・アーティスト名 |
| `artist_id` | `string` | | 作者ID |
| `description` | `string` | | 説明文 |
| `file_size` | `number` | | ファイルサイズ（バイト） |
| `duration_sec` | `number` | | 再生時間（秒）※video/music |
| `page_count` | `number` | | ページ数 ※comic |
| `series` | `string` | | シリーズ名 |
| `volume_number` | `number` | | 巻数（非推奨: 手動設定は無視されます。`volume_text`を使用してください） |
| `volume_text` | `string` | | 巻数テキスト表記（整数を設定すると`volume_number`が自動計算されます） |
| `volume_title` | `string` | | 巻タイトル |
| `magazine` | `string` | | 雑誌名 |
| `magazine_id` | `string` | | 雑誌ID |
| `language` | `string` | | 言語コード（例: `ja`, `en`） |
| `source` | `string` | | データソース（例: `manual`, `api`） |
| `external_id` | `string` | | 外部システムのID |
| `artist_en` | `string` | | 作者名（英語） |
| `title_en` | `string` | | タイトル（英語） |
| `chapters` | `string` | | チャプター情報 |
| `extension` | `string` | | ファイル拡張子 |
| `flag_exist` | `boolean` | | ファイルの存在フラグ |
| `title_pron` | `string` | | タイトル読み仮名 |
| `artist_pron` | `string` | | 作者読み仮名 |
| `series_pron` | `string` | | シリーズ読み仮名 |

**戻り値:** `Media` - 作成されたメディア情報（`id`, `created_at`, `updated_at`が自動設定される）

**使用例:**

```typescript
// 最小限の情報で作成
const media = db.createMedia({
  title: 'サンプルコミック',
  media_type: 'comic',
});

// 詳細情報を含めて作成
const media = db.createMedia({
  title: 'ワンピース 1巻',
  media_type: 'comic',
  artist: '尾田栄一郎',
  series: 'ワンピース',
  volume_number: 1,
  page_count: 200,
  language: 'ja',
  source: 'manual',
  path: '/media/comics/onepiece-01.cbz',
  flag_exist: true,
});

console.log(`Created media ID: ${media.id}`);
```

**エラー:**
- `title`または`media_type`が未指定: `Error: NOT NULL constraint failed`
- `path`が重複: `Error: UNIQUE constraint failed: media.path`
- 不正な`media_type`: `Error: CHECK constraint failed: media_type`

---

#### `getMedia(id: number): Media | null`

IDでメディアを取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `id` | `number` | ✓ | メディアID |

**戻り値:** `Media | null` - メディア情報。存在しない場合は`null`

**使用例:**

```typescript
const media = db.getMedia(1);
if (media) {
  console.log(`Title: ${media.title}`);
  console.log(`Type: ${media.media_type}`);
} else {
  console.log('Media not found');
}
```

---

#### `updateMedia(id: number, data: Partial<MediaInput>): void`

メディア情報を更新します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `id` | `number` | ✓ | 更新対象のメディアID |
| `data` | `Partial<MediaInput>` | ✓ | 更新する項目（一部のみ指定可能） |

**戻り値:** なし

**動作:**
- 指定されたフィールドのみを更新
- `updated_at`は自動更新（トリガー）
- 存在しないIDを指定しても エラーにはならない（SQLiteの仕様）

**使用例:**

```typescript
// タイトルのみ更新
db.updateMedia(1, {
  title: '新しいタイトル',
});

// 複数フィールドを更新
db.updateMedia(1, {
  description: '更新された説明文',
  page_count: 250,
  flag_exist: true,
});
```

**エラー:**
- `path`が重複: `Error: UNIQUE constraint failed: media.path`
- 不正な`media_type`: `Error: CHECK constraint failed: media_type`

---

#### `deleteMedia(id: number): void`

メディアを削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `id` | `number` | ✓ | 削除対象のメディアID |

**戻り値:** なし

**動作:**
- `media_tags`の関連レコードも自動削除（外部キー制約）
- `media_attributes`の関連レコードも自動削除（外部キー制約）
- 存在しないIDを指定してもエラーにはならない

**使用例:**

```typescript
db.deleteMedia(1);

// 削除確認
const media = db.getMedia(1);
console.log(media === null); // true
```

---

#### `findMedia(filter: MediaFilter, options?: QueryOptions): Media[]`

条件に合うメディアを検索します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `filter` | `MediaFilter` | ✓ | 検索条件 |
| `options` | `QueryOptions` | | ソート・ページネーション設定 |

**MediaFilter:**

| プロパティ | 型 | 説明 |
|-----------|-----|------|
| `title` | `string` | タイトル部分一致検索（LIKE `%title%`） |
| `title_id` | `string` | タイトルID完全一致 |
| `artist` | `string` | 作者部分一致検索 |
| `artist_id` | `string` | 作者ID完全一致 |
| `media_type` | `'comic' \| 'video' \| 'music'` | メディアタイプ完全一致 |
| `series` | `string` | シリーズ部分一致検索 |
| `source` | `string` | データソース完全一致 |
| `tag_ids` | `number[]` | タグIDの配列（指定されたタグを持つメディア） |
| `flag_exist` | `boolean` | ファイル存在フラグで絞り込み |
| `language` | `string` | 言語コード完全一致 |
| `magazine` | `string` | 雑誌名部分一致検索 |
| `magazine_id` | `string` | 雑誌ID完全一致 |
| `extension` | `string` | 拡張子完全一致 |
| `external_id` | `string` | 外部ID完全一致 |
| `volume_title` | `string` | 巻タイトル部分一致検索 |
| `title_en` | `string` | タイトル（英語）部分一致検索 |
| `artist_en` | `string` | 作者名（英語）部分一致検索 |
| `id_in` | `number[]` | IDのIN句フィルタ（複数IDを一括フェッチする場合に使用）。999件超の場合は自動的にチャンク分割して処理 |
| `or_filters` | `MediaFilter[]` | OR条件で結合する追加フィルタ（ネスト可能） |

**QueryOptions:**

| プロパティ | 型 | デフォルト | 説明 |
|-----------|-----|-----------|------|
| `sortKeys` | `SortKey[]` | `[]` | ソートキーの配列（複数指定で多段ソート） |
| `limit` | `number` | なし | 最大取得件数 |
| `offset` | `number` | `0` | スキップする件数 |

**SortKey:**

| プロパティ | 型 | デフォルト | 説明 |
|-----------|-----|-----------|------|
| `field` | `string` | | ソート対象カラム名 |
| `order` | `'ASC' \| 'DESC'` | `'ASC'` | ソート順 |

**戻り値:** `Media[]` - 検索結果の配列（0件の場合は空配列）

**使用例:**

```typescript
// 全件取得
const allMedia = db.findMedia({});

// タイトルで検索
const results = db.findMedia({ title: 'ワンピース' });

// メディアタイプでフィルタ
const comics = db.findMedia({ media_type: 'comic' });

// 複数条件を組み合わせ
const results = db.findMedia({
  media_type: 'comic',
  artist: '尾田',
  series: 'ワンピース',
});

// ソート指定（単一フィールド）
const sortedMedia = db.findMedia(
  { media_type: 'comic' },
  { sortKeys: [{ field: 'title', order: 'ASC' }] }
);

// 多段ソート（作者昇順 → タイトル昇順）
const multiSorted = db.findMedia(
  { media_type: 'comic' },
  { sortKeys: [{ field: 'artist', order: 'ASC' }, { field: 'title', order: 'ASC' }] }
);

// ページネーション
const page1 = db.findMedia({}, { limit: 10, offset: 0 });
const page2 = db.findMedia({}, { limit: 10, offset: 10 });

// タグで検索
const favorites = db.findMedia({ tag_ids: [1, 2] }); // タグID 1 or 2を持つメディア

// OR条件で検索
const results = db.findMedia({
  or_filters: [
    { artist: 'Author1' },
    { artist: 'Author2' }
  ]
});

// 複雑なOR条件（ネスト可能）
const complex = db.findMedia({
  or_filters: [
    { artist: 'A', series: 'X' },
    { artist: 'B' }
  ]
});
```

**注意:**
- `tag_ids`を指定した場合、いずれかのタグを持つメディアが返されます（OR条件）
- 複数の検索条件は AND 条件で結合されます
- `or_filters`を使用するとOR条件で検索できます
  - 各フィルタ内の条件はAND結合
  - `or_filters`間はOR結合
  - ネスト可能（`or_filters`の中にさらに`or_filters`）
- 部分一致検索は大文字小文字を区別します

---

#### `getDistinctValues(fields: string[], filter: MediaFilter): (string | null)[][]`

指定したフィールド群の重複なしの値の組み合わせ一覧を取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `fields` | `string[]` | ✓ | 対象フィールド名の配列（ホワイトリストで検証） |
| `filter` | `MediaFilter` | ✓ | 絞り込み条件（条件なしの場合は `{}` を渡す） |

**使用可能なフィールド（`ALLOWED_DISTINCT_FIELDS`）:**

`title`, `title_id`, `artist`, `artist_id`, `media_type`, `series`, `volume_text`, `volume_title`, `magazine`, `magazine_id`, `language`, `source`, `external_id`, `artist_en`, `title_en`, `chapters`, `extension`, `title_pron`, `artist_pron`, `series_pron`

**戻り値:** `(string | null)[][]` - 各要素は `fields` と同じ順序のフィールド値。NULLの組み合わせも含まれる。昇順ソート済み。

**使用例:**

```typescript
import { ALLOWED_DISTINCT_FIELDS } from 'kijuku-db';

// 全artistの一覧（重複なし）
const artists = db.getDistinctValues(['artist'], {});
// => [['Author1'], ['Author2'], [null]]  ← NULLも含まれる

// コミックのシリーズ一覧（フィルタあり）
const series = db.getDistinctValues(['series'], { media_type: 'comic' });

// artist × series の組み合わせ一覧
const combinations = db.getDistinctValues(['artist', 'series'], {});
// => [['Author1', 'Series1'], ['Author1', 'Series2'], ['Author2', 'Series1'], ...]
```

**注意:**
- `fields` が空配列の場合はエラー
- ホワイトリスト外のフィールドを指定するとエラー
- NULLを除きたい場合はアプリケーション側でフィルタリングする

---

#### `bulkCreateMedia(dataList: MediaInput[]): Media[]`

複数のメディアを一括作成します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `dataList` | `MediaInput[]` | ✓ | メディア情報の配列 |

**戻り値:** `Media[]` - 作成されたメディア情報の配列

**動作:**
- トランザクション内で全ての作成処理を実行
- 1件でもエラーがあれば全てロールバック

**使用例:**

```typescript
const mediaList = [
  { title: 'コミック1', media_type: 'comic' },
  { title: 'コミック2', media_type: 'comic' },
  { title: 'ビデオ1', media_type: 'video' },
];

const created = db.bulkCreateMedia(mediaList);
console.log(`Created ${created.length} media`);
```

**エラー:**
- いずれかのデータが不正な場合、全てロールバック
- `createMedia()`と同じエラーが発生する可能性あり

---

#### `bulkDeleteMedia(ids: number[]): void`

複数のメディアを一括削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `ids` | `number[]` | ✓ | 削除対象のメディアIDの配列 |

**戻り値:** なし

**動作:**
- トランザクション内で全ての削除処理を実行
- 1件でもエラーがあれば全てロールバック
- 関連するタグ・属性も自動削除（外部キー制約）

**使用例:**

```typescript
// 複数のメディアを一括削除
db.bulkDeleteMedia([1, 2, 3]);

// 検索結果を一括削除
const oldMedia = db.findMedia({ source: 'deprecated' });
db.bulkDeleteMedia(oldMedia.map(m => m.id));
```

**エラー:**
- 存在しないIDが含まれている場合、エラーが発生してロールバック

---

#### `bulkUpdateMedia(updates: BulkUpdateItem[]): void`

複数のメディアを一括更新します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `updates` | `BulkUpdateItem[]` | ✓ | 更新情報の配列 |

**BulkUpdateItem:**

| プロパティ | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `id` | `number` | ✓ | 更新対象のメディアID |
| `data` | `Partial<MediaInput>` | ✓ | 更新する項目 |

**戻り値:** なし

**動作:**
- トランザクション内で全ての更新処理を実行
- 1件でもエラーがあれば全てロールバック

**使用例:**

```typescript
// 複数のメディアを一括更新
db.bulkUpdateMedia([
  { id: 1, data: { artist: '新しい作者名' } },
  { id: 2, data: { series: '新しいシリーズ名', volume_text: '1' } },
  { id: 3, data: { flag_exist: false } },
]);

// 検索結果を一括更新
const comics = db.findMedia({ media_type: 'comic' });
db.bulkUpdateMedia(
  comics.map(m => ({
    id: m.id,
    data: { source: 'migrated' }
  }))
);
```

**エラー:**
- 存在しないIDが含まれている場合、エラーが発生してロールバック
- `updateMedia()`と同じエラーが発生する可能性あり

---

### タグ操作

#### `createTag(name: string): Tag`

新しいタグを作成します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `name` | `string` | ✓ | タグ名 |

**戻り値:** `Tag` - 作成されたタグ情報

**使用例:**

```typescript
const tag = db.createTag('お気に入り');
console.log(`Tag ID: ${tag.id}`);
```

**エラー:**
- タグ名が重複: `Error: UNIQUE constraint failed: tags.name`

---

#### `getTagByName(name: string): Tag | null`

タグ名でタグを取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `name` | `string` | ✓ | タグ名 |

**戻り値:** `Tag | null` - タグ情報。存在しない場合は`null`

**使用例:**

```typescript
const tag = db.getTagByName('お気に入り');
if (tag) {
  console.log(`Tag ID: ${tag.id}`);
}
```

---

#### `getAllTags(): Tag[]`

全てのタグを取得します。

**パラメータ:** なし

**戻り値:** `Tag[]` - タグの配列

**使用例:**

```typescript
const tags = db.getAllTags();
tags.forEach((tag) => {
  console.log(`${tag.name} (ID: ${tag.id})`);
});
```

---

#### `addTagToMedia(mediaId: number, tagId: number): void`

メディアにタグを追加します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | ✓ | メディアID |
| `tagId` | `number` | ✓ | タグID |

**戻り値:** なし

**使用例:**

```typescript
const media = db.createMedia({ title: 'テスト', media_type: 'comic' });
const tag = db.createTag('新着');

db.addTagToMedia(media.id, tag.id);
```

**注意:**
- 同じ組み合わせが既に存在する場合: エラーをスローせず、何もしない（冪等）
- 存在しない`mediaId`または`tagId`: `Error: Media X or Tag Y not found`

---

#### `removeTagFromMedia(mediaId: number, tagId: number): void`

メディアからタグを削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | ✓ | メディアID |
| `tagId` | `number` | ✓ | タグID |

**戻り値:** なし

**使用例:**

```typescript
db.removeTagFromMedia(1, 2);
```

---

#### `getMediaTags(mediaId: number): Tag[]`

メディアに関連付けられたタグを取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | ✓ | メディアID |

**戻り値:** `Tag[]` - タグの配列

**使用例:**

```typescript
const tags = db.getMediaTags(1);
console.log(`Media has ${tags.length} tags`);
tags.forEach((tag) => console.log(`- ${tag.name}`));
```

---

#### `getTagUsageStats(): TagUsageStats[]`

全タグの使用状況（各タグが何件のメディアに紐付いているか）を取得します。

**パラメータ:** なし

**戻り値:** `TagUsageStats[]` - タグ使用状況の配列

**TagUsageStats:**

| プロパティ | 型 | 説明 |
|-----------|-----|------|
| `tag_id` | `number` | タグID |
| `tag_name` | `string` | タグ名 |
| `count` | `number` | 紐付けられているメディア件数 |

**使用例:**

```typescript
const stats = db.getTagUsageStats();
stats.forEach((s) => {
  console.log(`${s.tag_name}: ${s.count}件`);
});
```

---

#### `findUnusedTags(): Tag[]`

どのメディアにも紐付けられていないタグを検索します。

**パラメータ:** なし

**戻り値:** `Tag[]` - 未使用タグの配列

**使用例:**

```typescript
const unused = db.findUnusedTags();
console.log(`未使用タグ: ${unused.length}件`);
unused.forEach((tag) => console.log(`- ${tag.name}`));
```

---

### 属性操作

メディアの拡張属性（EAVモデル）を管理します。スキーマに定義されていない任意のキー・バリューをメディアに付与できます。

#### `setMediaAttribute(mediaId: number, key: string, value: string | null, valueType?: string): void`

メディアに属性を設定します。同じキーが存在する場合は上書きされます。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | ✓ | メディアID |
| `key` | `string` | ✓ | 属性キー |
| `value` | `string \| null` | ✓ | 属性値（`null`で値なし） |
| `valueType` | `'string' \| 'integer' \| 'boolean'` | | 値の型（デフォルト: `'string'`） |

**使用例:**

```typescript
db.setMediaAttribute(1, 'rating', '5');
db.setMediaAttribute(1, 'is_read', 'true', 'boolean');
db.setMediaAttribute(1, 'page_count_checked', '200', 'integer');
```

---

#### `getMediaAttribute(mediaId: number, key: string): MediaAttribute | null`

メディアの特定の属性を取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | ✓ | メディアID |
| `key` | `string` | ✓ | 属性キー |

**戻り値:** `MediaAttribute | null` - 属性情報。存在しない場合は`null`

**使用例:**

```typescript
const attr = db.getMediaAttribute(1, 'rating');
if (attr) {
  console.log(`${attr.key} = ${attr.value} (${attr.value_type})`);
}
```

---

#### `getMediaAttributes(mediaId: number): MediaAttribute[]`

メディアの全属性を取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | ✓ | メディアID |

**戻り値:** `MediaAttribute[]` - 属性の配列

**使用例:**

```typescript
const attrs = db.getMediaAttributes(1);
attrs.forEach((attr) => {
  console.log(`${attr.key}: ${attr.value} [${attr.value_type}]`);
});
```

---

#### `deleteMediaAttribute(mediaId: number, key: string): void`

メディアの特定の属性を削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | ✓ | メディアID |
| `key` | `string` | ✓ | 属性キー |

**使用例:**

```typescript
db.deleteMediaAttribute(1, 'rating');
```

---

#### `deleteAllMediaAttributes(mediaId: number): void`

メディアの全属性を一括削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | ✓ | メディアID |

**使用例:**

```typescript
db.deleteAllMediaAttributes(1);
```

---

### トランザクション

#### `transaction<T>(fn: () => T): T`

トランザクション内で複数の操作を実行します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `fn` | `() => T` | ✓ | トランザクション内で実行する関数 |

**戻り値:** `T` - 関数の戻り値

**動作:**
- 関数が正常終了すればコミット
- 例外が発生すればロールバック

**使用例:**

```typescript
// 複数操作をアトミックに実行
db.transaction(() => {
  const media = db.createMedia({
    title: 'テストメディア',
    media_type: 'comic',
  });

  const tag = db.createTag('新着');
  db.addTagToMedia(media.id, tag.id);
});

// 戻り値を受け取る
const result = db.transaction(() => {
  const media1 = db.createMedia({ title: 'A', media_type: 'comic' });
  const media2 = db.createMedia({ title: 'B', media_type: 'video' });
  return { media1, media2 };
});

console.log(result.media1.id, result.media2.id);
```

**エラー:**
- トランザクション内でエラーが発生した場合、全てロールバックされ、エラーが再スローされる

---

### バックアップ操作

#### `backup(): Promise<string | undefined>`

データベースを手動でバックアップします。

**パラメータ:** なし

**戻り値:** `Promise<string | undefined>` - バックアップファイルのパス。バックアップマネージャーが設定されていない場合は`undefined`

**動作:**
- バックアップファイルは`BackupOptions.backupDir`で指定したディレクトリに`{dbname}.{timestamp}.db`形式で保存
- better-sqlite3のバックアップAPIを使用して安全にコピー

**使用例:**

```typescript
// バックアップを実行
const backupPath = await db.backup();
if (backupPath) {
  console.log(`バックアップを作成しました: ${backupPath}`);
}
```

**エラー:**
- バックアップ先ディレクトリが存在しない場合: `Error`
- バックアップ中にエラーが発生した場合: `Error`

---

#### `listBackups(): BackupInfo[]`

バックアップファイルの一覧を取得します。

**パラメータ:** なし

**戻り値:** `BackupInfo[]` - バックアップ情報の配列（作成日時の降順）

**BackupInfo:**

| プロパティ | 型 | 説明 |
|-----------|-----|------|
| `id` | `string` | バックアップID |
| `name` | `string` | バックアップファイル名 |
| `path` | `string` | バックアップファイルのパス |
| `createdAt` | `Date` | 作成日時 |
| `scope` | `BackupScope` | バックアップスコープ（`'auto'` / `'manual'` / `'tmp'`） |
| `kind` | `BackupKind` | バックアップ種別（`'full'` / `'diff'`） |
| `label` | `string \| undefined` | ラベル（backupWithLabelで指定時） |

**使用例:**

```typescript
const backups = db.listBackups();
console.log(`バックアップファイル数: ${backups.length}`);
backups.forEach((backup) => {
  console.log(`${backup.name} [${backup.scope}/${backup.kind}] - ${backup.createdAt.toISOString()}`);
});
```

---

#### `backupWithLabel(label: string): Promise<string | null>`

ラベル付き手動バックアップを実行します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `label` | `string` | ✓ | バックアップのラベル（例: `"before_import"`） |

**戻り値:** `Promise<string | undefined>` - バックアップファイルのパス。バックアップマネージャーが設定されていない場合は`undefined`

**使用例:**

```typescript
const backupPath = await db.backupWithLabel('before_import');
if (backupPath) {
  console.log(`バックアップを作成しました: ${backupPath}`);
}
```

---

#### `restore(selector?: BackupSelector): string`

バックアップからDBを復元します。復元前に現在のDBを`tmp/`へ自動退避します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `selector` | `BackupSelector` | | 復元するバックアップの選択条件（省略時: 最新） |

**戻り値:** `string` - 復元に使用したバックアップファイルのパス

**エラー:**
- バックアップマネージャーが設定されていない場合: `Error`
- 条件に一致するバックアップが存在しない場合: `Error`

**使用例:**

```typescript
import { BackupSelector } from 'kijuku-db';

// 最新のバックアップから復元
const restoredPath = db.restore();

// N番目に新しいバックアップから復元
const restoredPath2 = db.restore(BackupSelector.nth(1));
```

**注意:**
- 復元後はDB接続が再オープンされます。`KijukuDB`インスタンスはそのまま使用可能です

---

#### `getBackupManager(): BackupManager | undefined`

BackupManagerインスタンスを取得します（高度な使用）。

**パラメータ:** なし

**戻り値:** `BackupManager | undefined` - バックアップマネージャー。バックアップ設定なしで初期化した場合は`undefined`

**使用例:**

```typescript
const manager = db.getBackupManager();
if (manager) {
  const backups = manager.listBackups();
  console.log(`バックアップ数: ${backups.length}`);
}
```

---

### サムネイル操作

#### `checkThumbnail(filter?: MediaFilter, options?: QueryOptions): CheckThumbnailResult`

フィルタで絞り込んだメディアのサムネイル状態をチェックします（ファイル生成は行いません）。

**パラメータ:**

| パラメータ | 型 | デフォルト | 説明 |
|-----------|-----|-----------|------|
| `filter` | `MediaFilter` | `{}` | 対象メディアの絞り込み条件 |
| `options` | `QueryOptions` | | ページネーション等のオプション |

**戻り値:** `CheckThumbnailResult`

```typescript
interface CheckThumbnailResult {
  total: number;
  ok: number;
  missing: number;
  file_not_found: number;
  skipped: number;
  details: CheckThumbnailItemResult[];
}
```

**使用例:**

```typescript
const result = db.checkThumbnail({ media_type: 'comic' });
console.log(`OK: ${result.ok}件, 未生成: ${result.missing}件`);
```

---

#### `updateThumbnail(filter?: MediaFilter, options?: QueryOptions, thumbnailOptions?: ThumbnailOptions): UpdateThumbnailResult`

フィルタで絞り込んだメディアのサムネイルを生成・更新します。ImageMagick (`convert`) が必要です。

**パラメータ:**

| パラメータ | 型 | デフォルト | 説明 |
|-----------|-----|-----------|------|
| `filter` | `MediaFilter` | `{}` | 対象メディアの絞り込み条件 |
| `options` | `QueryOptions` | | ページネーション等のオプション |
| `thumbnailOptions` | `ThumbnailOptions` | `{}` | サムネイル生成オプション |

**ThumbnailOptions:**

| プロパティ | 型 | デフォルト | 説明 |
|-----------|-----|-----------|------|
| `dry_run` | `boolean` | `false` | DBを更新せず結果を出力のみ |
| `force` | `boolean` | `false` | 既存サムネイルを強制再生成 |

**戻り値:** `UpdateThumbnailResult`

```typescript
interface UpdateThumbnailResult {
  total: number;
  generated: number;
  already_exists: number;
  skipped: number;
  errors: number;
  details: UpdateThumbnailItemResult[];
}
```

**使用例:**

```typescript
const result = db.updateThumbnail({}, undefined, { dry_run: true });
console.log(`生成予定: ${result.generated}件`);
```

---

### バックアップ読み取り操作

バックアップファイルから読み取り専用でデータを取得します。リストアせずに過去のデータを確認する用途に使用します。

#### `getMediaFromBackup(id: number, selector?: BackupSelector): Media | null`

バックアップからメディアをIDで取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `id` | `number` | ✓ | メディアID |
| `selector` | `BackupSelector` | | バックアップ選択条件（省略時: 最新） |

**戻り値:** `Media | null` - メディア情報

**使用例:**

```typescript
const media = db.getMediaFromBackup(1);
const mediaFromOld = db.getMediaFromBackup(1, BackupSelector.nth(2));
```

---

#### `findMediaFromBackup(filter: MediaFilter, options?: QueryOptions, selector?: BackupSelector): Media[]`

バックアップから条件に合うメディアを検索します。

**パラメータ:** `findMedia`と同じ（`selector`が追加）

**戻り値:** `Media[]`

**使用例:**

```typescript
const media = db.findMediaFromBackup({ media_type: 'comic' });
```

---

#### `getTagByNameFromBackup(name: string, selector?: BackupSelector): Tag | null`

バックアップからタグを名前で取得します。

**戻り値:** `Tag | null`

---

#### `getAllTagsFromBackup(selector?: BackupSelector): Tag[]`

バックアップから全タグを取得します。

**戻り値:** `Tag[]`

---

#### `getMediaTagsFromBackup(mediaId: number, selector?: BackupSelector): Tag[]`

バックアップからメディアに紐付くタグを取得します。

**戻り値:** `Tag[]`

---

#### `getMediaAttributeFromBackup(mediaId: number, key: string, selector?: BackupSelector): MediaAttribute | null`

バックアップからメディアの特定の属性を取得します。

**戻り値:** `MediaAttribute | null`

---

#### `getMediaAttributesFromBackup(mediaId: number, selector?: BackupSelector): MediaAttribute[]`

バックアップからメディアの全属性を取得します。

**戻り値:** `MediaAttribute[]`

---

### ファイル存在チェック

#### `updateExist(filter: MediaFilter, options?: QueryOptions, updateOptions?: UpdateExistOptions): UpdateExistResult`

メディアの`path`を確認し、ファイルが存在するかどうかに基づいて`flag_exist`を一括更新します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `filter` | `MediaFilter` | ✓ | 対象メディアの絞り込み条件 |
| `options` | `QueryOptions` | | ページネーション等のオプション |
| `updateOptions` | `UpdateExistOptions` | | 更新オプション |

**UpdateExistOptions:**

| プロパティ | 型 | デフォルト | 説明 |
|-----------|-----|-----------|------|
| `dry_run` | `boolean` | `false` | DBを更新せず結果のみを返す |

**戻り値:** `UpdateExistResult`

| プロパティ | 型 | 説明 |
|-----------|-----|------|
| `total` | `number` | チェック対象の総数 |
| `updated` | `number` | 更新された件数 |
| `updated_ids` | `number[] \| null` | 更新されたメディアIDの配列 |
| `updated_ids_file` | `string \| null` | 更新ID一覧のファイルパス |
| `detail_file` | `string` | 詳細結果のファイルパス |

**使用例:**

```typescript
// ドライラン（結果のみ確認）
const result = db.updateExist({}, undefined, { dry_run: true });
console.log(`更新対象: ${result.updated}件`);

// 実行
const result = db.updateExist({ media_type: 'comic' });
console.log(`${result.updated}/${result.total}件を更新`);
```

---

### その他

#### `close(): void`

データベース接続を閉じます。

**パラメータ:** なし

**戻り値:** なし

**使用例:**

```typescript
const db = new KijukuDB('./data/kijuku.db');
// ... 処理 ...
db.close();
```

**注意:**
- 接続を閉じた後は、再度メソッドを呼び出すとエラーになります

---

#### `getTables(): string[]`

データベース内のテーブル一覧を取得します（テスト用）。

**パラメータ:** なし

**戻り値:** `string[]` - テーブル名の配列

---

#### `isForeignKeysEnabled(): boolean`

外部キー制約が有効かどうかを確認します（テスト用）。

**パラメータ:** なし

**戻り値:** `boolean` - 有効なら`true`

---

#### `getTableInfo(tableName: string): TableColumnInfo[]`

指定テーブルのカラム情報を取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `tableName` | `string` | ✓ | テーブル名 |

**戻り値:** `TableColumnInfo[]` - カラム情報の配列

**TableColumnInfo:**

| プロパティ | 型 | 説明 |
|-----------|-----|------|
| `cid` | `number` | カラムID |
| `name` | `string` | カラム名 |
| `type` | `string` | データ型 |
| `notnull` | `boolean` | NOT NULL制約 |
| `default_value` | `string \| null` | デフォルト値 |
| `pk` | `number` | 主キー（0: 非PK, 1以上: PKの順序） |

**使用例:**

```typescript
const columns = db.getTableInfo('media');
columns.forEach((col) => {
  console.log(`${col.name} (${col.type})${col.notnull ? ' NOT NULL' : ''}`);
});
```

---

## RemoteKijukuDB

SSH経由でリモートサーバーのDBを操作するクラス。ローカルの`KijukuDB`と同等のAPIを非同期（`Promise`）で提供します。

### RemoteKijukuDBコンストラクタ

#### `new RemoteKijukuDB(config: RemoteConfig)`

**RemoteConfig:**

| プロパティ | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `sshHost` | `string` | ✓ | SSH接続先ホスト（.ssh/configのHost名） |
| `port` | `number` | | SSHポート（省略時はSSH設定から読み取り） |
| `dbPath` | `string` | | リモートのDBファイルパス（デフォルト: `~/.local/share/kijuku/kijuku.db`） |
| `binaryPath` | `string` | | リモートのkijuku-cliパス（デフォルト: `~/.local/bin/kijuku-cli`） |

**使用例:**

```typescript
import { RemoteKijukuDB } from 'kijuku-db';

const remoteDb = new RemoteKijukuDB({
  sshHost: 'example.com',
  dbPath: '/path/to/kijuku.db',
});
```

### RemoteKijukuDB対応メソッド一覧

以下のメソッドは`KijukuDB`と同じシグネチャですが、戻り値が`Promise`で包まれます。

| カテゴリ | メソッド | 備考 |
|---------|---------|------|
| **マイグレーション** | `migrate()`, `getSchemaVersion()`, `getTables()`, `getTableInfo()` | |
| **メディアCRUD** | `createMedia()`, `getMedia()`, `updateMedia()`, `deleteMedia()` | |
| **メディア検索** | `findMedia()`, `getDistinctValues()` | |
| **バルク操作** | `bulkCreateMedia()`, `bulkDeleteMedia()`, `bulkUpdateMedia()` | |
| **タグ操作** | `createTag()`, `getTagByName()`, `getAllTags()`, `addTagToMedia()`, `removeTagFromMedia()`, `getMediaTags()`, `getTagUsageStats()`, `findUnusedTags()` | |
| **属性操作** | `setMediaAttribute()`, `getMediaAttribute()`, `getMediaAttributes()`, `deleteMediaAttribute()`, `deleteAllMediaAttributes()` | |
| **サムネイル** | `checkThumbnail()`, `updateThumbnail()` | |
| **ファイル存在** | `updateExist()` | |

**相違点:**
- 全メソッドが`Promise`を返す（例: `getMedia(id): Promise<Media | null>`）
- `close()`は不要（使用後に自動的に切断される）

### RemoteKijukuDBバックアップ操作

#### `backup(label?: string, timeoutMs?: number): Promise<string>`

リモートDBの手動バックアップを実行します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `label` | `string` | | バックアップのラベル（省略時はラベルなし） |
| `timeoutMs` | `number` | | タイムアウト（ms）。省略時はDBサイズから自動計算 |

**戻り値:** `Promise<string>` - バックアップファイルのパス（リモートサーバー上）

**使用例:**

```typescript
const path = await remoteDb.backup();
const pathWithLabel = await remoteDb.backup('before_import');
// タイムアウトを明示的に指定（例: 30分）
const pathWithTimeout = await remoteDb.backup('large_db', 30 * 60_000);
```

---

#### `listBackups(): Promise<BackupInfo[]>`

リモートDBのバックアップ一覧を取得します。

**戻り値:** `Promise<BackupInfo[]>` - バックアップ情報の配列（作成日時の降順）

**使用例:**

```typescript
const backups = await remoteDb.listBackups();
backups.forEach(b => console.log(`${b.name} (${b.scope})`));
```

---

#### `restore(selector?: RemoteBackupSelector, timeoutMs?: number): Promise<string>`

リモートDBをバックアップから復元します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `selector` | `RemoteBackupSelector` | | 復元するバックアップの選択条件（省略時: 最新） |
| `timeoutMs` | `number` | | タイムアウト（ms）。省略時はDBサイズから自動計算 |

**RemoteBackupSelector:**

```typescript
type RemoteBackupSelector =
  | { type: 'latest' }
  | { type: 'nth'; n: number };
```

**戻り値:** `Promise<string>` - 復元に使用したバックアップファイルのパス

**使用例:**

```typescript
// 最新から復元
const path = await remoteDb.restore();
const path2 = await remoteDb.restore({ type: 'latest' });

// 2番目に新しいバックアップから復元
const path3 = await remoteDb.restore({ type: 'nth', n: 1 });
```

---

## Rust SDK

Rust SDKはTypeScript SDKと同等のAPIを提供します。主な相違点:

### 初期化

```rust
use kijuku_db::{KijukuDB, DBOptions};

// 通常
let db = KijukuDB::open("./data/kijuku.db")?;

// オプション指定
let db = KijukuDB::open_with_options("./data/kijuku.db", DBOptions::default())?;

// インメモリ
let db = KijukuDB::open_in_memory()?;
```

### メソッド対応表

Rust SDKのメソッドはsnake_caseで、戻り値が`Result<T>`で包まれます:

| TypeScript | Rust | 備考 |
|-----------|------|------|
| `new KijukuDB(path)` | `KijukuDB::open(path)` | コンストラクタ → ファクトリメソッド |
| `createMedia(data)` | `create_media(&self, input: &MediaInput)` | |
| `getMedia(id)` | `get_media(&self, id: i64) -> Option<Media>` | |
| `updateMedia(id, data)` | `update_media(&self, id: i64, input: &MediaUpdateInput)` | `Partial<MediaInput>` → `MediaUpdateInput` |
| `deleteMedia(id)` | `delete_media(&self, id: i64)` | |
| `findMedia(filter, opts)` | `find_media(&self, filter: &MediaFilter, options: Option<&QueryOptions>)` | |
| `getDistinctValues(fields, filter)` | `get_distinct_values(&self, fields: &[&str], filter: &MediaFilter)` | |
| `bulkCreateMedia(list)` | `bulk_create_media(&self, data_list: &[MediaInput])` | |
| `bulkDeleteMedia(ids)` | `bulk_delete_media(&self, ids: &[i64])` | |
| `bulkUpdateMedia(updates)` | `bulk_update_media(&self, updates: &[BulkUpdateItem])` | |
| `createTag(name)` | `create_tag(&self, name: &str)` | |
| `getTagByName(name)` | `get_tag_by_name(&self, name: &str) -> Option<Tag>` | |
| `getAllTags()` | `get_all_tags(&self)` | |
| `addTagToMedia(mid, tid)` | `add_tag_to_media(&self, media_id: i64, tag_id: i64)` | |
| `removeTagFromMedia(mid, tid)` | `remove_tag_from_media(&self, media_id: i64, tag_id: i64)` | |
| `getMediaTags(mid)` | `get_media_tags(&self, media_id: i64)` | |
| `getTagUsageStats()` | `get_tag_usage_stats(&self)` | |
| `findUnusedTags()` | `find_unused_tags(&self)` | |
| `setMediaAttribute(mid, k, v, vt)` | `set_media_attribute(&self, media_id: i64, key: &str, value: Option<&str>, value_type: Option<AttributeValueType>)` | |
| `getMediaAttribute(mid, k)` | `get_media_attribute(&self, media_id: i64, key: &str)` | |
| `getMediaAttributes(mid)` | `get_media_attributes(&self, media_id: i64)` | |
| `deleteMediaAttribute(mid, k)` | `delete_media_attribute(&self, media_id: i64, key: &str)` | |
| `deleteAllMediaAttributes(mid)` | `delete_all_media_attributes(&self, media_id: i64)` | |
| `migrate()` | `migrate(&self)` | |
| `getSchemaVersion()` | `get_schema_version(&self) -> Result<i64>` | |
| `getTables()` | `get_tables(&self)` | |
| `getTableInfo(table)` | `get_table_info(&self, table_name: &str)` | |
| `isForeignKeysEnabled()` | `is_foreign_keys_enabled(&self)` | |
| `checkThumbnail(filter, opts)` | `check_thumbnail(&self, filter: &MediaFilter, options: Option<&QueryOptions>)` | |
| `updateThumbnail(filter, opts, to)` | `update_thumbnail(&self, filter: &MediaFilter, options: Option<&QueryOptions>, thumbnail_options: &ThumbnailOptions)` | |
| `updateExist(filter, opts, uo)` | `update_exist(&self, filter: &MediaFilter, options: Option<&QueryOptions>, update_options: &UpdateExistOptions)` | |
| `backup()` | `backup(&self) -> Result<Option<String>>` | |
| `backupWithLabel(label)` | `backup_with_label(&self, label: &str)` | |
| `restore(selector)` | `restore(&mut self, selector: &BackupSelector) -> Result<PathBuf>` | `&mut self` |
| `listBackups()` | `list_backups(&self) -> Result<Vec<BackupInfo>>` | |
| `getBackupManager()` | `get_backup_manager(&self) -> Option<&BackupManager>` | |
| `getMediaFromBackup(id, sel)` | `get_media_from_backup(&self, id: i64, selector: &BackupSelector)` | |
| `findMediaFromBackup(f, o, s)` | `find_media_from_backup(&self, filter: &MediaFilter, options: Option<&QueryOptions>, selector: &BackupSelector)` | |
| `getTagByNameFromBackup(n, s)` | `get_tag_by_name_from_backup(&self, name: &str, selector: &BackupSelector)` | |
| `getAllTagsFromBackup(sel)` | `get_all_tags_from_backup(&self, selector: &BackupSelector)` | |
| `getMediaTagsFromBackup(mid, s)` | `get_media_tags_from_backup(&self, media_id: i64, selector: &BackupSelector)` | |
| `getMediaAttributeFromBackup(mid, k, s)` | `get_media_attribute_from_backup(&self, media_id: i64, key: &str, selector: &BackupSelector)` | |
| `getMediaAttributesFromBackup(mid, s)` | `get_media_attributes_from_backup(&self, media_id: i64, selector: &BackupSelector)` | |
| `close()` | `close(self)` | `self`を消費 |
| `transaction(fn)` | `transaction<F, T>(&self, f: F) -> Result<T>` | `FnOnce(&Self) -> Result<T>` |

### Rust RemoteKijukuDB

Rust SDKにも`RemoteKijukuDB`が存在し、同じAPIをSSH経由で提供します。詳細はRemoteKijukuDBセクションを参照してください。

---

## Web GUIサーバー

### `startServer(db: KijukuDB, options: ServerOptions): void`

認証付きWeb GUIサーバーを起動します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `db` | `KijukuDB` | ✓ | データベースインスタンス |
| `options` | `ServerOptions` | ✓ | サーバー設定オプション |

**ServerOptions:**

| プロパティ | 型 | 必須 | デフォルト | 説明 |
|-----------|-----|------|----------|------|
| `port` | `number` | | `40001` | サーバーのポート番号 |
| `password` | `string` | | (自動生成) | 認証パスワード |

**戻り値:** なし

**動作:**
1. パスワードが指定されていない場合、12文字のランダムパスワードを生成
2. セッション管理システムを初期化
3. 指定されたポートでHTTPサーバーを起動
4. コンソールにURL・パスワードを表示

**使用例:**

```typescript
import { KijukuDB, startServer } from 'kijuku-db';

const db = new KijukuDB('./data/kijuku.db');
db.migrate();

// デフォルト設定で起動
startServer(db, { port: 40001 });

// パスワード指定
startServer(db, {
  port: 8080,
  password: 'mypassword123'
});
```

**出力例:**

```
Kijuku DB Web GUI Server
========================
URL: http://localhost:40001
Password: Ab12Cd34Ef56

Press Ctrl+C to stop the server
```

### APIエンドポイント

#### `POST /api/auth/login`

ログインを行います。

**リクエストボディ:**
```json
{
  "password": "string"
}
```

**レスポンス（成功時）:**
```json
{
  "success": true
}
```

**HTTPステータス:**
- 200: ログイン成功
- 401: パスワードが間違っている

---

#### `POST /api/auth/logout`

ログアウトを行います。

**レスポンス:**
```json
{
  "success": true
}
```

---

#### `GET /api/media`

メディア一覧を取得します（認証必須）。

**クエリパラメータ:**

| 名前 | 型 | 説明 |
|------|-----|------|
| `title` | `string` | タイトルで検索（部分一致） |
| `artist` | `string` | 作者で検索（部分一致） |
| `series` | `string` | シリーズで検索（部分一致） |
| `media_type` | `string` | メディアタイプ（comic/video/music） |
| `limit` | `number` | 取得件数（デフォルト: 20） |
| `offset` | `number` | オフセット（デフォルト: 0） |
| `orderBy` | `string` | ソートフィールド |
| `order` | `string` | ソート順（ASC/DESC） |

**レスポンス:**
```json
{
  "media": [/* Media配列 */],
  "count": 20,
  "total": 250
}
```

**レスポンスフィールド:**

| フィールド | 型 | 説明 |
|-----------|-----|------|
| `media` | `Media[]` | メディアオブジェクトの配列 |
| `count` | `number` | このページで取得したメディアの件数（`media.length`と同じ） |
| `total` | `number` | フィルタ条件に一致する全件数 |

**HTTPステータス:**
- 200: 成功
- 401: 未認証

---

#### `GET /api/media/:id`

メディア詳細を取得します（認証必須）。

**パスパラメータ:**

| 名前 | 型 | 説明 |
|------|-----|------|
| `id` | `number` | メディアID |

**レスポンス:**
```json
{
  "media": {/* Mediaオブジェクト */},
  "tags": [/* Tag配列 */],
  "attributes": [/* MediaAttribute配列 */]
}
```

**HTTPステータス:**
- 200: 成功
- 401: 未認証
- 404: メディアが見つからない

---

## 型定義

### MediaType

```typescript
type MediaType = 'comic' | 'video' | 'music';
```

メディアの種類を表す型。

---

### Media

```typescript
interface Media {
  id: number;
  uuid: string;
  title: string;
  title_id?: string;
  path?: string;
  media_type: MediaType;
  thumbnail_path?: string;
  artist?: string;
  artist_id?: string;
  description?: string;
  file_size?: number;
  duration_sec?: number;
  page_count?: number;
  series?: string;
  volume_number?: number;
  volume_text?: string;
  volume_title?: string;
  magazine?: string;
  magazine_id?: string;
  language?: string;
  source?: string;
  external_id?: string;
  artist_en?: string;
  title_en?: string;
  chapters?: string;
  extension?: string;
  flag_exist: boolean;
  created_at: Date;
  updated_at: Date;
  title_pron?: string;
  artist_pron?: string;
  series_pron?: string;
}
```

データベースから取得されるメディア情報の完全な型。

---

### MediaInput

```typescript
interface MediaInput {
  title: string;
  media_type: MediaType;
  uuid?: string;
  // ... その他のオプションフィールド
}
```

メディア作成・更新時の入力型。`title`と`media_type`は必須。

---

### MediaFilter

```typescript
interface MediaFilter {
  title?: string;
  title_id?: string;
  artist?: string;
  artist_id?: string;
  media_type?: MediaType;
  series?: string;
  source?: string;
  tag_ids?: number[];
  flag_exist?: boolean;
  language?: string;
  magazine?: string;
  magazine_id?: string;
  extension?: string;
  external_id?: string;
  volume_title?: string;
  title_en?: string;
  artist_en?: string;
  id_in?: number[];
  or_filters?: MediaFilter[];
}
```

メディア検索時のフィルタ条件。全てオプション。

---

### SortKey

```typescript
interface SortKey {
  field: string;
  order?: 'ASC' | 'DESC';
}
```

ソートキー（フィールドと方向）。

---

### QueryOptions

```typescript
interface QueryOptions {
  sortKeys?: SortKey[];
  limit?: number;
  offset?: number;
}
```

ソート・ページネーション設定。`sortKeys` に複数のキーを指定することで多段ソートが可能。

---

### BulkUpdateItem

```typescript
interface BulkUpdateItem {
  id: number;
  data: Partial<MediaInput>;
}
```

一括更新時の個別アイテム。

---

### Tag

```typescript
interface Tag {
  id: number;
  name: string;
}
```

タグ情報。

---

### DBOptions

```typescript
interface DBOptions {
  timeout?: number;
  readonly?: boolean;
  verbose?: boolean;
  backup?: BackupOptions;
}
```

データベース接続オプション。

---

### BackupOptions

```typescript
interface BackupOptions {
  backupDir?: string;         // バックアップ保存先（省略時: dbPathの親ディレクトリ/backup/）
  intervalMs?: number;        // バックアップ間隔（ミリ秒、デフォルト: 3600000 = 1時間）
  enabled?: boolean;          // バックアップを有効化（デフォルト: true）
  onProgress?: (info: { totalPages: number; remainingPages: number }) => void;  // バックアップ進捗コールバック
}
```

自動バックアップ設定オプション。

**使用例:**

```typescript
const db = new KijukuDB('./data/kijuku.db', {
  backup: {
    backupDir: './backups',
    intervalMs: 1800000,  // 30分間隔
    onProgress: (info) => {
      console.log(`Backup progress: ${info.totalPages - info.remainingPages} / ${info.totalPages} pages completed`);
    }
  }
});
```

---

### TagUsageStats

```typescript
interface TagUsageStats {
  tag_id: number;
  tag_name: string;
  count: number;
}
```

タグごとの使用状況。

---

### MediaAttribute

```typescript
interface MediaAttribute {
  media_id: number;
  key: string;
  value?: string;
  value_type: 'string' | 'integer' | 'boolean';
}
```

メディアの拡張属性（EAVモデル）。

---

### BackupInfo

```typescript
interface BackupInfo {
  id: string;
  name: string;
  path: string;
  createdAt: Date;
  scope: BackupScope;
  kind: BackupKind;
  label?: string;
}
```

バックアップファイルのメタデータ。

---

### BackupScope

```typescript
type BackupScope = 'auto' | 'manual' | 'tmp';
```

- `auto`: 自動バックアップ
- `manual`: 手動バックアップ
- `tmp`: 一時バックアップ（リストア時の退避等）

---

### BackupKind

```typescript
type BackupKind = 'full' | 'diff';
```

- `full`: 完全バックアップ
- `diff`: 差分バックアップ

---

### BackupSelector

バックアップ選択のためのユーティリティクラス。

```typescript
class BackupSelector {
  static latest(): BackupSelector;
  static nth(n: number): BackupSelector;
  static withLabel(label: string): BackupSelector;
}
```

**使用例:**

```typescript
import { BackupSelector } from 'kijuku-db';

db.restore(BackupSelector.latest());
db.restore(BackupSelector.nth(1));          // 2番目に新しい
db.restore(BackupSelector.withLabel('before_import'));
```

---

### RemoteBackupSelector

```typescript
type RemoteBackupSelector =
  | { type: 'latest' }
  | { type: 'nth'; n: number };
```

リモートバックアップの選択条件。

---

### ThumbnailOptions

```typescript
interface ThumbnailOptions {
  dry_run?: boolean;   // DBを更新せず結果を出力のみ
  force?: boolean;     // 既存サムネイルを強制再生成
}
```

サムネイル生成オプション。

---

### CheckThumbnailResult

```typescript
interface CheckThumbnailResult {
  total: number;
  ok: number;
  missing: number;
  file_not_found: number;
  skipped: number;
  details: CheckThumbnailItemResult[];
}

interface CheckThumbnailItemResult {
  id: number;
  title: string;
  path?: string;
  thumbnail_path?: string;
  status: 'ok' | 'missing' | 'file_not_found' | 'skipped';
}
```

---

### UpdateThumbnailResult

```typescript
interface UpdateThumbnailResult {
  total: number;
  generated: number;
  already_exists: number;
  skipped: number;
  errors: number;
  details: UpdateThumbnailItemResult[];
}

interface UpdateThumbnailItemResult {
  id: number;
  title: string;
  path?: string;
  thumbnail_path?: string;
  status: 'generated' | 'already_exists' | 'skipped' | 'error';
  error?: string;
}
```

---

### UpdateExistOptions

```typescript
interface UpdateExistOptions {
  dry_run?: boolean;   // DBを更新せず結果のみを返す
}
```

---

### UpdateExistResult

```typescript
interface UpdateExistResult {
  total: number;
  updated: number;
  updated_ids: number[] | null;
  updated_ids_file: string | null;
  detail_file: string;
}
```

---

### TableColumnInfo

```typescript
interface TableColumnInfo {
  cid: number;
  name: string;
  type: string;
  notnull: boolean;
  default_value: string | null;
  pk: number;
}
```

テーブルカラムのメタデータ。

---

### RemoteConfig

```typescript
interface RemoteConfig {
  sshHost: string;
  port?: number;
  dbPath?: string;
  binaryPath?: string;
}
```

SSH経由のリモートDB接続設定。

---

## エラーハンドリング

### SQLiteエラー

better-sqlite3は同期APIのため、エラーは直接スローされます。

**主なエラーコード:**

| エラー | 説明 | 対処方法 |
|-------|------|---------|
| `SQLITE_CANTOPEN` | ファイルを開けない | パス確認、権限確認 |
| `SQLITE_READONLY` | 読み取り専用DBへの書き込み | `readonly: false`で開く |
| `SQLITE_CONSTRAINT` | 制約違反（UNIQUE, NOT NULL等） | データを確認 |
| `SQLITE_ERROR` | 一般的なSQLエラー | SQLクエリを確認 |

**エラーハンドリング例:**

```typescript
try {
  const media = db.createMedia({
    title: 'テスト',
    media_type: 'comic',
    path: '/duplicate/path.cbz',
  });
} catch (error) {
  if (error instanceof Error) {
    if (error.message.includes('UNIQUE constraint failed')) {
      console.error('パスが重複しています');
    } else if (error.message.includes('NOT NULL constraint failed')) {
      console.error('必須フィールドが不足しています');
    } else {
      console.error('エラー:', error.message);
    }
  }
}
```

### トランザクションエラー

```typescript
try {
  db.transaction(() => {
    db.createMedia({ title: 'A', media_type: 'comic' });
    throw new Error('意図的なエラー');
  });
} catch (error) {
  console.error('トランザクションがロールバックされました');
}
```

---

## 環境変数

TypeScript SDK CLI が読み込む環境変数：

| 変数名 | 説明 | デフォルト |
|--------|------|-----------|
| `DATABASE_PATH` | データベースファイルのパス | `./kijuku.db` |
| `KIJUKU_DB_VERBOSE` | SQLログ出力（`true` で有効） | `false` |

**設定例:**

```bash
export DATABASE_PATH=/var/lib/kijuku/kijuku.db
export KIJUKU_DB_VERBOSE=true
```

> **Rust CLI の場合:** `KIJUKU_DB_VERBOSE` 環境変数ではなく `--verbose` フラグを使用してください。
>
> ```bash
> kijuku-cli --db ./kijuku.db --verbose update-exist
> ```

---

## パフォーマンスに関する注意

### インデックスの活用

以下のフィールドにはインデックスが設定されています：
- `title_id`, `artist_id`（完全一致検索用）
- `media_type`
- `series`
- `source`
- `media_type, created_at`（複合インデックス）

これらのフィールドでの検索は高速です。

### 部分一致検索の注意

`title`, `artist`, `series`での部分一致検索（LIKE `%value%`）はインデックスを使用しないため、大量データでは遅くなる可能性があります。

### トランザクションの活用

複数の書き込み操作を行う場合は、`transaction()`を使用することでパフォーマンスが向上します。

```typescript
// 遅い
for (let i = 0; i < 1000; i++) {
  db.createMedia({ title: `Media ${i}`, media_type: 'comic' });
}

// 速い
db.transaction(() => {
  for (let i = 0; i < 1000; i++) {
    db.createMedia({ title: `Media ${i}`, media_type: 'comic' });
  }
});

// さらに速い
db.bulkCreateMedia(
  Array.from({ length: 1000 }, (_, i) => ({
    title: `Media ${i}`,
    media_type: 'comic',
  }))
);
```

---

## 参考リンク

- [README.md](../README.md) - プロジェクト概要
- [設計ドキュメント](design/decisions.md) - 設計決定事項
- [サンプルコード](../examples/README.md) - 使用例
