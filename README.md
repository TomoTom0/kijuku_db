# kijuku-db

メディア（コミック、ビデオ、音楽）のメタデータを管理するためのSQLiteデータベースライブラリ

## 概要

kijuku-dbは、メディアコンテンツのメタデータを効率的に管理するためのデータベースライブラリです。SQLiteをバックエンドとして使用し、TypeScriptとRustのSDKを提供します。

### 特徴

- コミック、ビデオ、音楽の3種類のメディアタイプに対応
- 柔軟なタグ管理システム
- EAVモデルによる拡張可能な追加属性
- トランザクション対応
- バルク操作サポート
- 高度な検索・フィルタリング機能
- **時間間隔ベースの自動バックアップ機能**
- **認証付きWeb GUIサーバー（メディア閲覧・検索）**
- CLIツール付属
- SSH経由でのリモートDB操作をサポート
- 自動バイナリデプロイ機能
- TypeScript SDK と Rust SDK の両方を提供

## インストール

### 前提条件

**TypeScript SDK:**
- Node.js 18以上
- Bun（推奨）またはnpm

**Rust SDK:**
- Rust 1.70以上
- Cargo

### TypeScript SDK

```bash
cd ts-sdk
bun install
```

npmを使用する場合：

```bash
cd ts-sdk
npm install
```

#### ビルド

```bash
bun run build
```

これにより`dist/`ディレクトリに以下のファイルが生成されます：
- `index.js` - メインライブラリ
- `index.d.ts` - TypeScript型定義
- `cli.js` - CLIツール

### Rust SDK

```bash
cd rust-sdk
cargo build --release
```

これにより`target/release/`ディレクトリにバイナリが生成されます：
- `kijuku-cli` - CLIツール（ライブラリ機能を含む）

**使用例:**
```bash
# SDK利用ガイドを表示
./target/release/kijuku-cli docs rust

# Web GUIサーバー起動
./target/release/kijuku-cli --db ./data/kijuku.db server --port 40001
```

**注意:** Rust CLIは主にSSH経由で使用されることを想定しています。直接操作する場合はTypeScript CLIを推奨します。

## クイックスタート

> **外部プロジェクトからSDKとして利用する場合は、[SDK利用ガイド](docs/usage/sdk/README.md)を参照してください。**

### ライブラリとして使用（このリポジトリ内で開発する場合）

```typescript
import { KijukuDB } from 'kijuku-db';

// データベースを初期化
const db = new KijukuDB('./data/kijuku.db');

// マイグレーション実行
db.migrate();

// メディアを作成
const media = db.createMedia({
  title: 'サンプルコミック',
  media_type: 'comic',
  artist: '作者名',
  series: 'シリーズ名',
  path: '/path/to/comic.cbz',
});

console.log(`作成されたメディアID: ${media.id}`);

// メディアを検索
const results = db.findMedia(
  { media_type: 'comic', series: 'シリーズ名' },
  { orderBy: 'created_at', order: 'DESC', limit: 10 }
);

console.log(`検索結果: ${results.length}件`);

// タグを作成して付与
const tag = db.createTag('お気に入り');
db.addTagToMedia(media.id, tag.id);

// トランザクション
db.transaction(() => {
  db.createMedia({ title: 'メディア1', media_type: 'comic' });
  db.createMedia({ title: 'メディア2', media_type: 'video' });
});

// 接続を閉じる
db.close();
```

### CLIツールとして使用

```bash
# マイグレーション実行
kijuku-cli migrate --db ./data/kijuku.db

# メディア検索
kijuku-cli search --title "コミック" --type comic --db ./data/kijuku.db

# JSONファイルからインポート
kijuku-cli import --file data.json --db ./data/kijuku.db

# CSVファイルからインポート
kijuku-cli import --file data.csv --db ./data/kijuku.db

# SDK利用ガイドを表示
kijuku-cli docs          # 概要
kijuku-cli docs ts       # TypeScript SDK
kijuku-cli docs rust     # Rust SDK
kijuku-cli docs api      # API仕様書

# ヘルプ表示
kijuku-cli help
```

## API仕様

### KijukuDBクラス

#### コンストラクタ

```typescript
constructor(dbPath: string, options?: DBOptions)
```

