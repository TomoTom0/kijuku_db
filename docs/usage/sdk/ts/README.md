# TypeScript SDK利用ガイド

このガイドでは、kijuku-db TypeScript SDKを外部プロジェクトから利用する方法を説明します。

## インストール

### npm/yarn/bunからのインストール

現在、kijuku-dbはnpmレジストリに公開されていません（`private: true`設定）。
ローカル開発での利用方法を以下に示します。

#### 方法1: npm link を使用

開発中のプロジェクトで使用する場合：

```bash
# kijuku-dbプロジェクトで
cd /path/to/kijuku_db/ts-sdk
npm install
npm run build
npm link

# 利用するプロジェクトで
cd /path/to/your-project
npm link kijuku-db
```

#### 方法2: ローカルパスを指定

`package.json`に直接パスを指定：

```json
{
  "dependencies": {
    "kijuku-db": "file:../kijuku_db/ts-sdk"
  }
}
```

```bash
npm install
```

#### 方法3: Git URLを指定（将来の公開後）

```json
{
  "dependencies": {
    "kijuku-db": "git+https://github.com/your-org/kijuku_db.git#main:ts-sdk"
  }
}
```

## 基本的な使い方

### 1. データベースの初期化

```typescript
import { KijukuDB } from 'kijuku-db';

// データベース接続を作成
const db = new KijukuDB('./data/kijuku.db');

// スキーマを初期化（初回のみ）
db.migrate();
```

### 2. メディアの作成

```typescript
const media = db.createMedia({
  title: 'ワンピース 第1巻',
  media_type: 'comic',
  artist: '尾田栄一郎',
  series: 'ワンピース',
  volume_text: '1',  // volume_numberは自動計算されるためvolume_textを使用
  path: '/media/comics/onepiece_v01.cbz',
});

console.log(`メディアID: ${media.id}`);
```

### 2b. メディアの取得・更新・削除

```typescript
// IDで1件取得
const media = db.getMedia(1);
if (media) {
  console.log(media.title);
}

// 部分更新（指定フィールドのみ更新）
db.updateMedia(1, { artist: '新しい作者名', flag_exist: false });

// 削除
db.deleteMedia(1);
```

### 3. メディアの検索

```typescript
// シリーズで検索
const results = db.findMedia(
  { series: 'ワンピース' },
  { sortKeys: [{ field: 'volume_number', order: 'ASC' }] }
);

console.log(`見つかったメディア: ${results.length}件`);
results.forEach(m => {
  console.log(`- ${m.title}`);
});
```

#### OR条件での検索

`or_filters`を使用すると、複雑なOR条件で検索できます：

```typescript
// artist="A" OR artist="B"
const results = db.findMedia({
  or_filters: [
    { artist: 'Author1' },
    { artist: 'Author2' }
  ]
});

// (artist="A" AND series="X") OR (artist="B")
const complex = db.findMedia({
  or_filters: [
    { artist: 'A', series: 'X' },
    { artist: 'B' }
  ]
});

// ネストしたOR条件
const nested = db.findMedia({
  or_filters: [
    {
      media_type: 'comic',
      or_filters: [
        { series: 'X' },
        { series: 'Y' }
      ]
    },
    { artist: 'C' }
  ]
});
// => (media_type='comic' AND (series='X' OR series='Y')) OR (artist='C')
```

**セマンティクス:**
- 同一フィルタ内の条件: AND結合
- `or_filters`間: OR結合
- ネスト可能

#### 特定IDの除外（exclude_ids）

`exclude_ids` を使うと、指定したIDを NOT IN で除外して検索できます。`id_in` の逆で、未視聴メディア取得などで「既知のIDを差し引く」用途に使います。999件超は `id_in` と同様にチャンク分割されます。

```typescript
// 視聴済みIDを除外して未視聴メディアを取得
const watched = [1, 2, 3];
const unwatched = db.findMedia(
  { media_type: 'comic', exclude_ids: watched },
  { sortKeys: [{ field: 'title', order: 'ASC' }] }
);
```

