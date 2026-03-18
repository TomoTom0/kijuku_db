# 設計決定事項

## プロジェクト構成

### リポジトリ構成
**決定**: モノレポ

```
kijuku_db/
├── rust-sdk/          # Rust SDK
├── ts-sdk/            # TypeScript SDK
├── schema/            # データベーススキーマ（DDL）
├── data/              # データベースファイル保存先
└── docs/              # ドキュメント
```

**理由**:
- スキーマ定義を共有しやすい
- バージョン管理が一元化される

### SDK実装方針
**決定**: Rust SDK、TypeScript SDKは完全独立実装

**理由**:
- この規模ではビルド環境をシンプルに保つ方が開発効率が良い
- CRUD中心のロジックなので二重実装の負担は小さい
- 各言語のエコシステムを最大限活用できる
- 将来的にパフォーマンスが問題になったら部分的にRustバインディングを検討

## SDK機能要件

### コア機能
1. **CRUD操作**
   - create, read, update, delete

2. **バルク操作**
   - 配列・辞書をまとめて受け取って登録

3. **フィルタ・ソート**
   - タイトル、作者、シリーズ、ソース、タグでの検索
   - 各カラムでソート
   - ページネーション（オフセット/リミット）

4. **トランザクション管理**
   - 複数操作をアトミックに実行

5. **エラーハンドリング**
   - 標準的なエラー型定義
   - エラーの分類（DB接続エラー、バリデーションエラー等）

6. **マイグレーション機能**
   - スキーマ初期化
   - スキーマバージョン管理
   - マイグレーション実行

### SDK境界
SDK外（アプリケーション層）で実装すべき機能：
- ファイルスキャン・ウォッチャー
- サムネイル生成
- メタデータ自動取得（外部API連携）
- flag_exist自動更新

### 補助ツール
当面は不要。必要に応じて後で対応：
- データ変換・検証系（バリデーション、インポート/エクスポート、重複検出）

## データベース設計

### 追加属性管理
**決定**: EAVモデル

```sql
CREATE TABLE media_attributes (
  media_id INTEGER NOT NULL,
  key TEXT NOT NULL,
  value TEXT,
  value_type TEXT, -- 'string', 'integer', 'boolean'
  PRIMARY KEY (media_id, key),
  FOREIGN KEY (media_id) REFERENCES media(id)
);
```

**理由**:
- 特定の属性でフィルタリングしやすい
- 値の型を管理できる
- よく使う属性が判明したら、mediaテーブルに正式カラムとして昇格可能

### インデックス設計

```sql
-- ID検索用（完全一致）
CREATE INDEX idx_media_title_id ON media(title_id);
CREATE INDEX idx_media_artist_id ON media(artist_id);

-- メディアタイプフィルタ
CREATE INDEX idx_media_media_type ON media(media_type);

-- シリーズ検索
CREATE INDEX idx_media_series ON media(series);

-- データソース検索
CREATE INDEX idx_media_source ON media(source);

-- 複合インデックス（メディアタイプ×作成日時）
CREATE INDEX idx_media_type_created ON media(media_type, created_at DESC);

-- タグ検索用
CREATE INDEX idx_media_tags_tag_id ON media_tags(tag_id);
CREATE INDEX idx_media_tags_media_id ON media_tags(media_id);
```

**方針**:
- 読み仮名検索は当面インデックスなし
- 追加属性検索も当面インデックスなし
- パフォーマンス問題が発生したら追加検討

### 制約

#### NOT NULL制約
- `media.title`
- `media.media_type`

**方針**: 最小限に留める。artistもNULL許可（作者不明のメディアを登録可能に）

#### UNIQUE制約
- `tags.name`
- `media.path`（NULL許可、値がある場合は一意）

#### 外部キー制約
```sql
-- media_tags
FOREIGN KEY (media_id) REFERENCES media(id)
FOREIGN KEY (tag_id) REFERENCES tags(id)

-- media_attributes
FOREIGN KEY (media_id) REFERENCES media(id)
```

**方針**: CASCADE動作は使わない。孤立レコードの削除は別途実装。

### デフォルト値

- `media.flag_exist`: DEFAULT 0（false）
  - CREATE時に`path IS NOT NULL`なら1に設定（アプリケーションロジックで制御）
- `media.created_at`: DEFAULT CURRENT_TIMESTAMP
- `media.updated_at`: DEFAULT CURRENT_TIMESTAMP

### トリガー

```sql
-- updated_atの自動更新
CREATE TRIGGER update_media_timestamp
AFTER UPDATE ON media
FOR EACH ROW
BEGIN
  UPDATE media SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;
```

### スキーマバージョン管理

```sql
CREATE TABLE schema_version (
  version INTEGER PRIMARY KEY,
  applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

マイグレーション実行時にバージョンを記録し、スキーマの履歴を管理する。

## 実装戦略

### SDK実装の優先順位
**決定**: TypeScript SDK優先

**理由**:
- API設計を早く固められる
- 動作確認が早い（Node.js環境ですぐ試せる）
- 設計が固まった段階でRust SDKに移植すれば、手戻りが少ない

### 機能実装の順序
1. マイグレーション機能（スキーマ初期化、バージョン管理）
2. 基本CRUD（create, read by id, update, delete）
3. 検索・フィルタ（read with filters）
4. バルク操作
5. トランザクション管理
6. 統計・集計機能

### テスト戦略

#### テストDB
**基本**: インメモリDB（`:memory:`）
**一部**: 一時ファイル（マイグレーションテスト等）

```typescript
// 基本: インメモリDB
const db = new Database(':memory:');

