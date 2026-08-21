# RemoteKijukuDB（SSH リモート）

SSH 経由でリモートホスト上の DB を操作する `RemoteKijukuDB`（TypeScript SDK）のAPI仕様。


SSH経由でリモートサーバーのDBを操作するクラス。ローカルの`KijukuDB`と同等のAPIを非同期（`Promise`）で提供します。

## RemoteKijukuDBコンストラクタ

### `new RemoteKijukuDB(config: RemoteConfig)`

**RemoteConfig:**

| プロパティ | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| `sshHost` | `string` | 必須 | SSH接続先ホスト（.ssh/configのHost名） |
| `dbPath` | `string` | | リモートの prod DB パス（デフォルト: `~/.local/share/kijuku/kijuku.db`） |
| `stgDbPath` | `string` | | リモートの stg DB パス（未指定時は `dbPath` から `<stem>.stg.db` を導出・設計 §4.1） |
| `binaryPath` | `string` | | リモートのkijuku-cliパス（デフォルト: `~/.local/bin/kijuku-cli`） |
| `port` | `number` | | SSHポート（省略時はSSH設定から読み取り） |
| `mediaRoot` | `string` | | リモートホスト上の media root（ファイル操作APIのサンドボックス境界。未設定だとファイル操作APIはエラー） |
| `target` | `'prod' \| 'stg'` | | 操作対象DB（デフォルト: `stg`・RW。`prod` は readonly + migrate skip） |
| `workDir` | `string` | | 作業ディレクトリ（将来の拡張用） |

**使用例:**

```typescript
import { RemoteKijukuDB } from 'kijuku-db';

const remoteDb = new RemoteKijukuDB({
  sshHost: 'example.com',
  dbPath: '/path/to/kijuku.db',
});
```

## RemoteKijukuDB対応メソッド一覧

以下のメソッドは`KijukuDB`と同じシグネチャですが、戻り値が`Promise`で包まれます。

| カテゴリ | メソッド | 備考 |
|---------|---------|------|
| **マイグレーション** | `migrate()`, `getSchemaVersion()`, `getTables()`, `getTableInfo()` | |
| **メディアCRUD** | `createMedia()`, `getMedia()`, `getMediaByUuid()`, `updateMedia()`, `deleteMedia()` | |
| **メディア検索** | `findMedia()`, `getDistinctValues()` | |
| **バルク操作** | `bulkCreateMedia()`, `bulkDeleteMedia()`, `bulkUpdateMedia()` | |
| **タグ操作** | `createTag()`, `getTagByName()`, `getAllTags()`, `addTagToMedia()`, `removeTagFromMedia()`, `getMediaTags()`, `getTagUsageStats()`, `findUnusedTags()` | |
| **属性操作** | `setMediaAttribute()`, `getMediaAttribute()`, `getMediaAttributes()`, `deleteMediaAttribute()`, `deleteAllMediaAttributes()` | |
| **サムネイル** | `checkThumbnail()`, `updateThumbnail()` | |
| **ハッシュ操作** | `addMediaHash()`, `addMediaHashes()`, `getMediaHashes()`, `getMediaHash()`, `findByContentHash()`, `deleteMediaHash()`, `deleteMediaHashes()`, `findDuplicateHashes()`, `computeMediaHash()`, `computeMediaHashes()` | |
| **ファイル存在** | `updateExist()` | |
| **タグ一括取得** | `getMediaTagsBulk()` | JOIN 1発 |
| **ファイル操作** | `mediaCp()`, `mediaMv()`, `mediaSync()` | dry-run ファースト（config `mediaRoot` がサンドボックス境界） |
| **trash操作** | `moveToTrash()`, `listTrash()`, `restoreFromTrash()`, `purgeTrash()` | |
| **ファイル転送** | `upload()`, `download()` | SFTP 経由のファイル送受信 |
| **DB複製** | `sync()`, `discard()` | prod(RO)→stg(RW) のフル複製 / stg 破棄・再構築 |
| **差分・promote gate** | `diffWithBackup()`, `diffWithProd()`, `observe()`, `promote()` | [backup-sync.md](./backup-sync.md) と同等の prod/stg 運用 |
| **バックアップメタ** | `setBackupLabel()`, `setBackupNote()`, `getBackupMeta()` | 事後ラベル/メモ |
| **監査ログ** | `listAuditLogs()` | prod 側 `backup/meta/audit.log` |

**相違点:**
- 全メソッドが`Promise`を返す（例: `getMedia(id): Promise<Media | null>`）
- `close()`は存在しない。SSH session pool（TASK-70/71）で接続を再利用するため、one-shot プロセスでは終了前に `disconnect()` を呼ぶこと（未呼び出しだとプロセスが終了しないことがある）

## RemoteKijukuDB DB複製（sync）

### `sync(timeoutMs?: number): Promise<{ prodPath: string; stgPath: string }>`