### 3b. フィールドのユニーク値取得

`getDistinctValues` を使うと、特定フィールドの重複なし値一覧や、複数フィールドの組み合わせ一覧を取得できます。

```typescript
import { ALLOWED_DISTINCT_FIELDS } from 'kijuku-db';

// 全artistの一覧（重複なし、昇順）
const rows = db.getDistinctValues(['artist'], {});
const artists = rows.map(r => r[0]); // [null, 'Author1', 'Author2', ...]

// コミックのシリーズ一覧（フィルタあり）
const seriesRows = db.getDistinctValues(['series'], { media_type: 'comic' });
const seriesList = seriesRows.flatMap(r => r[0] ? [r[0]] : []);

// artist × series の組み合わせ一覧
const combinations = db.getDistinctValues(['artist', 'series'], {});
// => [['Author1', 'Series1'], ['Author1', 'Series2'], ['Author2', null], ...]
```

**使用可能なフィールド:** `ALLOWED_DISTINCT_FIELDS` をエクスポートしています。

### 4. タグの管理

```typescript
// タグを作成
const tag = db.createTag('お気に入り');

// タグ名でタグを取得
const existing = db.getTagByName('お気に入り');
if (existing) {
  console.log(`既存タグID: ${existing.id}`);
}

// 全タグを取得
const allTags = db.getAllTags();
console.log(`全タグ数: ${allTags.length}`);

// メディアにタグを追加
db.addTagToMedia(media.id, tag.id);

// メディアからタグを削除
db.removeTagFromMedia(media.id, tag.id);

// メディアのタグを取得
const tags = db.getMediaTags(media.id);
console.log('タグ:', tags.map(t => t.name).join(', '));

// 複数メディアのタグを一括取得（N+1回避: JOIN 1発）
const mediaTags = db.getMediaTagsBulk([1, 2, 3]);
for (const [mid, ts] of Object.entries(mediaTags)) {
  console.log(`Media ${mid}:`, ts.map(t => t.name).join(', '));
}

// タグの使用数統計を取得
const stats = db.getTagUsageStats();
stats.forEach(s => console.log(`${s.tag_name}: ${s.count}件`));

// 未使用タグを取得
const unused = db.findUnusedTags();
console.log(`未使用タグ数: ${unused.length}`);
```

### 5. トランザクション

```typescript
db.transaction(() => {
  const media1 = db.createMedia({
    title: 'メディア1',
    media_type: 'comic'
  });

  const tag = db.createTag('新着');
  db.addTagToMedia(media1.id, tag.id);

  // エラーが発生した場合は全てロールバック
});
```

### 6. データベースのクローズ

```typescript
// 使用後は接続を閉じる
db.close();
```

### 7. データベース情報の取得

```typescript
// スキーマバージョン
const version = db.getSchemaVersion();
console.log(`Schema version: ${version}`);

// テーブル一覧
const tables = db.getTables();
console.log('テーブル:', tables);

// テーブル定義の確認
const columns = db.getTableInfo('media');
columns.forEach(col => {
  console.log(`${col.name} (${col.type})${col.notnull ? ' NOT NULL' : ''}`);
});

// 外部キー制約の確認
const fkEnabled = db.isForeignKeysEnabled();
console.log(`外部キー制約: ${fkEnabled ? '有効' : '無効'}`);
```

## 高度な機能

SDK利用者が主に使用する機能です。

### 属性管理

メディアに任意のキー・バリューペアで拡張属性を付与できます：

```typescript
const mediaId = 1;

// 属性を設定（上書き）
db.setMediaAttribute(mediaId, 'rating', '5');
db.setMediaAttribute(mediaId, 'note', 'お気に入り', 'text');

// 属性を1件取得
const attr = db.getMediaAttribute(mediaId, 'rating');
if (attr) {
  console.log(`rating: ${attr.value}`);
}

// 全属性を取得
const attrs = db.getMediaAttributes(mediaId);
attrs.forEach(a => console.log(`${a.key}: ${a.value}`));

// 属性を削除
db.deleteMediaAttribute(mediaId, 'rating');

// 全属性を削除
db.deleteAllMediaAttributes(mediaId);
```

