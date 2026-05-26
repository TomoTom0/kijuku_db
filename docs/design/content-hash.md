# Content Hash - ファイル内容ベースの同定機構

## 背景

現在のkijuku-dbはファイルパス（`media.path`、UNIQUE制約）を唯一の実質的な識別子としている。ファイル内容のハッシュ値は一切保存していないため、以下の問題がある：

- ファイル名変更でエントリとの紐付けが切れる
- 同一内容の別パスファイルの重複検出が不可能
- ディレクトリ移動後の再同定ができない

## 設計方針

### 3段階の同定レベル

将来的な拡張を見据え、以下の3段階を想定する。第一フェーズでは段階1のみ実装する。

| レベル | 目的 | Music | Comic | Video |
|---|---|---|---|---|
| **1. 完全同一** | バイトレベル一致 | SHA256 | SHA256 | SHA256 |
| **2. 知覚的同一** | 解像度・品質差を吸収 | AcoustID | pHash（表紙） | キーフレームpHash |
| **3. ふわっと同一** | ノイズ・余分ページ無視 | 類似度閾値緩和 | ページセット比較 | 複数キーフレーム比較 |

段階2・3は外部依存（Chromaprint、FFmpeg等）が重く、現時点では要件もないため後回し。

### 段階1（SHA256）のみ先行実装する理由

- `sha2` crateは既にCargo.tomlに存在し、追加依存ゼロ
- ビルド環境への影響なし
- ファイル名変更後の再同定という現状の問題を解決する
- 段階2・3は実際に要望が出てから検討

### 全体hashと部分hashの併用

各メディアタイプで、ファイル全体のhashに加えて部分hashも記録する。部分hashは同定精度の向上と段階2・3の基礎データとして機能する。

| media_type | 全体hash | 部分hash |
|---|---|---|
| music | ファイル全体 | 先頭N秒等の区間 |
| video | ファイル全体 | キーフレーム区間等 |
| comic | 全ページ結合 | 各ページファイル |

### 照合フロー

```
SHA256で完全一致 lookup（content_hashインデックス、O(1)）
  → 見つからなければ段階2の軽量指紋で候補抽出（将来実装）
    → 候補があれば段階3の詳細比較（将来実装）
```

### FK参照の方針

本テーブルは`media.uuid`（TEXT）をFK参照先とする。既存の`media_tags`・`media_attributes`は`media.id`（INTEGER）を参照しているが、本テーブルはuuid参照とする。

理由：
- リモート同期環境で`id`（auto-increment）はDBインスタンスごとに異なる値になる。uuidはグローバルに一意なため、同期対象として安全
- 将来的に他テーブルもuuid参照に統一する可能性はあるが、本設計では本テーブルのみuuid参照とする

## スキーマ設計

### media_hashes テーブル

```sql
CREATE TABLE media_hashes (
    item_uuid       TEXT NOT NULL,
    filename        TEXT NOT NULL DEFAULT '',
    time_range      TEXT NOT NULL DEFAULT '',
    content_hash    BLOB NOT NULL CHECK(length(content_hash) = 32),
    alternative_of  TEXT,
    embedding       BLOB,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (item_uuid, filename, time_range),
    FOREIGN KEY (item_uuid) REFERENCES media(uuid) ON DELETE CASCADE
);

CREATE INDEX idx_media_hashes_content ON media_hashes(content_hash);

CREATE TRIGGER update_media_hashes_timestamp
AFTER UPDATE ON media_hashes
FOR EACH ROW
BEGIN
    UPDATE media_hashes SET updated_at = datetime('now')
    WHERE item_uuid = NEW.item_uuid AND filename = NEW.filename AND time_range = NEW.time_range;
END;
```

### マイグレーション（schema_version 5 → 6）

`migration.rs`にcase 6を追加：