// マイグレーションテスト等: 一時ファイル
const tmpDb = new Database('/tmp/test-kijuku.db');
```

**理由**:
- インメモリDBは高速で、テストの独立性を保ちやすい
- マイグレーション機能のテストでは実ファイルでの動作確認も必要

#### カバレッジ目標
- ユニットテスト: 80%以上
- 重要なCRUD操作、トランザクション処理は必須

#### テストフレームワーク
- TypeScript: Jest または Vitest
- Rust: cargo test（標準）

## 実行環境

### SDK実行場所
**決定**: NAS上のNode.js環境のみ

**理由**:
- ネットワークファイルシステム越しのSQLiteアクセスは信頼性が低い
- ファイルロック機構が正しく動作しない可能性
- データ破損リスクを避ける

**PC側からの利用方法**:
- SSH経由でNAS上のコマンド実行
- CLIツール経由での操作

## 依存パッケージ（TypeScript SDK）

### SQLiteライブラリ
**決定**: better-sqlite3

**理由**:
- 同期APIでシンプル
- パフォーマンスが高い
- トランザクション処理が書きやすい
- SDK用途に適している

### パッケージマネージャ
**決定**: bun

## API設計

### クラス構成

```typescript
class KijukuDB {
  constructor(dbPath: string, options?: DBOptions)

  // マイグレーション
  migrate(): void
  getSchemaVersion(): number

  // Media CRUD
  createMedia(data: MediaInput): Media
  getMedia(id: number): Media | null
  updateMedia(id: number, data: Partial<MediaInput>): void
  deleteMedia(id: number): void

  // 検索・フィルタ
  findMedia(filter: MediaFilter, options?: QueryOptions): Media[]

  // バルク操作
  bulkCreateMedia(dataList: MediaInput[]): Media[]

  // タグ操作
  createTag(name: string): Tag
  addTagToMedia(mediaId: number, tagId: number): void

  // トランザクション
  transaction<T>(fn: () => T): T
}
```

### 型定義

```typescript
type MediaType = 'comic' | 'video' | 'music';

interface Media {
  id: number;
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

interface MediaInput {
  title: string;
  media_type: MediaType;
  title_id?: string;
  path?: string;
  // ... 他のオプションフィールド
}

interface MediaFilter {
  title?: string;
  title_id?: string;
  artist?: string;
  artist_id?: string;
  media_type?: MediaType;
  series?: string;
  source?: string;
  tag_ids?: number[];
  or_filters?: MediaFilter[];  // OR条件（ネスト可能）
}

interface SortKey {
  field: string;  // 'created_at', 'title', etc.
  order?: 'ASC' | 'DESC';
}

interface QueryOptions {
  sortKeys?: SortKey[];  // 複数指定で多段ソート
  limit?: number;
  offset?: number;
}

interface Tag {
  id: number;
  name: string;
}
```

## 設定管理

### DB接続オプション

```typescript
interface DBOptions {
  timeout?: number;           // クエリタイムアウト（ms）デフォルト: 5000
  readonly?: boolean;         // 読み取り専用モード デフォルト: false
  verbose?: boolean;          // SQLログ出力 デフォルト: false
}
```

### SQLite PRAGMA設定

コンストラクタ内で自動設定：
```typescript
db.pragma('foreign_keys = ON');      // 外部キー制約を有効化
db.pragma('journal_mode = WAL');     // WALモードで安全性向上
```

### 環境変数

```bash
DATABASE_PATH=/path/to/kijuku.db
KIJUKU_DB_TIMEOUT=5000
KIJUKU_DB_VERBOSE=false
```

**設定の優先順位**:
1. コンストラクタ引数
2. 環境変数
3. デフォルト値

## パッケージ情報

```json
{
  "name": "kijuku-db",
  "version": "0.1.0",
  "private": true,
  "license": "MIT"
}
```

**npm公開**: しない（プライベート使用）

### CLIツール

提供機能（後で実装でもOK）:
- `search`: メディア検索
- `import`: JSON/CSV/TSV形式でのインポート
- `migrate`: マイグレーション実行

## 開発環境

### package.json

```json
{
  "name": "kijuku-db",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "main": "./dist/index.js",
  "types": "./dist/index.d.ts",
  "bin": {
    "kijuku-cli": "./dist/cli.js"
  },
  "scripts": {
    "build": "bun build src/index.ts --outdir dist --target node",
    "test": "bun test",
    "dev": "bun run src/index.ts"
  },
  "dependencies": {
    "better-sqlite3": "^11.0.0"
  },
  "devDependencies": {
    "@types/better-sqlite3": "^7.6.0",
    "bun-types": "latest"
  }
}
```

### tsconfig.json

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "outDir": "./dist",
    "declaration": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
```