### コンテンツハッシュ操作

ファイル内容ベースの同定・重複検出を行います。

```typescript
// 特定作品のハッシュを計算・登録
const result = db.computeMediaHash(
  'item-uuid-here',
  '/path/to/media/file.mp3',
  'music',
  180 // durationSec（省略可）
);
if (result.skipped) {
  console.log('Skipped:', result.skip_reason);
} else {
  for (const hash of result.hashes) {
    console.log(`${hash.time_range}: ${Buffer.from(hash.content_hash).toString('hex')}`);
  }
}

// ハッシュ未計算の全作品を一括計算
const results = db.computeMediaHashes({}, undefined, false);
console.log(`Computed ${results.length} items`);

// SHA256で検索
const hashBytes = new Uint8Array(32); // SHA256ハッシュ値
const found = db.findByContentHash(hashBytes);

// 重複検出
const dupes = db.findDuplicateHashes();
for (const { content_hash, count } of dupes) {
  console.log(`Duplicate: ${Buffer.from(content_hash).toString('hex')} (${count} times)`);
}
```

### ファイル存在チェック（updateExist）

メディアの `path` に実ファイルが存在するかチェックし、`flag_exist` を更新します：

```typescript
// 全メディアを対象に実行
const result = db.updateExist({});
console.log(`対象: ${result.total}件, 更新: ${result.updated}件`);

// 特定フィルタで絞り込み
const result2 = db.updateExist({ media_type: 'comic', series: 'ワンピース' });
console.log(`絞り込み結果: 対象: ${result2.total}件, 更新: ${result2.updated}件`);

// dry_run: DBを更新せず結果のみ確認
const dryResult = db.updateExist({}, undefined, { dry_run: true });
// 変更があったIDはインライン（1000件以下）またはファイルで取得
if (dryResult.updated_ids) {
  console.log(`変更対象ID: ${dryResult.updated_ids.join(', ')}`);
} else if (dryResult.updated_ids_file) {
  console.log(`変更対象IDファイル: ${dryResult.updated_ids_file}`);
}
// 詳細はdetail_fileから取得
const items = JSON.parse(fs.readFileSync(dryResult.detail_file, 'utf-8'));
items.forEach((item: UpdateExistItemResult) => {
  if (item.flag_exist_before !== item.flag_exist_after) {
    console.log(`[${item.id}] ${item.title}: ${item.flag_exist_before} -> ${item.flag_exist_after}`);
  }
  if (item.found_extension) {
    console.log(`  代替拡張子: ${item.found_extension}`);
  }
  if (item.page_count_warning) {
    console.warn(`  警告: ${item.page_count_warning}`);
  }
});
```

**ファイル存在チェックのロジック:**
- `path` が NULL → `flag_exist = false`
- `media_type = comic`: `{path}/001.{ext}` が存在すれば `flag_exist = true`。存在しない場合はフォルダ内で代替拡張子を検索
- `media_type = video / music`: `path` のファイルが存在すれば `flag_exist = true`。存在しない場合は同ディレクトリ内で `{uuid}.{任意拡張子}` を検索
- 代替拡張子が見つかった場合は `extension` も自動更新（`dry_run = false` のとき）
- `comic` で `page_count = null` の場合、実ページ数を自動設定

### サムネイル操作（checkThumbnail / updateThumbnail）

#### checkThumbnail

メディアのサムネイル状態をチェックします（ファイル生成は行いません）：

```typescript
const result = db.checkThumbnail({});
console.log(`対象: ${result.total}件, OK: ${result.ok}件, 未生成: ${result.missing}件`);

result.details.forEach(item => {
  if (item.status.type !== 'ok') {
    console.log(`[${item.id}] ${item.title}: ${item.status.type}`);
  }
});
```

#### updateThumbnail

ImageMagick (`convert`) を使用してサムネイルを生成・更新します：