```rust
6 => {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS media_hashes (
            item_uuid       TEXT NOT NULL,
            filename        TEXT NOT NULL DEFAULT '',
            time_range      TEXT NOT NULL DEFAULT '',
            content_hash    BLOB NOT NULL CHECK(length(content_hash) = 32),
            alternative_of  TEXT,
            embedding       BLOB,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (item_uuid, filename, time_range),
            FOREIGN KEY (item_uuid) REFERENCES media(uuid) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_media_hashes_content ON media_hashes(content_hash);

        CREATE TRIGGER IF NOT EXISTS update_media_hashes_timestamp
        AFTER UPDATE ON media_hashes
        FOR EACH ROW
        BEGIN
            UPDATE media_hashes SET updated_at = datetime('now')
            WHERE item_uuid = NEW.item_uuid AND filename = NEW.filename AND time_range = NEW.time_range;
        END;
        "
    )?;
    tx.execute("INSERT OR IGNORE INTO schema_version (version) VALUES (?)", [version])?;
}
```

- `target_version` を 5 から 6 に更新
- `schema.sql` にも同テーブル定義を追加
- 新規テーブル追加なのでALTER TABLEではなくCREATE TABLE IF NOT EXISTSで対応

### PK設計: (item_uuid, filename, time_range)

自動採番idではなく3カラムの複合PKを採用。理由：

- リモート同期でid衝突を回避できる
- `(item_uuid, filename, time_range)` で一意に定まる
- `filename`と`time_range`を分けることで型の意味が明示的

### 各カラムの意味

| カラム | 型 | 説明 |
|---|---|---|
| `item_uuid` | TEXT NOT NULL | `media.uuid`へのFK。どの作品か |
| `filename` | TEXT NOT NULL | ファイル名。Comicのページファイル等。該当しない場合は空文字 |
| `time_range` | TEXT NOT NULL | 時間範囲（秒）。`開始秒-終了秒`形式。該当しない場合は空文字 |
| `content_hash` | BLOB NOT NULL | SHA256ハッシュ（32バイト） |
| `alternative_of` | TEXT NULLABLE | 同じ内容の原本の識別情報。NULLなら原本 |
| `embedding` | BLOB NULLABLE | 将来の画像検索用embeddingベクトル |
| `created_at` | TEXT NOT NULL | 作成日時 |
| `updated_at` | TEXT NOT NULL | 更新日時（トリガー自動更新） |

### filename と time_range の値

`filename`と`time_range`はメディアタイプとhashの粒度に応じて使い分ける：

**Music**

| filename | time_range | 意味 |
|---|---|---|
| `` | `` | ファイル全体hash |
| `` | `0.0-30.0` | 先頭30秒hash |

**Video**

| filename | time_range | 意味 |
|---|---|---|
| `` | `` | ファイル全体hash |
| `` | `0.0-5.0` | 最初の5秒hash |
| `` | `145.0-150.0` | 特定区間hash |

**Comic**

| filename | time_range | 意味 |
|---|---|---|
| `` | `` | 全体hash（全ページ結合） |
| `001.jpg` | `` | 表紙hash |
| `002.jpg` | `` | 2ページ目hash |

`filename`と`time_range`を分ける理由：

- Comic: `001.jpg`と`0.jpg`の混同を防ぐ。不正な`0.jpg`がソート順で先頭になっても`filename`で本来のファイル名が分かる
- Video: 整数だけでは動画のどの部分か特定できない。秒数が必要
- 各メディアタイプで必要な識別子が異なるため、単一カラムへの統合は曖昧さを生む

### time_range のフォーマット

- 形式: `開始秒-終了秒`（例: `0.0-30.0`, `145.2-150.8`）
- 小数点以下の精度は小数第一位（100ミリ秒単位）を標準とする
- 全体を表す場合は空文字

### alternative_of の設計

同じ視覚的内容だが別フォーマット・別位置にある場合の紐付け。

`alternative_of`には原本の識別情報を格納する。書式はメディアタイプに応じて`filename`または`time_range`の値を使用する。

Comic（別フォーマットの同一画像）:

| filename | time_range | alternative_of |
|---|---|---|
| `001.jpg` | `` | NULL（原本） |
| `001.gif` | `` | `001.jpg` |
| `001.png` | `` | `001.jpg` |

Video（別品質の同一区間）:

異エンコード版は別の`item_uuid`（別メディアエントリ）として登録する。同じPK `(item_uuid, filename, time_range)`で2行は作れないため、同一`item_uuid`内で`alternative_of`が指す先は常に別の`(filename, time_range)`となる。

| item_uuid | filename | time_range | alternative_of |
|---|---|---|---|
| `abc...` | `` | `0.0-5.0` | NULL（原本） |
| `def...` | `` | `0.0-5.0` | NULL（別エンコード版、別エントリ） |

Comic（別フォーマットの同一画像）は`filename`が異なるため同一`item_uuid`内で表現可能。VideoやMusicで同一位置の別品質版を表現したい場合は別エントリとして登録する。

多対多（3つ以上が同一内容）の場合、全ての代替版が原本を指すため再帰クエリ不要で原本が一意に定まる。

`content_hash`インデックスだけでは段階2（知覚的同一）の結果を記録できない。フォーマットが違えばSHA256も別値になるため、`alternative_of`で関係性を明示的に保存する。

**注意**: SQLiteは複合FK（`REFERENCES media_hashes(item_uuid, filename, time_range)`）をサポートしないため、`alternative_of`の参照整合性はアプリケーション層で保証する。

### embedding 列

将来の画像検索用。別backendで生成されたembeddingベクトルをBLOBとして保存する。

- 用途は「保存のみ」。ベクトル類似度検索にはsqlite-vec等の導入を将来検討
- embeddingの次元数は未定。BLOBにfloat32配列をシリアライズして格納
- 段階1ではカラム定義のみで利用しない

### インデックス

| インデックス | 用途 |
|---|---|
| PK `(item_uuid, filename, time_range)` | 特定作品の全ハッシュ取得、特定位置のハッシュ取得 |
| `idx_media_hashes_content(content_hash)` | SHA256完全一致lookup（照合フローの第一段階） |

## content_hash の一意性

`content_hash`にUNIQUE制約は設けない。同一ハッシュを持つ別`(item_uuid, filename, time_range)`の存在を許容する。これは重複検出の用途で必要である：

- 同じファイルが別のエントリとして登録されているケース
- 異なる作品に同じ画像が含まれるケース

重複検出のクエリ:

```sql
SELECT content_hash, COUNT(*) as cnt
FROM media_hashes
GROUP BY content_hash
HAVING cnt > 1;
```

## alternative_of 参照整合性

SQLiteの制約によりFKで参照先を保証できないため、アプリケーション層で以下を保証する：

- `alternative_of`に設定する値は、同じ`item_uuid`内に実際に存在する`(filename, time_range)`でなければならない
- 原本行（`alternative_of = NULL`）が削除される場合、その代替行も併せて削除する

削除連鎖はRust SDKの`delete_media_hash`（単一行削除）内で実装する。原本行を削除する際、同じ`item_uuid`内で`alternative_of`が該当`(filename, time_range)`を指す行を先に削除してから原本行を削除する。

## API設計

### Rust SDK

`KijukuDB`に以下のメソッドを追加する。プロジェクト方針「SDKが先、CLIはSDKの薄いラッパー」に従う。

