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
  volume_number: 1,
  path: '/media/comics/onepiece_v01.cbz',
});

console.log(`メディアID: ${media.id}`);
```

### 3. メディアの検索

```typescript
// シリーズで検索
const results = db.findMedia(
  { series: 'ワンピース' },
  { orderBy: 'volume_number', order: 'ASC' }
);

console.log(`見つかったメディア: ${results.length}件`);
results.forEach(m => {
  console.log(`- ${m.title}`);
});
```

### 4. タグの管理

```typescript
// タグを作成
const tag = db.createTag('お気に入り');

// メディアにタグを追加
db.addTagToMedia(media.id, tag.id);

// メディアのタグを取得
const tags = db.getMediaTags(media.id);
console.log('タグ:', tags.map(t => t.name).join(', '));
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

## 高度な機能

SDK利用者が主に使用する機能です。

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
console.log(`バックアップを作成しました`);
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
```

**注意:**
- リモート操作は非同期（async/await）です
- 初回実行時、リモート側にバイナリが存在しない場合は自動的に転送されます

### Web GUIサーバー（特殊ケース）

**通常はCLIツールを使用してください：**
```bash
kijuku-cli server --db ./data/kijuku.db --port 40001
```

既存アプリケーションに組み込む必要がある場合のみ、プログラムから起動できます：

```typescript
import { startServer } from 'kijuku-db';

const server = await startServer({
  dbPath: './data/kijuku.db',
  port: 40001,
});
```

## TypeScript型定義

kijuku-dbは完全な型定義を提供しています：

```typescript
import type {
  Media,
  MediaInput,
  MediaFilter,
  QueryOptions,
  Tag,
  MediaAttribute,
} from 'kijuku-db';

// 型安全な関数
function processMedia(media: Media): void {
  console.log(media.title); // 型補完が効く
}

// フィルタを型安全に構築
const filter: MediaFilter = {
  media_type: 'comic',
  series: 'ワンピース',
};

const options: QueryOptions = {
  orderBy: 'created_at',
  order: 'DESC',
  limit: 10,
};
```

## 環境変数

以下の環境変数で動作をカスタマイズできます：

```bash
# データベースファイルのパス（デフォルト値）
DATABASE_PATH=./data/kijuku.db

# クエリタイムアウト（ミリ秒）
KIJUKU_DB_TIMEOUT=5000

# SQLログ出力の有効化
KIJUKU_DB_VERBOSE=true

# リモートDB設定
REMOTE_SSH_HOST=myserver
REMOTE_DB_PATH=~/.local/share/kijuku/kijuku.db
REMOTE_BINARY_PATH=~/.local/bin/kijuku-cli
```

`.env`ファイルを使用する場合は、`dotenv`パッケージと組み合わせて使用してください：

```typescript
import 'dotenv/config';
import { KijukuDB } from 'kijuku-db';

const db = new KijukuDB(process.env.DATABASE_PATH || './data/kijuku.db');
```

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

- [基本的なCRUD操作](../../../../examples/01-basic-crud.ts)
- [検索とフィルタリング](../../../../examples/02-search-filter.ts)
- [バルク操作](../../../../examples/03-bulk-operations.ts)
- [リモート操作](../../../../examples/04-remote-operations.ts)

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