```typescript
// 全メディアのサムネイルを生成（既存はスキップ）
const result = db.updateThumbnail({});
console.log(`生成: ${result.generated}件, スキップ: ${result.skipped}件, エラー: ${result.errors}件`);

// dry_run: 生成せずに対象を確認
const dryResult = db.updateThumbnail({}, undefined, { dry_run: true });

// force: 既存サムネイルも再生成
const forceResult = db.updateThumbnail({ media_type: 'comic' }, undefined, { force: true });
```

**サムネイル生成のロジック:**
- `path` が未設定 → スキップ
- `path` に `content` コンポーネントが含まれない → スキップ
- `{path}/001.{ext}` が存在しない → スキップ
- サムネイルパス: `{content親}/cover/{uuid}.jpg`
- ImageMagick `convert` で高さ 180px 固定（縦横比維持）、品質 85

### 自動バックアップ

データベースの自動バックアップを設定できます：

```typescript
const db = new KijukuDB('./data/kijuku.db', {
  backup: {
    enabled: true,
    intervalMs: 3600000, // 1時間ごと（ミリ秒）
    backupDir: './backups',
  }
});

db.migrate();
// バックアップは自動的に実行されます
```

手動でバックアップを作成：

```typescript
await db.backup();
console.log('バックアップを作成しました');

// ラベル付きバックアップ
const path = await db.backupWithLabel('before_import');
if (path) {
  console.log(`バックアップ作成: ${path}`);
}
```

バックアップ一覧を取得：

```typescript
const backups = db.listBackups();
backups.forEach(b => console.log(`${b.name} [${b.scope}] (${b.createdAt.toISOString()})`));

// バックアップからリストア
import { BackupSelector } from 'kijuku-db';
const restoredPath = db.restore();  // 最新
const restoredPath2 = db.restore(BackupSelector.nth(1));  // 2番目に新しい
const restoredPath3 = db.restore(BackupSelector.withLabel('before_import'));

// BackupManagerに直接アクセス（高度な用途）
const manager = db.getBackupManager();
if (manager) {
  const info = manager.listBackups();
  console.log(`バックアップ数: ${info.length}`);
}
```

バックアップからデータを読み取る（読み取り専用）：

```typescript
import { BackupSelector } from 'kijuku-db';

// 最新バックアップからメディアを取得
const media = db.getMediaFromBackup(1);
if (media) {
  console.log(`バックアップから取得したメディア: ${media.title}`);
}
const results = db.findMediaFromBackup({ series: 'ワンピース' });
console.log(`バックアップからの検索結果: ${results.length}件`);

// タグをバックアップから取得
const tag = db.getTagByNameFromBackup('お気に入り');
if (tag) {
  console.log(`バックアップから取得したタグ: ${tag.name}`);
}
const allTags = db.getAllTagsFromBackup();
const mediaTags = db.getMediaTagsFromBackup(1);

// 属性をバックアップから取得
const attr = db.getMediaAttributeFromBackup(1, 'rating');
const attrs = db.getMediaAttributesFromBackup(1);
```

### 復元判断支援（差分・事後ラベル/メモ）

```typescript
// バックアップと現在DBの差分（復元判断）
//   added: 復元で復活 / removed: 復元で失われる / changed: 復元で上書き
const diff = db.diffWithBackup(BackupSelector.latest());
console.log(`media: +${diff.summary.media.added} -${diff.summary.media.removed} ~${diff.summary.media.changed}`);

// ID（タイムスタンプ）でバックアップを直接指定
const byId = db.diffWithBackup(BackupSelector.byId('20260707120000-000'));

// 既存バックアップにラベル/メモを事後付与（ファイル名は変更せず backup/meta/backup-meta.json に保存）
const id = db.listBackups()[0].id;
db.setBackupLabel(id, '重要');
db.setBackupNote(id, '作業前の状態');

// listBackups はサイドカー優先で label/note/labelSource を返す
for (const b of db.listBackups()) {
  console.log(`${b.id} label=${b.label ?? '-'} note=${b.note ?? '-'}`);
}
```