**パラメータ:**
- `dbPath`: データベースファイルのパス
- `options`: オプション設定
  - `timeout`: クエリタイムアウト（ミリ秒、デフォルト: 5000）
  - `readonly`: 読み取り専用モード（デフォルト: false）
  - `verbose`: SQLログ出力（デフォルト: false）

**環境変数:**
- `DATABASE_PATH`: データベースファイルのパス（デフォルト値として使用）
- `KIJUKU_DB_TIMEOUT`: タイムアウト時間
- `KIJUKU_DB_VERBOSE`: ログ出力の有効化

#### マイグレーション

```typescript
migrate(): void
getSchemaVersion(): number
```

- `migrate()`: データベーススキーマを初期化・更新
- `getSchemaVersion()`: 現在のスキーマバージョンを取得

#### メディア操作

```typescript
createMedia(data: MediaInput): Media
getMedia(id: number): Media | null
updateMedia(id: number, data: Partial<MediaInput>): void
deleteMedia(id: number): void
findMedia(filter: MediaFilter, options?: QueryOptions): Media[]
bulkCreateMedia(dataList: MediaInput[]): Media[]
```

**MediaInput型:**
```typescript
interface MediaInput {
  title: string;              // 必須
  media_type: 'comic' | 'video' | 'music';  // 必須
  title_id?: string;
  path?: string;
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
  // その他のオプションフィールド
}
```

**MediaFilter型:**
```typescript
interface MediaFilter {
  title?: string;
  title_id?: string;
  artist?: string;
  artist_id?: string;
  media_type?: 'comic' | 'video' | 'music';
  series?: string;
  source?: string;
  tag_ids?: number[];
}
```

**QueryOptions型:**
```typescript
interface QueryOptions {
  orderBy?: string;      // 'created_at', 'title', 'artist' など
  order?: 'ASC' | 'DESC';
  limit?: number;
  offset?: number;
}
```

#### タグ操作

```typescript
createTag(name: string): Tag
getTagByName(name: string): Tag | null
getAllTags(): Tag[]
addTagToMedia(mediaId: number, tagId: number): void
removeTagFromMedia(mediaId: number, tagId: number): void
getMediaTags(mediaId: number): Tag[]
```

**Tag型:**
```typescript
interface Tag {
  id: number;
  name: string;
}
```

#### トランザクション

```typescript
transaction<T>(fn: () => T): T
```

トランザクション内で複数の操作をアトミックに実行します。

**例:**
```typescript
db.transaction(() => {
  const media1 = db.createMedia({ title: 'メディア1', media_type: 'comic' });
  const tag = db.createTag('新着');
  db.addTagToMedia(media1.id, tag.id);
});
```

#### その他

```typescript
close(): void
```

データベース接続を閉じます。

## CLIツールの使い方

### コマンド一覧

| コマンド | 説明 |
|---------|------|
| `migrate` | データベースのマイグレーションを実行 |
| `search` | メディアを検索 |
| `import` | JSON/CSV/TSVファイルからメディアをインポート |
| `server` | Web GUIサーバーを起動（認証付き） |
| `help` | ヘルプを表示 |

### 共通オプション

- `--db <path>`: データベースファイルのパス（デフォルト: `./kijuku.db`）

### migrateコマンド

データベースを初期化し、スキーマを作成します。

```bash
kijuku-cli migrate --db ./data/kijuku.db
```

### searchコマンド

メディアを検索します。

```bash
kijuku-cli search [options] --db <path>
```

**オプション:**
- `--title <text>`: タイトルで検索
- `--artist <text>`: 作者で検索
- `--type <type>`: メディアタイプで検索（`comic`, `video`, `music`）
- `--series <text>`: シリーズで検索
- `--source <text>`: データソースで検索
- `--limit <n>`: 結果の最大件数
- `--offset <n>`: 結果のオフセット
- `--orderBy <field>`: ソートフィールド
- `--order <ASC|DESC>`: ソート順

**例:**
```bash
# タイトルで検索
kijuku-cli search --title "ワンピース" --db ./data/kijuku.db

# コミックタイプで最新10件を取得
kijuku-cli search --type comic --orderBy created_at --order DESC --limit 10 --db ./data/kijuku.db

# シリーズで検索
kijuku-cli search --series "ドラゴンボール" --db ./data/kijuku.db
```

### importコマンド

JSON、CSV、TSVファイルからメディアデータを一括インポートします。

