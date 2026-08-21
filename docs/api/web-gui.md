# Web GUIサーバー


## `startServer(db: KijukuDB, options: ServerOptions): void`

認証付きWeb GUIサーバーを起動します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `db` | `KijukuDB` | 必須 | データベースインスタンス |
| `options` | `ServerOptions` | 必須 | サーバー設定オプション |

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

## APIエンドポイント

### `POST /api/auth/login`

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

### `POST /api/auth/logout`

ログアウトを行います。

**レスポンス:**
```json
{
  "success": true
}
```

---

### `GET /api/media`

メディア一覧を取得します（認証必須）。

**クエリパラメータ:**

| 名前 | 型 | 説明 |
|------|-----|------|
| `title` | `string` | タイトルで検索（部分一致） |
| `artist` | `string` | 作者で検索（部分一致） |
| `series` | `string` | シリーズで検索（部分一致） |
| `media_type` | `string` | メディアタイプ（comic/video/music） |
| `uuid` | `string` | UUID完全一致（uuidカラムはUNIQUE） |
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

### `GET /api/media/:id`

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

### `GET /api/media/uuid/:uuid`

UUIDでメディア詳細を取得します（認証必須。uuidカラムはUNIQUE）。

**パスパラメータ:**

| 名前 | 型 | 説明 |
|------|-----|------|
| `uuid` | `string` | メディアUUID |

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