### リモートDB操作（SSH経由）

SSH経由でリモートサーバー上のデータベースを操作できます。

#### セットアップ

1. `.ssh/config`にリモートホスト設定を追加：

```
Host myserver
    HostName example.com
    User username
    Port 22
    IdentityFile ~/.ssh/id_rsa
```

2. リモートDBに接続：

```typescript
import { RemoteKijukuDB } from 'kijuku-db';

const remoteDb = new RemoteKijukuDB({
  sshHost: 'myserver',  // .ssh/configのHost名
  dbPath: '~/.local/share/kijuku/kijuku.db',
});

// ローカルDBと同じAPIで操作可能
const media = await remoteDb.createMedia({
  title: 'リモート作品',
  media_type: 'comic',
  artist: 'リモート作者',
});

const results = await remoteDb.findMedia({ media_type: 'comic' });
console.log(`検索結果: ${results.length}件`);

// バックアップ操作
const backupPath = await remoteDb.backup();
const backupPathWithLabel = await remoteDb.backup('before_import');
// タイムアウトを明示的に指定（例: 30分）
const backupPathWithTimeout = await remoteDb.backup('large_db', 30 * 60_000);

const backups = await remoteDb.listBackups();
backups.forEach(b => console.log(`${b.name} (${b.scope})`));

// 最新バックアップから復元
const restoredPath = await remoteDb.restore();
// N番目に新しいバックアップから復元
const restoredPath2 = await remoteDb.restore({ type: 'nth', n: 1 });
// タイムアウトを明示的に指定
const restoredPath3 = await remoteDb.restore({ type: 'latest' }, 30 * 60_000);

// プールされた SSH 接続を閉じる（未呼び出しだと TCP ソケットがイベントループを
// 保持し one-shot プロセスが終了しないため、スクリプト等では必須）
await remoteDb.disconnect();
```

**注意:**
- リモート操作は非同期（async/await）です
- 初回実行時、リモート側にバイナリが存在しない場合は自動的に転送されます

#### リモートDBでのバルク操作（重要）

リモートDBでは、**1操作ごとにリモート CLI プロセスが1回起動されます**（SSH 接続自体は接続プールで再利用されるため接続回数は増えません・後述）。個別操作をループで繰り返すと RPC 往復とリモートプロセス起動が繰り返されて大幅に遅くなるため、バルク操作の利用を推奨します。

```typescript
// ❌ 危険: 1000件 = 1000回のSSH呼び出し
for (const item of largeDataset) {
  await remoteDb.createMedia(item);  // SSHが1000回実行される
}

// ✅ 推奨: 1回のSSH呼び出しで完結
await remoteDb.bulkCreateMedia(largeDataset);
```

バルク操作は `RemoteKijukuDB` でも同じAPIで使えます：

```typescript
// 一括作成
const created = await remoteDb.bulkCreateMedia([
  { title: '作品1', media_type: 'comic' },
  { title: '作品2', media_type: 'comic' },
  // ... 何千件でも1回のSSH呼び出し
]);

// 一括更新
await remoteDb.bulkUpdateMedia([
  { id: 1, data: { artist: '新しい作者名' } },
  { id: 2, data: { series: '新しいシリーズ名' } },
]);

// 一括削除
await remoteDb.bulkDeleteMedia([1, 2, 3, 4, 5]);

// 検索結果を一括削除する場合
const oldMedia = await remoteDb.findMedia({ source: 'deprecated' });
await remoteDb.bulkDeleteMedia(oldMedia.map(m => m.id));
```

#### リモートDBでサポートされる全メソッド

RemoteKijukuDBはKijukuDBと同等の全メソッドを`Promise`で提供します：

- メディアCRUD / 検索 / バルク操作
- タグ操作 / 属性操作
- `updateExist()` / `checkThumbnail()` / `updateThumbnail()`
- `getSchemaVersion()` / `getServerVersion()` / `getTables()` / `getTableInfo()`

