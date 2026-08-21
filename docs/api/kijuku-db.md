# KijukuDB（ローカル）

ローカル SQLite に直接接続する `KijukuDB` クラス（TypeScript SDK）のAPI仕様。SSH リモート経由は [remote.md](./remote.md)、バックアップ・prod/stg 運用は [backup-sync.md](./backup-sync.md)、Rust SDK は [rust.md](./rust.md) を参照。

## コンストラクタ

### `constructor(dbPath: string, options?: DBOptions)`

データベース接続を初期化します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `dbPath` | `string` | 必須 | データベースファイルのパス。`:memory:`を指定するとインメモリDBになります |
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

## マイグレーション

### `migrate(): void`

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

### `getSchemaVersion(): number`

現在のデータベーススキーマバージョンを取得します。

**パラメータ:** なし

**戻り値:** `number` - スキーマバージョン（現在は`6`）

**使用例:**

```typescript
const version = db.getSchemaVersion();
console.log(`Schema version: ${version}`);
```

---

### `getServerVersion(): string`（CLI stdin operation）

リモート CLI バイナリ自身のバージョンを返す stdin operation です（TASK-69・自動デプロイのバージョン比較用）。TypeScript のローカル `KijukuDB` にはメソッドとして存在しません（CLI・RPC 内部で使用）。DB アクセス不要・prod/stg 両バックエンドで共通。`MAJOR.MINOR.PATCH` 形式（例: `"0.2.2"`）。

> **Note:** リモート（`RemoteKijukuDB`）では毎 RPC の先頭でこの operation を呼び、クライアント（ローカル CLI）バージョンと比較してリモート CLI が古い場合に自動デプロイします（設計: 後述の自動デプロイ節）。

**パラメータ:** なし

**戻り値:** `string` - CLI バイナリのバージョン（`CARGO_PKG_VERSION`）

**使用例:**

```typescript
const cliVersion = await db.getServerVersion();
console.log(`CLI version: ${cliVersion}`);
```

---

## メディア操作

### `createMedia(data: MediaInput): Media`

新しいメディアを作成します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `data` | `MediaInput` | 必須 | メディア情報 |

**MediaInput:**

| プロパティ | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `title` | `string` | 必須 | タイトル |
| `media_type` | `'comic' \| 'video' \| 'music'` | 必須 | メディアタイプ |
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

### `getMedia(id: number): Media | null`

IDでメディアを取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `id` | `number` | 必須 | メディアID |

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

### `getMediaByUuid(uuid: string): Media | null`

UUIDでメディアを取得します（uuidカラムはUNIQUEのため単一取得）。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `uuid` | `string` | 必須 | メディアUUID |

**戻り値:** `Media | null` - メディア情報。存在しない場合は`null`

**使用例:**

```typescript
const media = db.getMediaByUuid('550e8400-e29b-41d4-a716-446655440000');
if (media) {
  console.log(`Title: ${media.title}`);
} else {
  console.log('Media not found');
}
```

---

### `updateMedia(id: number, data: Partial<MediaInput>): void`

メディア情報を更新します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `id` | `number` | 必須 | 更新対象のメディアID |
| `data` | `Partial<MediaInput>` | 必須 | 更新する項目（一部のみ指定可能） |

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

### `deleteMedia(id: number): void`

メディアを削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `id` | `number` | 必須 | 削除対象のメディアID |

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

### `findMedia(filter: MediaFilter, options?: QueryOptions): Media[]`

条件に合うメディアを検索します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `filter` | `MediaFilter` | 必須 | 検索条件 |
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
| `uuid` | `string` | UUID完全一致（uuidカラムはUNIQUE） |
| `uuid_in` | `string[]` | UUIDのIN句フィルタ（複数UUIDを一括フェッチする場合に使用）。999件超の場合は自動的にチャンク分割して処理 |
| `volume_title` | `string` | 巻タイトル部分一致検索 |
| `title_en` | `string` | タイトル（英語）部分一致検索 |
| `artist_en` | `string` | 作者名（英語）部分一致検索 |
| `id_in` | `number[]` | IDのIN句フィルタ（複数IDを一括フェッチする場合に使用）。999件超の場合は自動的にチャンク分割して処理 |
| `exclude_ids` | `number[]` | IDのNOT IN句フィルタ（指定IDを除外）。`id_in` の逆。999件超の場合は自動的にチャンク分割（NOT IN 句は AND で結合） |
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

// UUIDで検索
const byUuid = db.findMedia({ uuid: '550e8400-e29b-41d4-a716-446655440000' });

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