prod(RO)→stg(RW) のフル複製をリモート CLI に委譲します（設計 §4.2/§4.5）。リモート側の prod/stg パス間でファイルコピーが完結し（NAS 上で閉じる）、SSH 経由で DB 実体を転送しません。prod/stg パスは config（`dbPath`/`stgDbPath`）から自動解決し `from`/`to` でリモートに明示渡します（リモート側の環境変数に依存しない）。タイムアウトは prod DB サイズから自動計算します（`timeoutMs` で上書き可）。`prodPath === stgPath` はリモート側で弾かれます。

**使用例:**

```typescript
// リモートの prod → stg を複製（パスは config から自動解決・タイムアウト自動計算）
const { prodPath, stgPath } = await remoteDb.sync();
```

---

## RemoteKijukuDBバックアップ操作

### `backup(label?: string, timeoutMs?: number): Promise<string>`

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

### `listBackups(): Promise<BackupInfo[]>`

リモートDBのバックアップ一覧を取得します。

**戻り値:** `Promise<BackupInfo[]>` - バックアップ情報の配列（作成日時の降順）

**使用例:**

```typescript
const backups = await remoteDb.listBackups();
backups.forEach(b => console.log(`${b.name} (${b.scope})`));
```

---

### `listPreStashes(): Promise<BackupInfo[]>`

リモートDBの pre-stash（即時復旧用ロールバックファイル）一覧を取得します（設計 [§8](../design/db-protection.md)）。`listBackups` は pre-stash を除外するため、promote/(b)操作が返す `preStashPath` を失った場合の発見経路として使います。

**戻り値:** `Promise<BackupInfo[]>` - pre-stash 情報の配列（id 降順）。`path` はそのまま `RemoteBackupSelector` の `{ type: 'byPath', path }` で `restore` に渡して prod を即時復旧できます。

**使用例:**

```typescript
const stashes = await remoteDb.listPreStashes();
// path を byPath で restore に渡し prod を即時復旧（§8）
if (stashes[0]) {
  await remoteDb.restore({ type: 'byPath', path: stashes[0].path });
}
```

---

### `restore(selector?: RemoteBackupSelector, timeoutMs?: number): Promise<string>`

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
  | { type: 'nth'; n: number }
  | { type: 'byId'; id: string }
  | { type: 'byPath'; path: string };
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


## RemoteKijukuDB stg運用（discard・diff・observe・promote）

prod/stg 運用のリモート版。処理はいずれもリモートホスト上の CLI に委譲され、NAS 上で完結する（SSH 経由で DB 実体は転送しない）。prod/stg パスは config（`dbPath`/`stgDbPath`）から自動解決する。ローカル版の詳細は [backup-sync.md](./backup-sync.md) を参照。

### `discard(timeoutMs?: number): Promise<{ prodPath: string; stgPath: string }>`

stg 破棄・再 sync（設計 §4.6）。書込セッション中断・observe gate 不合格時に stg を捨てて prod から再構築する。処理は `sync` と同一（既存 stg は上書き破棄・prod は一切触らない）。操作名のみ監査ログで区別するため独立メソッド。

---

### `diffWithBackup(params?: { selector?: RemoteBackupSelector; options?: { detail?: DiffDetail } }, timeoutMs?: number): Promise<BackupDiff>`

リモートのバックアップと現在DB（stg）の差分を取得します（`diffBackup` operation）。戻り値は [types.md](./types.md) の `BackupDiff`。

---

### `diffWithProd(params?: { prodDbPath?: string; options?: { detail?: DiffDetail } }, timeoutMs?: number): Promise<BackupDiff>`

prod(RO) と現在DB（stg）の差分を取得します（promote 判断用・`diffProdStg` operation）。`prodDbPath` 省略時は CLI 側で prod target のデフォルトパスを解決します。

---

### `observe(params?: { prodDbPath?: string; options?: ObserveOptions }, timeoutMs?: number): Promise<ObserveResult>`

stg と prod を比較し、機械的 promote gate を評価します（`observe` operation）。戻り値は [types.md](./types.md) の `ObserveResult`。

---

### `promote(params?: { prodDbPath?: string; options?: ObserveOptions; backupOpts?: BackupOptions }, timeoutMs?: number): Promise<PromoteOutcome>`

stg→prod へ反映します（`promote` operation）。gate 不合格時はリモートから error 文字列が返り、素の `Error` が throw される（ローカルの `PromoteGateFailedError` とは型が異なる・observe と同じ既存制約）。`backupOpts` は pre-stash 先のカスタマイズにそのまま渡される。

---

## RemoteKijukuDB バックアップメタ操作

### `setBackupLabel(id: string, label: string | undefined): Promise<void>`

リモートのバックアップにラベルを事後付与します（`undefined`で解除）。

---

### `setBackupNote(id: string, note: string | undefined): Promise<void>`

リモートのバックアップにメモを事後付与します（`undefined`で解除）。

---

### `getBackupMeta(id: string): Promise<BackupMetaEntry | null>`

リモートのバックアップの事後メタを取得します。

---

### `listAuditLogs(filter?: AuditLogFilter, timeoutMs?: number): Promise<AuditRecord[]>`

リモートの監査ログ（prod 側 `backup/meta/audit.log`）を取得します。フィルタ・戻り値はローカル `KijukuDB.listAuditLogs` と同じ（[`AuditRecord`](./types.md) 参照）。

---
