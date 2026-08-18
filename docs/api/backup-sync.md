# バックアップ・DB複製・差分・promote gate（prod/stg 運用）

本番DB保護を含む `KijukuDB` のバックアップ系・prod/stg 運用系API。設計の詳細は `docs/design/db-protection.md`・`docs/design/backup.md` を参照。

> **バックアップ出力先**: `BackupOptions.backupDir` 未指定時の既定は「DBパスの親ディレクトリ + `backup/`」。ただしファイル実体のないDB（`:memory:`）では cwd 依存の解決となるため、**`backupDir` の明示指定が必須**（未指定だとエラー・出力先の無断cwd基準解決は禁止）。`KijukuDB(':memory:')` は `backup` 未指定ならバックアップ無効で動作する。

## バックアップ操作

### `backup(): Promise<string | undefined>`

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

### `listBackups(): BackupInfo[]`

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

### `listPreStashes(): BackupInfo[]`

pre-stash（即時復旧用ロールバックファイル）の一覧を取得します（設計 [§8](../design/db-protection.md)）。`listBackups` は pre-stash を除外するため、promote/(b)操作が返す `preStashPath` を失った場合の発見経路として使います。

**パラメータ:** なし

**戻り値:** `BackupInfo[]` - pre-stash 情報の配列（id 降順）。`path` はそのまま `BackupSelector.byPath()` で `restore` に渡して prod を即時復旧できます。

**使用例:**

```typescript
const stashes = db.listPreStashes();
// path を byPath で restore に渡し prod を即時復旧（§8）
if (stashes[0]) {
  db.restore(BackupSelector.byPath(stashes[0].path));
}
```

---

### `backupWithLabel(label: string): Promise<string | undefined>`

ラベル付き手動バックアップを実行します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `label` | `string` | 必須 | バックアップのラベル（例: `"before_import"`） |

**戻り値:** `Promise<string | undefined>` - バックアップファイルのパス。バックアップマネージャーが設定されていない場合は`undefined`

**使用例:**

```typescript
const backupPath = await db.backupWithLabel('before_import');
if (backupPath) {
  console.log(`バックアップを作成しました: ${backupPath}`);
}
```

---

### `restore(selector?: BackupSelector): string`

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

### `getBackupManager(): BackupManager | undefined`

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

### `setBackupLabel(id: string, label: string | undefined): void`

既存バックアップにラベルを事後付与します（`undefined`で解除）。`BackupManager` の同名メソッドの委譲です。

---

### `setBackupNote(id: string, note: string | undefined): void`

既存バックアップにメモを事後付与します（`undefined`で解除）。`BackupManager` の同名メソッドの委譲です。

---

### `getBackupMeta(id: string): BackupMetaEntry | null`