```bash
kijuku-cli import --file <path> --db <path>
```

**オプション:**
- `--file <path>`: インポートするファイルのパス（必須）

**JSON形式の例:**
```json
[
  {
    "title": "サンプルコミック1",
    "media_type": "comic",
    "artist": "作者A",
    "series": "シリーズ1",
    "path": "/path/to/comic1.cbz"
  },
  {
    "title": "サンプルコミック2",
    "media_type": "comic",
    "artist": "作者B",
    "path": "/path/to/comic2.cbz"
  }
]
```

**CSV形式の例:**
```csv
title,media_type,artist,series,path
サンプルコミック1,comic,作者A,シリーズ1,/path/to/comic1.cbz
サンプルコミック2,comic,作者B,,/path/to/comic2.cbz
```

**インポート例:**
```bash
# JSONファイルからインポート
kijuku-cli import --file ./data/media.json --db ./data/kijuku.db

# CSVファイルからインポート
kijuku-cli import --file ./data/media.csv --db ./data/kijuku.db
```

### serverコマンド

認証付きWeb GUIサーバーを起動します。ブラウザでメディアの閲覧・検索ができます。

```bash
kijuku-cli server --db <path> [options]
```

**オプション:**
- `--port <number>`: サーバーのポート番号（デフォルト: 40001）
- `--password <text>`: 認証パスワード（省略時は自動生成）

**例:**
```bash
# デフォルト設定で起動（パスワードは自動生成）
kijuku-cli server --db ./data/kijuku.db

# ポートとパスワードを指定して起動
kijuku-cli server --db ./data/kijuku.db --port 8080 --password mypassword
```

起動すると以下のような情報が表示されます：

```
Kijuku DB Web GUI Server
========================
URL: http://localhost:40001
Password: Ab12Cd34Ef56

Press Ctrl+C to stop the server
```

ブラウザで表示されたURLにアクセスし、パスワードを入力してログインします。

**機能:**
- メディア一覧の表示（ページネーション対応）
- タイトル・作者・シリーズ・メディアタイプでの検索
- メディア詳細の表示（タグ、追加属性を含む）
- レスポンシブデザイン（モバイル対応）

**注意事項:**
- serverコマンドはローカルDBのみサポート（リモートDB非対応）
- Ctrl+Cでサーバーを停止できます

## データベーススキーマ

### テーブル構成

- `media`: メディア情報の本体
- `tags`: タグ定義
- `media_tags`: メディアとタグの多対多リレーション
- `media_attributes`: 追加属性（EAVモデル）
- `schema_version`: スキーマバージョン管理

詳細なスキーマ定義は `schema/schema.sql` を参照してください。

### インデックス

以下のフィールドにインデックスが作成されます：
- `title_id`, `artist_id` (完全一致検索用)
- `media_type` (メディアタイプフィルタ)
- `series` (シリーズ検索)
- `source` (データソース検索)
- `media_type, created_at` (複合インデックス)
- `media_tags` の `tag_id`, `media_id`

## 開発環境のセットアップ

### リポジトリのクローン

```bash
git clone <repository-url>
cd kijuku_db
```

### TypeScript SDKの開発

```bash
cd ts-sdk
bun install
```

### ビルド

```bash
bun run build
```

### テスト実行

```bash
# 全テストを実行（リモートテストを除く）
bun run test:all

# カテゴリ別にテストを実行
bun run test:unit          # 単体テストのみ
bun run test:integration   # 結合テストのみ
bun run test:e2e           # E2Eテスト（ローカル）のみ

# テストをウォッチモードで実行
bun run test:watch
```

詳細なテスト方針とガイドラインについては [TESTING.md](./TESTING.md) を参照してください。

**テストの分類:**
- **単体テスト**: `test/unit/` - 個々の関数のテスト（モック使用）
- **結合テスト**: `test/integration/` - 複数コンポーネントの連携テスト
- **E2Eテスト**: `test/e2e/` - 実際のCLI実行テスト

### 開発モード

```bash
bun run dev
```

## プロジェクト構成