```rust
// 型定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaHash {
    pub item_uuid: String,
    pub filename: String,
    pub time_range: String,
    pub content_hash: Vec<u8>,
    pub alternative_of: Option<String>,
    pub embedding: Option<Vec<u8>>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaHashInput {
    pub item_uuid: String,
    pub filename: String,
    pub time_range: String,
    pub content_hash: Vec<u8>,
    pub alternative_of: Option<String>,
}

impl KijukuDB {
    // ハッシュ計算・登録（ファイル読み込み + SHA256計算 + DB登録）
    fn compute_media_hash(&self, item_uuid: &str) -> Result<Vec<MediaHash>>;

    // ハッシュ計算・登録（フィルタ条件で対象絞り込み）
    fn compute_media_hashes(&self, filter: &MediaFilter) -> Result<Vec<MediaHash>>;

    // ハッシュ登録（単件、計算済みハッシュの登録用）
    fn add_media_hash(&self, input: &MediaHashInput) -> Result<MediaHash>;

    // ハッシュ登録（一括、Comicの数百ページ用）
    fn add_media_hashes(&self, inputs: &[MediaHashInput]) -> Result<Vec<MediaHash>>;

    // 特定作品の全ハッシュ取得
    fn get_media_hashes(&self, item_uuid: &str) -> Result<Vec<MediaHash>>;

    // 特定位置のハッシュ取得
    fn get_media_hash(&self, item_uuid: &str, filename: &str, time_range: &str) -> Result<Option<MediaHash>>;

    // SHA256による完全一致検索
    fn find_by_content_hash(&self, hash: &[u8]) -> Result<Vec<MediaHash>>;

    // 特定位置のハッシュ削除（代替行の連鎖削除を含む）
    fn delete_media_hash(&self, item_uuid: &str, filename: &str, time_range: &str) -> Result<()>;

    // 特定作品のハッシュ全削除
    fn delete_media_hashes(&self, item_uuid: &str) -> Result<()>;

    // 重複ハッシュの検出
    fn find_duplicate_hashes(&self) -> Result<Vec<(Vec<u8>, i64)>>;
}
```

### TypeScript SDK

```typescript
// 型定義
export interface MediaHash {
  item_uuid: string;
  filename: string;
  time_range: string;
  content_hash: Uint8Array;
  alternative_of?: string;
  embedding?: Uint8Array;
  created_at: string;
  updated_at: string;
}

export interface MediaHashInput {
  item_uuid: string;
  filename: string;
  time_range: string;
  content_hash: Uint8Array;
  alternative_of?: string;
}

// KijukuDB（ローカル）
class KijukuDB {
  computeMediaHash(itemUuid: string): MediaHash[];
  computeMediaHashes(filter: MediaFilter): MediaHash[];
  addMediaHash(input: MediaHashInput): MediaHash;
  addMediaHashes(inputs: MediaHashInput[]): MediaHash[];
  getMediaHashes(itemUuid: string): MediaHash[];
  getMediaHash(itemUuid: string, filename: string, timeRange: string): MediaHash | null;
  findByContentHash(hash: Uint8Array): MediaHash[];
  deleteMediaHashes(itemUuid: string): void;
  findDuplicateHashes(): Array<{ content_hash: Uint8Array; count: number }>;
}

// RemoteKijukuDB（リモート）
// SSH経由でNAS上の処理を実行するため、computeMediaHashも提供可能
class RemoteKijukuDB {
  computeMediaHash(itemUuid: string): Promise<MediaHash[]>;
  computeMediaHashes(filter: MediaFilter): Promise<MediaHash[]>;
  addMediaHash(input: MediaHashInput): Promise<MediaHash>;
  addMediaHashes(inputs: MediaHashInput[]): Promise<MediaHash[]>;
  getMediaHashes(itemUuid: string): Promise<MediaHash[]>;
  findByContentHash(hash: Uint8Array): Promise<MediaHash[]>;
  // ...
}
```

### CLI サブコマンド

```
kijuku-cli hash <command> [options]

# ハッシュ計算・登録
kijuku-cli hash compute --uuid <item_uuid> [--all | --filter <filter>]

# 特定作品のハッシュ表示
kijuku-cli hash list --uuid <item_uuid>

# SHA256による検索
kijuku-cli hash find --hash <sha256_hex>

# 重複ハッシュ検出
kijuku-cli hash duplicates
```

`hash compute`は指定作品（または全作品）のファイルを読み込み、SHA256を計算してDBに登録する。

### hash compute の処理フロー

各メディアタイプごとの計算・登録フロー：

**Music**