バックアップの事後メタ（ラベル・メモ）を取得します。型は [`BackupMetaEntry`](./types.md#backupmetaentry--labelsource事後ラベルメモ) を参照。

---

## DB複製（sync）

prod(RO)→stg(RW) のフル複製を行います（設計 §4.2/§4.5・本番DB保護 P1）。LLM 編集用の stg を prod から生成・更新する基盤。Online Backup API で src を読み取り専用コピーし、dst を新規生成します（既存 dst は完全上書き）。

### `static replicateDb(src: string, dst: string, operation: 'sync' | 'discard' = 'sync'): Promise<void>`

`src`（prod）を `dst`（stg）へフル複製します。`BackupManager.copyDbOnline` で src を RO コピーし dst を生成します。既存 dst と WAL/SHM 副産物（`-wal`/`-shm`）は事前に削除し、WAL モードの残留を排除して完全な複製を保証します。Rust の `KijukuDB::replicate_db` と同等。

**排他前提:** 先頭で `<dst>.lock` の排他ロックを取得し（別セッション編集中は `StgBusyError`・設計 §15-11 P1）。コピー後に sync 元 prod revision 指紋を `<dst>.meta.json` に記録し、observe が drift を検出できるようにします。`src === dst` は誤設定としてエラー。`operation: 'discard'` は stg 破棄・再構築（処理は同一・監査ログの操作名のみ区別）。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `src` | `string` | ○ | 複製元（prod）のDBパス |
| `dst` | `string` | ○ | 複製先（stg）のDBパス |
| `operation` | `'sync' \| 'discard'` | | 監査ログ上の操作名（既定: `sync`） |

**使用例:**

```typescript
// prod → stg へ複製（stg を LLM 編集用に最新化）
await KijukuDB.replicateDb('/data/kijuku.db', '/data/kijuku.stg.db');
```

---

### `static BackupManager.copyDbOnline(src: string, dst: string): Promise<void>`

任意の src→dst の Online Backup コピー（設計 §4.5）。`replicateDb` の基盤となる関連関数で、`BackupManager` インスタンスに依存せず src を RO で開いて dst を新規生成します。

**使用例:**

```typescript
import { BackupManager } from 'kijuku-db';

await BackupManager.copyDbOnline('/data/prod.db', '/data/stg.db');
```

---

## バックアップ読み取り操作

バックアップファイルから読み取り専用でデータを取得します。リストアせずに過去のデータを確認する用途に使用します。

### `getMediaFromBackup(id: number, selector?: BackupSelector): Media | null`

バックアップからメディアをIDで取得します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `id` | `number` | 必須 | メディアID |
| `selector` | `BackupSelector` | | バックアップ選択条件（省略時: 最新） |

**戻り値:** `Media | null` - メディア情報

**使用例:**

```typescript
const media = db.getMediaFromBackup(1);
const mediaFromOld = db.getMediaFromBackup(1, BackupSelector.nth(2));
```

---

### `findMediaFromBackup(filter: MediaFilter, options?: QueryOptions, selector?: BackupSelector): Media[]`

バックアップから条件に合うメディアを検索します。

**パラメータ:** `findMedia`と同じ（`selector`が追加）

**戻り値:** `Media[]`

**使用例:**

```typescript
const media = db.findMediaFromBackup({ media_type: 'comic' });
```

---

### `getTagByNameFromBackup(name: string, selector?: BackupSelector): Tag | null`

バックアップからタグを名前で取得します。

**戻り値:** `Tag | null`

---

### `getAllTagsFromBackup(selector?: BackupSelector): Tag[]`

バックアップから全タグを取得します。

**戻り値:** `Tag[]`

---

### `getMediaTagsFromBackup(mediaId: number, selector?: BackupSelector): Tag[]`

バックアップからメディアに紐付くタグを取得します。

**戻り値:** `Tag[]`

---

### `getMediaAttributeFromBackup(mediaId: number, key: string, selector?: BackupSelector): MediaAttribute | null`

バックアップからメディアの特定の属性を取得します。

**戻り値:** `MediaAttribute | null`

---

### `getMediaAttributesFromBackup(mediaId: number, selector?: BackupSelector): MediaAttribute[]`

バックアップからメディアの全属性を取得します。

**戻り値:** `MediaAttribute[]`

---

### `getMediaHashesFromBackup(itemUuid: string, selector?: BackupSelector): MediaHash[]`

バックアップから特定作品の全ハッシュを取得します。

**戻り値:** `MediaHash[]`

---

### `getMediaHashFromBackup(itemUuid: string, filename: string, timeRange: string, selector?: BackupSelector): MediaHash | null`

バックアップから特定位置（`item_uuid` × `filename` × `time_range`）のハッシュを取得します。

**戻り値:** `MediaHash | null`

---

### `findByContentHashFromBackup(hashBytes: Uint8Array, selector?: BackupSelector): MediaHash[]`

バックアップからSHA256による完全一致検索を行います。

**戻り値:** `MediaHash[]`

---

### `findDuplicateHashesFromBackup(selector?: BackupSelector): Array<{ content_hash: Uint8Array; count: number }>`

バックアップから重複ハッシュを検出します。戻り型は `findDuplicateHashes()` に準じます。

---

## 差分・promote gate操作

prod/stg 運用における差分取得と機械的 promote gate（設計 §3.4/§4.4/§4.5）。いずれも `self` を stg（RW）とし、prod は readonly 別接続で開きます（prod に書込むのは `promote` の内部のみ）。詳細は `docs/design/db-protection.md` を参照。

### `diffWithBackup(selector?: BackupSelector, options?: DiffOptions): BackupDiff`

現在DB（stg）とバックアップの差分を取得します（復元判断用）。方向: `added` = バックアップに在り現在に無い（復元で復活）、`removed` = 現在に在りバックアップに無い（復元で失われる）、`changed` = 両方に在り内容が異なる（復元で上書き）。

**戻り値:** `BackupDiff` - [`BackupDiff`](./types.md#backupdiff--diffoptionsバックアップ差分) を参照

---

### `diffWithProd(prodDbPath: string, options?: DiffOptions): BackupDiff`

prod(RO) と現在DB（stg）の差分を取得します（promote 判断用・設計 §4.4）。セマンティクスは `diffWithBackup` と反転: `added` = stg のみ（promote で prod に追加）、`removed` = prod のみ（promote で prod から削除）、`changed` = 両方で異なる（promote で prod が上書き）。prod 側 `backup/meta/audit.log` に `diffProdStg` として監査記録されます。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `prodDbPath` | `string` | ○ | 比較対象の prod DBパス |
| `options` | `DiffOptions` | | 差分の詳細度（省略時 `limited{n: 100}`） |

**戻り値:** `BackupDiff`

---

### `observe(prodDbPath: string, options?: ObserveOptions): ObserveResult`

stg（self）と prod を比較し、機械的 promote gate を評価します（設計 §3.4/§4.4）。内部で `diffWithProd` と同等の差分を計算・要約した上で、以下の gate を評価します:

- `PRAGMA integrity_check` == "ok"
- 外部キー整合性（`foreign_key_check` 空 + FK 有効）
- prod/stg の `schema_version` 一致
- 件数・差分上限（`summary.totals` vs `GateConfig`）
- 運用者定義 golden assertion

`passed` は全 gate 合格か（promote 可否の客観判定・人間 gate ではない）。

**戻り値:** `ObserveResult` - [`ObserveResult`](./types.md#promote-gateobserve機械的-gate) を参照

---

### `async promote(prodDbPath: string, options?: ObserveOptions, backupOpts?: BackupOptions): Promise<PromoteOutcome>`

stg（self）→ prod への反映（設計 §4.5）。`replicateDb`（sync: prod→stg）の反転で、gate 合格で prod を上書きし pre-stash（`tmp/{stem}.{ts}-pre_promote.db`）を作成します。gate 不合格時は prod を触る前に `PromoteGateFailedError` を throw します（pre-stash も作らない）。prod 排他ロック取得後に gate を再評価し、stale な gate 結果での上書きを防ぎます（TASK-76）。

`backupOpts` で pre-stash 先（`backupDir`）をカスタマイズ可能（省略時は `tmp/` のみ）。pre-stash 自体は常時実行されます。

**戻り値:** `Promise<PromoteOutcome>` - gate 評価結果（`observe`）と pre-stash パス

---

## 監査ログ操作

本番DB保護操作（sync/discard・observe/diffProdStg・promote・restore/mediaMv/purgeTrash）の事後追跡用監査ログです。prod DB本体ではなく`backup/meta/audit.log`（JSONL）にappend-onlyで記録されます。詳細は設計§10を参照してください。

### `listAuditLogs(filter?: AuditLogFilter): AuditRecord[]`

監査ログをフィルタ適用して取得します。新しい順（timestamp降順）で返します。

**パラメータ:**

| 名前 | 型 | 必須 | 説明 |
|------|-----|------|------|
| `filter` | `AuditLogFilter` | | フィルタ条件（省略時: デフォルト） |

**AuditLogFilter:**

| プロパティ | 型 | デフォルト | 説明 |
|-----------|-----|-----------|------|
| `operation` | `string \| undefined` | `undefined` | 操作種別でフィルタ（完全一致・省略時: 全操作） |
| `from` | `string \| undefined` | `undefined` | 開始日時（ISO 8601・包含） |
| `to` | `string \| undefined` | `undefined` | 終了日時（ISO 8601・包含） |
| `limit` | `number \| undefined` | `1000` | 上限件数（最大10000） |

**戻り値:** `AuditRecord[]`

**AuditRecord:**

| プロパティ | 型 | 説明 |
|-----------|-----|------|
| `timestamp` | `string` | ISO 8601 UTC（ミリ秒） |
| `operation` | `string` | 操作種別（`sync`, `discard`, `observe`, `diffProdStg`, `promote`, `b-restore`, `b-mediaMv`, `b-purgeTrash`） |
| `target` | `AuditTarget` | 操作の対象DB（`prod` または `stg`） |
| `actor` | `string \| null` | 実行者（`USER`環境変数・取れなければnull） |
| `prodDbPath` | `string` | prod DBの絶対パス |
| `result` | `AuditResult` | 操作結果（`success`, `failure`, `dryRun`） |
| `error` | `string \| null` | failure時のエラー文字列 |
| `summary` | `object` | 操作ごとの構造的サマリ（diff/gate結果・revision・preStashPath等） |

**使用例:**

```typescript
// 全監査ログを取得（デフォルト上限1000件）
const allLogs = db.listAuditLogs();

// 特定操作でフィルタ
const promoteLogs = db.listAuditLogs({ operation: 'promote' });

// 日時範囲でフィルタ
const recentLogs = db.listAuditLogs({
  from: '2026-07-01T00:00:00.000Z',
  to: '2026-07-31T23:59:59.999Z',
  limit: 500,
});

// 失敗した操作のみ
const failureLogs = db.listAuditLogs().filter(log => log.result === 'failure');
```

**監査される操作:**

- `sync` - prod→stg複製（§4.2）
- `discard` - stg破棄・再sync（§4.6）
- `observe` - prod-stg差分チェック・gate評価（§5）
- `diffProdStg` - prod-stg差分表示
- `promote` - stg→prod反映（§6）
- `b-restore` - バックアップ復元
- `b-mediaMv` - メディアファイル移動
- `b-purgeTrash` - trash完全削除

**重要:** 監査ログはprod DB外の`backup/meta/audit.log`に記録されるため、promote後もprod側の監査ログは残ります（破壊的上書きで消えません）。

---