**リモート CLI の自動デプロイ（TASK-69）:** 全ての RPC の先頭でリモート `kijuku-cli` のバージョン（`getServerVersion`）を取得し、ローカル（クライアント）より古い場合に自動デプロイします（`local > remote` の厳密大なり・ダウングレード保護・同等なら skip）。デプロイ先は `deploy-local.sh` と同じ実体 `~/.local/kijuku-db/bin/kijuku-cli` + symlink `~/.local/bin/kijuku-cli` 構成。リモートが未存在・または TASK-69 前の古いバイナリ（`getServerVersion` 未対応）でも自動デプロイで回復します。

**SSH Session の接続プール（TASK-71）:** RPC ごとに新規 SSH 接続を張るのではなく、初回 RPC で確立した接続をキャッシュして再利用します（連続 RPC のレイテンシ改善・再 handshake 省略・Rust SDK と parity）。セッション系エラー（`SshSessionError`）時は自動的に slot を無効化して次回 RPC で再接続します。使い終わったら `await remoteDb.disconnect()` を呼んで接続を閉じてください。未呼び出しの場合、プールされた TCP ソケットが Node.js のイベントループを保持しプロセスが終了しなくなるため、スクリプト等の one-shot プロセスでは必須です（長期稼働サーバー等で接続を維持したい場合を除く）。`remoteDb.connectCount` で新規接続回数を確認できます（診断用・連続 RPC で `1` のままなら再利用を示す）。

### TOML設定ファイル（config）

バックアップ設定等をTOMLファイルで管理できます：

```typescript
import { loadConfig, globalConfigPath } from 'kijuku-db';

// 設定を読み込み（dbPath基準でconfig.tomlを検索）
const config = loadConfig('./data/kijuku.db');
console.log('バックアップ設定:', config.backup);

// グローバル設定パス
console.log('グローバル設定:', globalConfigPath());
```

設定ファイルの優先順位:
1. `{dbPathと同じディレクトリ}/config.toml`
2. `~/.config/kijuku/config.toml`（グローバル）

### カスタムエラークラス

SDKは型安全なエラークラスを提供しています：

```typescript
import {
  KijukuDBError,
  DatabaseError,
  ValidationError,
  NotFoundError,
  ConflictError,
} from 'kijuku-db';

try {
  db.createMedia({ title: 'テスト', media_type: 'comic', path: '/duplicate' });
} catch (error) {
  if (error instanceof ConflictError) {
    console.error('データが重複しています');
  } else if (error instanceof ValidationError) {
    console.error('バリデーションエラー:', error.message);
  } else if (error instanceof DatabaseError) {
    console.error('データベースエラー:', error.message);
  }
}
```

---

### Web GUIサーバー（特殊ケース）

**通常はCLIツールを使用してください：**
```bash
kijuku-cli server --db ./data/kijuku.db --port 40001
```

既存アプリケーションに組み込む必要がある場合のみ、プログラムから起動できます：

```typescript
import { KijukuDB, startServer } from 'kijuku-db';

const db = new KijukuDB('./data/kijuku.db');
db.migrate();

startServer(db, { port: 40001 });
```

## TypeScript型定義

kijuku-dbは完全な型定義を提供しています：

```typescript
import type {
  // コア型
  Media,
  MediaInput,
  MediaFilter,
  QueryOptions,
  SortKey,
  BulkUpdateItem,
  Tag,
  TagUsageStats,
  MediaAttribute,
  DBOptions,
  // バックアップ型
  BackupInfo,
  BackupScope,
  BackupKind,
  BackupOptions,
  // サムネイル型
  ThumbnailOptions,
  CheckThumbnailResult,
  UpdateThumbnailResult,
  // ファイル存在チェック型
  UpdateExistOptions,
  UpdateExistResult,
  UpdateExistItemResult,
  // ハッシュ型
  MediaHash,
  MediaHashInput,
  ComputeHashResult,
  hexToBytes,
  bytesToHex,
  // DB情報型
  TableColumnInfo,
  // リモート型
  RemoteConfig,
  RemoteBackupSelector,
} from 'kijuku-db';

// 型安全な関数
function processMedia(media: Media): void {
  console.log(media.title); // 型補完が効く
}

// フィルタを型安全に構築
const filter: MediaFilter = {
  media_type: 'comic',
  series: 'ワンピース',
  tag_ids: [1, 2],       // タグIDで絞り込む場合
  id_in: [1, 2, 3],      // 複数IDを一括取得（999件超は自動チャンク分割）
};

const options: QueryOptions = {
  sortKeys: [{ field: 'created_at', order: 'DESC' }],
  limit: 10,
};
```