### `getDistinctValues(fields: string[], filter: MediaFilter): (string | null)[][]`

指定したフィールド群の重複なしの値の組み合わせ一覧を取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `fields` | `string[]` | 必須 | 対象フィールド名の配列（ホワイトリストで検証） |
| `filter` | `MediaFilter` | 必須 | 絞り込み条件（条件なしの場合は `{}` を渡す） |

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

### `bulkCreateMedia(dataList: MediaInput[]): Media[]`

複数のメディアを一括作成します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `dataList` | `MediaInput[]` | 必須 | メディア情報の配列 |

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

### `bulkDeleteMedia(ids: number[]): void`

複数のメディアを一括削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `ids` | `number[]` | 必須 | 削除対象のメディアIDの配列 |

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

### `bulkUpdateMedia(updates: BulkUpdateItem[]): void`

複数のメディアを一括更新します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `updates` | `BulkUpdateItem[]` | 必須 | 更新情報の配列 |

**BulkUpdateItem:**

| プロパティ | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `id` | `number` | 必須 | 更新対象のメディアID |
| `data` | `Partial<MediaInput>` | 必須 | 更新する項目 |

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

## タグ操作

### `createTag(name: string): Tag`

新しいタグを作成します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `name` | `string` | 必須 | タグ名 |

**戻り値:** `Tag` - 作成されたタグ情報

**使用例:**

```typescript
const tag = db.createTag('お気に入り');
console.log(`Tag ID: ${tag.id}`);
```

**エラー:**
- タグ名が重複: `Error: UNIQUE constraint failed: tags.name`

---

### `getTagByName(name: string): Tag | null`

タグ名でタグを取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `name` | `string` | 必須 | タグ名 |

**戻り値:** `Tag | null` - タグ情報。存在しない場合は`null`

**使用例:**

```typescript
const tag = db.getTagByName('お気に入り');
if (tag) {
  console.log(`Tag ID: ${tag.id}`);
}
```

---

### `getAllTags(): Tag[]`

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

### `addTagToMedia(mediaId: number, tagId: number): void`

メディアにタグを追加します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | 必須 | メディアID |
| `tagId` | `number` | 必須 | タグID |

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

### `removeTagFromMedia(mediaId: number, tagId: number): void`

メディアからタグを削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | 必須 | メディアID |
| `tagId` | `number` | 必須 | タグID |

**戻り値:** なし

**使用例:**

```typescript
db.removeTagFromMedia(1, 2);
```

---

### `getMediaTags(mediaId: number): Tag[]`

メディアに関連付けられたタグを取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | 必須 | メディアID |

**戻り値:** `Tag[]` - タグの配列

**使用例:**

```typescript
const tags = db.getMediaTags(1);
console.log(`Media has ${tags.length} tags`);
tags.forEach((tag) => console.log(`- ${tag.name}`));
```

---

### `getMediaTagsBulk(mediaIds: number[]): Record<number, Tag[]>`

