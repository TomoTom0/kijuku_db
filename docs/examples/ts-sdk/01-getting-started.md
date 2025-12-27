# Getting Started - 最初のステップ

kijuku-db TypeScript SDKを使い始めるための最も基本的なガイドです。

## インストール

### 外部プロジェクトから使用する場合

```bash
# npmの場合
npm install kijuku-db

# Bunの場合
bun add kijuku-db
```

### このリポジトリで開発する場合

```bash
cd ts-sdk
bun install
bun run build
```

## 最も簡単な例

```typescript
import { KijukuDB } from 'kijuku-db';

// 1. データベースを開く
const db = new KijukuDB('./data/myapp.db');

// 2. マイグレーション（初回のみ必要）
db.migrate();

// 3. メディアを作成
const media = db.createMedia({
  title: '進撃の巨人 1巻',
  media_type: 'comic',
  artist: '諫山創',
});

console.log(`作成されました: ID=${media.id}`);

// 4. メディアを検索
const results = db.findMedia({ media_type: 'comic' });
console.log(`コミックが ${results.length} 件見つかりました`);

// 5. 接続を閉じる
db.close();
```

## ステップバイステップ解説

### Step 1: データベースを開く

```typescript
const db = new KijukuDB('./data/myapp.db');
```

- ファイルが存在しない場合は自動的に作成されます
- 相対パスまたは絶対パスを指定できます
- `.db`拡張子が推奨されますが必須ではありません

### Step 2: マイグレーション

```typescript
db.migrate();
```

- **初回のみ必要**な操作です
- データベースのテーブルを作成します
- 既にマイグレーション済みの場合は何もしません（冪等性があります）

### Step 3: データを作成

```typescript
const media = db.createMedia({
  title: '進撃の巨人 1巻',        // 必須
  media_type: 'comic',             // 必須: 'comic' | 'video' | 'music'
  artist: '諫山創',                // オプション
});
```

**必須フィールド:**
- `title` - 作品のタイトル
- `media_type` - メディアタイプ（`'comic'`, `'video'`, `'music'`のいずれか）

**よく使うオプションフィールド:**
- `artist` - 作者名
- `series` - シリーズ名
- `volume_text` - 巻数（文字列）
- `page_count` - ページ数
- `path` - ファイルパス
- `thumbnail_path` - サムネイル画像パス

### Step 4: データを検索

```typescript
const results = db.findMedia({ media_type: 'comic' });
```

- 条件に一致するすべてのメディアが返されます
- 空のオブジェクト`{}`を渡すとすべてのメディアが返されます

### Step 5: 接続を閉じる

```typescript
db.close();
```

- アプリケーション終了時に必ず呼んでください
- リソースを適切に解放します

## よくある使い方

### 複数の条件で検索

```typescript
const results = db.findMedia({
  media_type: 'comic',
  artist: '諫山創',
  series: '進撃の巨人',
});
```

### メディアタイプの違い

```typescript
// コミック
db.createMedia({
  title: 'ワンピース 1巻',
  media_type: 'comic',
  artist: '尾田栄一郎',
  page_count: 200,
});

// ビデオ
db.createMedia({
  title: '君の名は。',
  media_type: 'video',
  artist: '新海誠',
  duration_sec: 6360, // 106分 = 6360秒
});

// 音楽
db.createMedia({
  title: '前前前世',
  media_type: 'music',
  artist: 'RADWIMPS',
  duration_sec: 285, // 4分45秒 = 285秒
});
```

## 設定オプション

データベース接続時にオプションを指定できます：

```typescript
const db = new KijukuDB('./data/myapp.db', {
  timeout: 5000,      // タイムアウト（ミリ秒）
  readonly: false,    // 読み取り専用モード
  verbose: false,     // SQLログを出力
});
```

## 環境変数

`.env`ファイルで設定を管理できます：

```bash
DATABASE_PATH=./data/myapp.db
KIJUKU_DB_TIMEOUT=5000
KIJUKU_DB_VERBOSE=false
```

```typescript
// 環境変数から自動的に読み込まれます
const db = new KijukuDB(process.env.DATABASE_PATH!);
```

## 次のステップ

基本的な使い方を理解したら、以下のサンプルに進んでください：

- [02-basic-crud.md](./02-basic-crud.md) - CRUD操作の詳細
- [03-search-and-filter.md](./03-search-and-filter.md) - 検索の詳細
- [04-tag-management.md](./04-tag-management.md) - タグの使い方

## トラブルシューティング

### エラー: `Cannot find module 'kijuku-db'`

SDKがインストールされていません：

```bash
npm install kijuku-db
```

### エラー: `SQLITE_ERROR: no such table: media`

マイグレーションが実行されていません：

```typescript
db.migrate(); // これを追加
```

### エラー: `media_type must be one of: comic, video, music`

`media_type`に不正な値が指定されています：

```typescript
// ✅ 正しい
media_type: 'comic'

// ❌ 間違い
media_type: 'book'  // サポートされていません
```