## 環境変数

以下の環境変数で動作をカスタマイズできます：

```bash
# データベースファイルのパス（デフォルト値）
DATABASE_PATH=./data/kijuku.db

# クエリログ出力の有効化
KIJUKU_DB_VERBOSE=true
```

`.env`ファイルを使用する場合は、`dotenv`パッケージと組み合わせて使用してください：

```typescript
import 'dotenv/config';
import { KijukuDB } from 'kijuku-db';

const db = new KijukuDB(process.env.DATABASE_PATH || './data/kijuku.db');
```

> **注意:** リモートDB設定は環境変数ではなく`RemoteConfig`オブジェクトで指定します。

## パフォーマンス最適化

### バルク操作

大量のメディアを一括で操作する場合は、バルク操作メソッドを使用してください。

#### 一括作成

```typescript
const mediaList = [
  { title: 'メディア1', media_type: 'comic' as const },
  { title: 'メディア2', media_type: 'comic' as const },
  // ... 大量のデータ
];

const createdMedia = db.bulkCreateMedia(mediaList);
console.log(`${createdMedia.length}件のメディアを作成しました`);
```

#### 一括削除

```typescript
// 複数のメディアを一括削除
db.bulkDeleteMedia([1, 2, 3]);

// 検索結果を一括削除
const oldMedia = db.findMedia({ source: 'deprecated' });
db.bulkDeleteMedia(oldMedia.map(m => m.id));
```

#### 一括更新

```typescript
// 複数のメディアを一括更新
db.bulkUpdateMedia([
  { id: 1, data: { artist: '新しい作者名' } },
  { id: 2, data: { series: '新しいシリーズ名' } },
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

### トランザクションの活用

複数の操作をまとめて実行する場合は、トランザクションを使用してパフォーマンスを向上させてください：

```typescript
db.transaction(() => {
  for (const item of largeDataset) {
    db.createMedia(item);
  }
});
```

## エラーハンドリング

```typescript
try {
  const media = db.createMedia({
    title: 'サンプル',
    media_type: 'comic',
  });
} catch (error) {
  if (error.message.includes('UNIQUE constraint failed')) {
    console.error('重複したメディアです');
  } else {
    console.error('エラーが発生しました:', error.message);
  }
}
```

## サンプルコード

より詳細なサンプルコードは以下を参照してください：

- [はじめに](../../../examples/ts-sdk/01-getting-started.md)
- [基本的なCRUD操作](../../../examples/ts-sdk/02-basic-crud.md)
- [検索とフィルタリング](../../../examples/ts-sdk/03-search-and-filter.md)
- [タグ管理](../../../examples/ts-sdk/04-tag-management.md)
- [バルク操作](../../../examples/ts-sdk/05-bulk-operations.md)

## トラブルシューティング

### better-sqlite3のビルドエラー

```
Error: Cannot find module 'better-sqlite3'
```

better-sqlite3はネイティブモジュールのため、プラットフォームに応じたビルドが必要です：

```bash
npm install better-sqlite3
# または
npm rebuild better-sqlite3
```

### WSL環境での注意

WSL環境で使用する場合、Windowsファイルシステム（/mnt/c/など）ではなく、Linux側のファイルシステムにデータベースを配置してください。

## 関連ドキュメント

- [API仕様書](../../../api.md) - 全メソッドの詳細仕様
- [サンプルコード](../../../../examples/README.md) - 実行可能なサンプル集
- [パフォーマンスガイド](../../../PERFORMANCE.md) - ベンチマークと最適化