```
1. media.path からファイルパスを取得
2. ファイル全体のSHA256をストリーミング計算 → (filename="", time_range="") として登録
3. 先頭30秒のSHA256を計算 → (filename="", time_range="0.0-30.0") として登録
```

**Video**

```
1. media.path からファイルパスを取得
2. ファイル全体のSHA256をストリーミング計算 → (filename="", time_range="") として登録
3. 段階1では部分hashは計算しない（キーフレーム抽出に動画デコードが必要なため）
```

**Comic**

```
1. media.path からディレクトリパスを取得
2. ディレクトリ内の画像ファイルを列挙（拡張子フィルタ: jpg, jpeg, png, gif, webp等）
3. 各ファイルのSHA256を計算 → (filename="001.jpg", time_range="") として登録
4. 全ファイルのSHA256をソート結合して全体hashを計算 → (filename="", time_range="") として登録
```

全体hash（Comic）の計算式：

```
whole_hash = SHA256(sort([SHA256(001.jpg), SHA256(002.jpg), ...]))
```

ファイルのソート順はファイル名の辞書順とする。順序に依存しないため、ファイル名が変わっても全体hashは同一。

### hash compute のオプション

```
kijuku-cli hash compute [options]

--uuid <item_uuid>     特定作品のみ計算
--all                  ハッシュ未計算の全作品を計算
--force                既存ハッシュがあっても再計算
--filter <filter>      フィルタ条件で絞り込み
--dry-run              計算結果を表示のみ（DB登録しない）
```

### 計算対象の判定

- `media.flag_exist = 1`のエントリのみ対象
- `media.path IS NOT NULL`のエントリのみ対象
- `--all`の場合、`media_hashes`にレコードがない作品を優先
- `--force`の場合、既存ハッシュを上書き

### スキップ条件

以下の場合は警告を出してスキップする：

- `media.path`が存在しない
- Comic: ディレクトリ内に画像ファイルがない
- Music/Video: ファイルサイズが0

## content_hash BLOBの取り扱い

`content_hash`は32バイト固定長のBLOBとして保存する。SDK間での取り扱い：

- **Rust**: `Vec<u8>`（32バイト）。`sha2` crateの出力をそのまま使用
- **TypeScript**: `Uint8Array`（32バイト）。Web Crypto APIの`crypto.subtle.digest`の出力をそのまま使用
- **CLI入出力**: HEX文字列（64文字）で表示・入力。SDK内部でBLOB <-> HEX変換を行う
- **JSON API**: base64エンコードでシリアライズ

`find_by_content_hash`への入力はHEX文字列を想定し、SDK内部でBLOBに変換してクエリを実行する。

`decisions.md`に定義されたSDK境界に基づき：

- **SDKの責務**: ハッシュの計算・登録・取得・検索・削除（compute_media_hash でファイル読み込み + SHA256計算 + DB登録まで一括）
- **CLIの責務**: SDKのメソッドを呼ぶだけの薄いラッパー
- **RemoteKijukuDB**: SSH経由でNAS上の処理を実行するため、computeMediaHashも提供する

## 初回スキャン戦略

既存のメディアエントリにはハッシュが未計算（`media_hashes`にレコードなし）。段階的な移行が必要：

1. `media_hashes`にレコードがない`flag_exist = 1`のメディアがハッシュ未計算対象
2. `kijuku-cli hash compute --all`でバッチ実行
3. トランザクション単位でコミット（1作品ずつ）し、長時間ロックを回避
4. Comicはページ数が多いため、1作品の全ページを1トランザクションで一括登録

## decisions.md の更新

本設計の実装時に以下の変更を`decisions.md`に反映する（設計承認後、実装開始前に更新）：

- 外部キー制約のCASCADE方針を更新（version 5で`media_tags`・`media_attributes`にCASCADEを追加済み、本テーブルもCASCADEを採用）
- 補助ツールの「重複検出」を「当面不要」から「content-hash機能として実装」に更新