```
kijuku_db/
├── rust-sdk/          # Rust SDK
├── ts-sdk/            # TypeScript SDK
│   ├── src/           # ソースコード
│   │   ├── index.ts   # メインエントリポイント
│   │   ├── cli.ts     # CLIツール
│   │   ├── types.ts   # 型定義
│   │   ├── migration.ts  # マイグレーション機能
│   │   ├── crud.ts    # CRUD操作
│   │   ├── search.ts  # 検索機能
│   │   ├── tag.ts     # タグ管理
│   │   ├── bulk.ts    # バルク操作
│   │   └── __tests__/ # テストコード
│   ├── dist/          # ビルド成果物
│   └── package.json
├── schema/            # データベーススキーマ（DDL）
│   └── schema.sql
├── data/              # データベースファイル保存先
└── docs/              # ドキュメント
    └── design/        # 設計ドキュメント
```

## 設計方針

詳細な設計方針については `docs/design/decisions.md` を参照してください。

主な設計決定：
- モノレポ構成（スキーマ定義を共有）
- SDK実装はRustとTypeScriptで独立
- TypeScript SDK優先で実装（API設計を早く固める）
- EAVモデルによる柔軟な追加属性管理
- NAS上のNode.js環境での実行を想定

## リモートDB操作

SSH経由でリモートサーバー上のデータベースを操作できます。

### セットアップ

1. Rustバイナリをビルドしてローカルに配置

```bash
cd ts-sdk
bun run deploy:local
```

2. `.ssh/config`にリモートホスト設定を追加

```
Host myserver
    HostName example.com
    User username
    Port 22
    IdentityFile ~/.ssh/id_rsa
```

3. `.env`ファイルに設定を追加

```bash
REMOTE_SSH_HOST=myserver
REMOTE_DB_PATH=~/.local/share/kijuku/kijuku.db
```

### 使用例

```typescript
import { RemoteKijukuDB } from 'kijuku-db';

// リモートDB接続を作成
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

### 設定オプション

```typescript
interface RemoteConfig {
  sshHost: string;      // .ssh/configのHost名（必須）
  dbPath?: string;      // リモートのDBパス（デフォルト: ~/.local/share/kijuku/kijuku.db）
  workDir?: string;     // 作業ディレクトリ（省略可、将来の拡張用）
  binaryPath?: string;  // バイナリパス（デフォルト: ~/.local/bin/kijuku-cli）
}
```

### 自動デプロイ機能

初回実行時、リモート側にバイナリが存在しない場合は自動的に転送されます。

**バイナリの配置場所:**
- `binaryPath`を指定した場合: 指定されたパス
- `binaryPath`未指定の場合: `~/.local/bin/kijuku-cli`（デフォルト）

### サンプルコード

詳細な使用例は `examples/04-remote-operations.ts` を参照してください。

## 実行環境の注意事項

このSDKは **NAS上のNode.js環境での実行を想定** しています。

ネットワークファイルシステム越しのSQLiteアクセスは以下の理由により推奨されません：
- ファイルロック機構が正しく動作しない可能性
- データ破損のリスク

PC側から利用する場合は、上記の「リモートDB操作」機能を使用してSSH経由でアクセスしてください。

## ドキュメント

### SDK利用ガイド（外部プロジェクトから使用する場合）

- **[SDK利用ガイド（概要）](docs/usage/sdk/README.md)** - TypeScript/Rust SDK選択ガイド
  - [TypeScript SDK利用ガイド](docs/usage/sdk/ts/README.md) - インストール、基本的な使い方、高度な機能
  - [Rust SDK利用ガイド](docs/usage/sdk/rust/README.md) - インストール、基本的な使い方

### 開発者向けドキュメント

- [データベースセットアップガイド](docs/DATABASE_SETUP.md) - DBの作成とデータインポート手順
- [テストガイド](docs/TESTING.md) - テスト実行方法
- [パフォーマンスガイド](docs/PERFORMANCE.md) - パフォーマンステストとベンチマーク
- [API仕様書](docs/api.md) - 詳細なAPI仕様
- [リモートテスト手順](docs/manual-testing-remote.md) - リモート環境でのテスト方法

## ライセンス

MIT

## 今後の開発予定

- バックアップファイルの自動削除・間引き機能
- パフォーマンス最適化
- エラーハンドリングの強化
- 実アプリケーションとの統合サンプル
- API仕様書の詳細化
- Web GUIのHTTPS対応
- セッション永続化機能

## 貢献

プライベートプロジェクトのため、外部からの貢献は受け付けていません。