複数メディアのタグを一括取得します（N+1クエリ回避）。`media_tags` JOIN `tags` 1発で取得し、`mediaId` ごとのタグ配列を返します。指定した `mediaId` にタグがない場合、結果にはそのキーが含まれない（または空配列）ことがあります。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaIds` | `number[]` | 必須 | タグを取得するメディアIDの配列 |

**戻り値:** `Record<number, Tag[]>` - メディアIDをキー、そのメディアのタグ配列を値とするオブジェクト

**使用例:**

```typescript
const mediaTags = db.getMediaTagsBulk([1, 2, 3]);
for (const [mediaId, tags] of Object.entries(mediaTags)) {
  console.log(`Media ${mediaId}: ${tags.map((t) => t.name).join(', ')}`);
}
```

> **Rust SDK:** `get_media_tags_bulk(media_ids: &[i64]) -> Result<HashMap<i64, Vec<Tag>>>`。Local/D1 バックエンドは JOIN 1発で取得しますが、Rust `RemoteKijukuDB` はデフォルト実装（`get_media_tags` の N+1）です。

---

### `getTagUsageStats(): TagUsageStats[]`

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

### `findUnusedTags(): Tag[]`

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

## 属性操作

メディアの拡張属性（EAVモデル）を管理します。スキーマに定義されていない任意のキー・バリューをメディアに付与できます。

### `setMediaAttribute(mediaId: number, key: string, value: string | null, valueType?: string): void`

メディアに属性を設定します。同じキーが存在する場合は上書きされます。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | 必須 | メディアID |
| `key` | `string` | 必須 | 属性キー |
| `value` | `string \| null` | 必須 | 属性値（`null`で値なし） |
| `valueType` | `'string' \| 'integer' \| 'boolean'` | | 値の型（デフォルト: `'string'`） |

**使用例:**

```typescript
db.setMediaAttribute(1, 'rating', '5');
db.setMediaAttribute(1, 'is_read', 'true', 'boolean');
db.setMediaAttribute(1, 'page_count_checked', '200', 'integer');
```

---

### `getMediaAttribute(mediaId: number, key: string): MediaAttribute | null`

メディアの特定の属性を取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | 必須 | メディアID |
| `key` | `string` | 必須 | 属性キー |

**戻り値:** `MediaAttribute | null` - 属性情報。存在しない場合は`null`

**使用例:**

```typescript
const attr = db.getMediaAttribute(1, 'rating');
if (attr) {
  console.log(`${attr.key} = ${attr.value} (${attr.value_type})`);
}
```

---

### `getMediaAttributes(mediaId: number): MediaAttribute[]`

メディアの全属性を取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | 必須 | メディアID |

**戻り値:** `MediaAttribute[]` - 属性の配列

**使用例:**

```typescript
const attrs = db.getMediaAttributes(1);
attrs.forEach((attr) => {
  console.log(`${attr.key}: ${attr.value} [${attr.value_type}]`);
});
```

---

### `deleteMediaAttribute(mediaId: number, key: string): void`

メディアの特定の属性を削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | 必須 | メディアID |
| `key` | `string` | 必須 | 属性キー |

**使用例:**

```typescript
db.deleteMediaAttribute(1, 'rating');
```

---

### `deleteAllMediaAttributes(mediaId: number): void`

メディアの全属性を一括削除します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `mediaId` | `number` | 必須 | メディアID |

**使用例:**

```typescript
db.deleteAllMediaAttributes(1);
```

---

## ハッシュ操作

### `addMediaHash(input: MediaHashInput): MediaHash`

メディアハッシュを登録（単件）。既存のPKと同じ場合はupsert。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `input` | `MediaHashInput` | 必須 | ハッシュ入力データ |

### `addMediaHashes(inputs: MediaHashInput[]): MediaHash[]`

メディアハッシュを一括登録。トランザクション内で処理。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `inputs` | `MediaHashInput[]` | 必須 | ハッシュ入力データ配列 |

### `getMediaHashes(itemUuid: string): MediaHash[]`

特定作品の全ハッシュを取得。

### `getMediaHash(itemUuid: string, filename: string, timeRange: string): MediaHash | null`

特定位置のハッシュを取得。

### `findByContentHash(hashBytes: Uint8Array): MediaHash[]`

SHA256による完全一致検索。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `hashBytes` | `Uint8Array` | 必須 | SHA256ハッシュ値（32バイト） |

### `deleteMediaHash(itemUuid: string, filename: string, timeRange: string): void`

特定位置のハッシュを削除。代替行（`alternative_of`が該当位置を指す行）も連鎖削除。

### `deleteMediaHashes(itemUuid: string): void`

特定作品のハッシュを全削除。

### `findDuplicateHashes(): Array<{ content_hash: Uint8Array; count: number }>`

重複ハッシュを検出。2件以上の同一`content_hash`を持つエントリを返す。

### `computeMediaHash(itemUuid: string, mediaPath: string, mediaType: string, durationSec?: number): ComputeHashResult`

特定のメディアのハッシュを計算・登録。

メディアタイプごとの計算内容:

| mediaType | 計算内容 |
|-----------|---------|
| `music` | ファイル全体hash + 先頭30秒hash |
| `video` | ファイル全体hashのみ |
| `comic` | 各ページ画像hash + 全体hash（全ページhash結合） |

### `computeMediaHashes(filter: MediaFilter, options?: QueryOptions, force?: boolean): ComputeHashResult[]`

フィルタ条件でメディアを絞り込み、ハッシュを計算・登録。`force=false`の場合、既存ハッシュがある作品はスキップ。

---

## トランザクション

### `transaction<T>(fn: () => T): T`

トランザクション内で複数の操作を実行します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `fn` | `() => T` | 必須 | トランザクション内で実行する関数 |

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

## サムネイル操作

### `checkThumbnail(filter?: MediaFilter, options?: QueryOptions): CheckThumbnailResult`

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

### `updateThumbnail(filter?: MediaFilter, options?: QueryOptions, thumbnailOptions?: ThumbnailOptions): UpdateThumbnailResult`

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

## ファイル存在チェック

### `updateExist(filter: MediaFilter, options?: QueryOptions, updateOptions?: UpdateExistOptions): UpdateExistResult`

メディアの`path`を確認し、ファイルが存在するかどうかに基づいて`flag_exist`を一括更新します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `filter` | `MediaFilter` | 必須 | 対象メディアの絞り込み条件 |
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

## ファイル操作

media root（`DBOptions.mediaRoot`）配下のファイル操作（cp/mv/sync）。すべて **dry-run ファースト**（`opts.apply` は `false` 既定で、計画のみ返し FS を触らない）。上書き・削除で消えるファイルは [trash 操作](#trash操作) 経由で `.trash/` へ退避される。`mediaRoot` 未設定だとエラー。パスは `..`・絶対パス・シンボリックリンク経由での root 脱出を拒否する。

### `getMediaRoot(): string | undefined`

設定された media root を返す（未設定は `undefined`）。ファイル操作 API のサンドボックス境界。

### `mediaCp(src: string, dst: string, opts?: FileOpOptions): FileOpResult`

`src` を `dst` へ複製する（ファイル/ディレクトリ両対応）。`dst` が既存なら上書き前に旧ファイルを trash へ退避する。

```ts
const plan = db.mediaCp('abc/content', 'abc/content_backup'); // dry-run（FS 不変）
console.log(plan.steps, plan.applied);
const done = db.mediaCp('abc/content', 'abc/content_backup', { apply: true, updateDb: false });
```

### `mediaMv(src: string, dst: string, opts?: FileOpOptions): FileOpResult`

`src` を `dst` へ移動する。`dst` が既存なら上書き前に旧ファイルを trash へ退避してから移動する。

### `mediaSync(src: string, dst: string, opts?: FileOpOptions): FileOpResult`

`src`（ディレクトリ）の内容を `dst` へ同期する（safe モード）。dst にあって src に無いファイルは trash へ回し（生 `--delete` 相当だが trash 経由）、内容が異なるファイルは旧内容を trash へ退避してからコピーする。source 側ファイルは削除しない。

## trash操作

ファイル操作で削除・上書きされるファイルを `mediaRoot/.trash/` 配下へ退避する論理削除方式。物理削除を伴うのは `purgeTrash` の実行のみ（`dryRun` は既定 `false` なので、対象を確認してから消すには `dryRun: true` を明示指定）。通常の削除・上書きは trash への移動となり、`restoreFromTrash` で復元できる。

### `moveToTrash(targetRel: string, operation: TrashOperation, reason?: string): string`

`targetRel` を trash へ移動し、trash ID を返す。target は media root 配下の既存パスで `.trash/` 自身でないこと。存在しないパス・`.trash/` 配下のパスはエラー。

### `listTrash(): TrashEntry[]`

trash 内の全エントリを一覧する（trash が無ければ空）。

### `restoreFromTrash(id: string): string`

trash エントリ `id` を元の位置へ復元し、復元先パスを返す。元の位置に既にファイルが存在する場合は**上書きせずエラー**にする（安全停止）。

### `purgeTrash(ids?: string[], dryRun?: boolean): string[]`

trash を物理削除する。`dryRun=true` を指定すると対象 ID の一覧を返すだけで削除しない（**既定は `false` で物理削除を実行する**ため注意）。`ids` を指定すればその ID のみ、未指定なら全エントリを対象とする。**trash からファイルを完全に消す唯一の経路**である。

> **RemoteKijukuDB**: 上記のファイル操作・trash 操作はすべて非同期（`Promise` を返す）で同名で公開されている（`mediaCp` / `mediaMv` / `mediaSync` / `moveToTrash` / `listTrash` / `restoreFromTrash` / `purgeTrash`）。SSH 先のリモート CLI に委譲する。

---

## その他

### `close(): void`

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

### `getTables(): string[]`

データベース内のテーブル一覧を取得します（テスト用）。

**パラメータ:** なし

**戻り値:** `string[]` - テーブル名の配列

---

### `isForeignKeysEnabled(): boolean`

外部キー制約が有効かどうかを確認します（テスト用）。

**パラメータ:** なし

**戻り値:** `boolean` - 有効なら`true`

---

### `getTableInfo(tableName: string): TableColumnInfo[]`

指定テーブルのカラム情報を取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `tableName` | `string` | 必須 | テーブル名 |

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

