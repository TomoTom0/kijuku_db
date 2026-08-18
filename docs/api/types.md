# 型定義

TypeScript SDK の公開型定義。Rust SDK は対応する構造体（`Media`・`MediaFilter` 等）を持つ（[rust.md](./rust.md) 参照）。


## MediaType

```typescript
type MediaType = 'comic' | 'video' | 'music';
```

メディアの種類を表す型。

---

## Media

```typescript
interface Media {
  id: number;
  uuid: string;
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
```

データベースから取得されるメディア情報の完全な型。`volume_number`は`volume_text`（整数表記）から保存時に自動計算される（詳細: `DATABASE_SETUP.md` §5）。

---

## MediaInput

```typescript
interface MediaInput {
  title: string;
  media_type: MediaType;
  /** UUIDを手動指定する場合はここに設定。省略時は自動生成。 */
  uuid?: string;
  title_id?: string;
  path?: string;
  thumbnail_path?: string;
  artist?: string;
  artist_id?: string;
  description?: string;
  file_size?: number;
  duration_sec?: number;
  page_count?: number;
  series?: string;
  volume_number?: number;  // 非推奨: 手動設定は無視されます
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
  flag_exist?: boolean;
  title_pron?: string;
  artist_pron?: string;
  series_pron?: string;
}
```

メディア作成・更新時の入力型。`title`と`media_type`は必須。各フィールドの説明は[`createMedia`](./kijuku-db.md#createmediadata-mediainput-media)の表を参照。`volume_number`は`volume_text`（整数表記）から保存時に自動計算されるため、手動設定は無視される。

---

## MediaFilter

```typescript
interface MediaFilter {
  title?: string;
  title_id?: string;
  artist?: string;
  artist_id?: string;
  media_type?: MediaType;
  series?: string;
  source?: string;
  tag_ids?: number[];
  flag_exist?: boolean;
  language?: string;
  magazine?: string;
  magazine_id?: string;
  extension?: string;
  external_id?: string;
  volume_title?: string;
  title_en?: string;
  artist_en?: string;
  id_in?: number[];
  /** 除外IDのNOT IN句フィルタ（視聴済みIDなど少数のIDを除外する場合に使用） */
  exclude_ids?: number[];
  /** OR条件で結合する追加フィルタ。各フィルタ内の条件はAND結合、or_filters間はOR結合 */
  or_filters?: MediaFilter[];
}
```

メディア検索時のフィルタ条件。全てオプション。

---

## SortKey

```typescript
interface SortKey {
  field: string;
  order?: 'ASC' | 'DESC';
}
```

ソートキー（フィールドと方向）。

---

## QueryOptions

```typescript
interface QueryOptions {
  sortKeys?: SortKey[];
  limit?: number;
  offset?: number;
}
```

ソート・ページネーション設定。`sortKeys` に複数のキーを指定することで多段ソートが可能。

---

## BulkUpdateItem

```typescript
interface BulkUpdateItem {
  id: number;
  data: Partial<MediaInput>;
}
```

一括更新時の個別アイテム。

---

## Tag

```typescript
interface Tag {
  id: number;
  name: string;
}
```

タグ情報。

---

## DBOptions

```typescript
interface DBOptions {
  timeout?: number;
  readonly?: boolean;
  verbose?: boolean;
  /** undefined: デフォルトで有効 / null: バックアップ無効（マネージャーを生成しない） / BackupOptions: 指定の内容で有効 */
  backup?: BackupOptions | null;
  /** media root ディレクトリ（ファイル操作APIのサンドボックス境界）。このディレクトリ配下のみファイル操作（cp/mv/sync/upload/download 等）を許可し、外への脱出（`..`・絶対パス・シンボリックリンク経由）を拒否する。未設定の場合、ファイル操作APIはエラーで拒否される */
  mediaRoot?: string;
}
```

データベース接続オプション。

> **`:memory:` の既定挙動**: `backup` を未指定のまま `:memory:` で開くとバックアップ無効（`getBackupManager()` が `undefined`）で動作する。バックアップを使う場合は `backup: { backupDir: ... }` の明示指定が必要（未指定だとエラー）。

---

## FileOpOptions

ファイル操作（cp/mv/sync）のオプション。既定は `{ apply: false, updateDb: false }`（`defaultFileOpOptions`）。

| フィールド | 型 | 既定 | 説明 |
|-----------|----|----|------|
| `apply` | `boolean` | `false` | 実際に FS へ変更を適用するか。`false` なら dry-run で計画のみ返す |
| `updateDb` | `boolean` | `false` | DB の `Media.path` を追従させるか。※当面はフラグを受領するのみで DB 更新は未サポート（後続タスクで拡張） |

## FileOpResult

ファイル操作の実行結果（dry-run 含む）。

| フィールド | 型 | 説明 |
|-----------|----|------|
| `steps` | `FileOpStep[]` | 実行する（した）操作ステップ一覧 |
| `applied` | `boolean` | 実際に FS へ変更を加えたか |

## FileOpStep

個々の操作ステップ。判別共用体で、`kind` で判別する。

| `kind` | 追加フィールド | 説明 |
|--------|--------------|------|
| `copy` | `from`, `to` | コピー |
| `move` | `from`, `to` | 移動 |
| `trash` | `path`, `reason` | trash へ退避 |

## TrashOperation

trash 行きの原因となった操作の種別: `'delete'`（明示削除）/ `'overwrite'`（上書きで消える旧ファイル）/ `'sync_extra'`（sync で余分と判断されたファイル）。

## TrashEntry / TrashMeta

trash エントリ。`TrashEntry` は `{ id: string; meta: TrashMeta }`。`TrashMeta`（`.trash/<id>/.meta.json`）は `originalPath`（元パス・root 相対）/ `trashedAt`（RFC3339）/ `operation`（`TrashOperation`）/ `reason?`（呼び出し元識別）を持つ。

---

## BackupOptions

```typescript
interface BackupOptions {
  backupDir?: string;        // バックアップ保存先ディレクトリ（省略時: dbPathの親ディレクトリ/backup/）
  intervalMs?: number;       // 自動バックアップのトリガー間隔（ミリ秒・デフォルト: 3600000 = 1時間）
  enabled?: boolean;         // バックアップ機能全体の有効/無効（デフォルト: true）
  autoEnabled?: boolean;     // 自動バックアップの有効/無効
  tmpRetentionSecs?: number; // tmp/ の保持期間（秒）
  maxBackups?: number;       // 最大バックアップ数（retentionPolicy 未設定時のみ有効）
  maxAgeDays?: number;       // 最大保持日数（retentionPolicy 未設定時のみ有効）
  retentionPolicy?: RetentionPolicy;  // 保持ポリシー（ティア別）
  busyTimeoutMs?: number;    // DBロック時に1回の試行で待機する最大時間（ミリ秒）
  retryIntervalsMs?: number[];  // DBロック時のリトライ間隔（ミリ秒）のリスト。長さがリトライ回数を決定
  onProgress?: (info: { totalPages: number; remainingPages: number }) => void;  // 進捗コールバック
}

interface RetentionTier {
  maxAgeSecs: number;       // このティアが扱う最大経過秒数
  keepIntervalSecs: number; // このティア内で保持する間隔（秒）
}

interface RetentionPolicy { tiers: RetentionTier[]; }
```

自動バックアップ設定オプション。`retentionPolicy` 未設定時は `maxBackups`/`maxAgeDays` で制限する。

> **file-less DB（`:memory:`）の扱い**: `backupDir` の既定解決（DBパスの親 + `backup/`）はファイル実体のないDBではcwd依存となるため、`:memory:` では** `backupDir` の明示指定が必須**（未指定だとエラー）。出力先の無断cwd基準解決は行わない。`KijukuDB(':memory:')` は `backup` オプション未指定の場合バックアップ無効（マネージャーを生成しない）で動作する。Rust `KijukuDB::open(":memory:")` も同じ挙動（既定オプションならバックアップ無効・backup_dir を含む明示指定は `BackupManager` が検証する）。

**使用例:**

```typescript
const db = new KijukuDB('./data/kijuku.db', {
  backup: {
    backupDir: './backups',
    intervalMs: 1800000,  // 30分間隔
    onProgress: (info) => {
      console.log(`Backup progress: ${info.totalPages - info.remainingPages} / ${info.totalPages} pages completed`);
    }
  }
});
```

---

## AuditTarget

監査ログの対象DB。

```typescript
type AuditTarget = 'prod' | 'stg';
```

---

## AuditResult

監査ログの操作結果。

```typescript
type AuditResult = 'success' | 'failure' | 'dryRun';
```

---

## AuditRecord

監査ログレコード（`audit.log`の1行・JSONL）。

```typescript
interface AuditRecord {
  timestamp: string;           // ISO 8601 UTC（ミリ秒）
  operation: string;           // 操作種別（sync/discard/observe/diffProdStg/promote/b-restore/b-mediaMv/b-purgeTrash）
  target: AuditTarget;         // 対象DB（prod/stg）
  actor: string | null;        // 実行者（USER環境変数・取れなければnull）
  prodDbPath: string;          // prod DBの絶対パス
  result: AuditResult;         // 操作結果
  error: string | null;        // failure時のエラー文字列
  summary: object;             // 操作ごとの構造的サマリ（diff/gate結果・revision・preStashPath等）
}
```

**summaryフィールドの構造例:**

```typescript
// sync/discard
{ stgPath: string, revision: string }

// observe
{ passed: boolean, diffTotals: object, failedChecks: object }

// promote
{ observe: object, preStashPath: string }

// restore
{ backupPath: string, preRestorePath?: string }

// mediaMv/purgeTrash
{ affectedPaths: string[] }
```

---

## AuditLogFilter

監査ログのフィルタ条件。

```typescript
interface AuditLogFilter {
  operation?: string;   // 操作種別（完全一致）
  from?: string;        // 開始日時（ISO 8601・包含）
  to?: string;          // 終了日時（ISO 8601・包含）
  limit?: number;       // 上限件数（デフォルト: 1000・最大: 10000）
}
```

---

## TagUsageStats

```typescript
interface TagUsageStats {
  tag_id: number;
  tag_name: string;
  count: number;
}
```

タグごとの使用状況。

---

## MediaAttribute

```typescript
interface MediaAttribute {
  media_id: number;
  key: string;
  value?: string;
  value_type: 'string' | 'integer' | 'boolean';
}
```

メディアの拡張属性（EAVモデル）。

---

## BackupInfo

```typescript
interface BackupInfo {
  id: string;
  name: string;
  path: string;
  createdAt: Date;
  scope: BackupScope;
  kind: BackupKind;
  label?: string;         // ラベル（サイドカー優先、なければファイル名由来）
  labelSource?: LabelSource;  // ラベルの由来
  note?: string;          // メモ（サイドカー backup-meta.json 由来）
}
```

バックアップファイルのメタデータ。

---

## BackupScope

```typescript
type BackupScope = 'auto' | 'manual' | 'tmp';
```

- `auto`: 自動バックアップ
- `manual`: 手動バックアップ
- `tmp`: 一時バックアップ（リストア時の退避等）

---

## BackupKind

```typescript
type BackupKind =
  | { type: 'full' }
  | { type: 'diff'; baseId: string };  // 基底フルバックアップのID
```

- `full`: 完全バックアップ
- `diff`: 差分バックアップ（`baseId` に基底フルのIDを持つ）

---

## BackupSelector

バックアップ選択のためのユーティリティクラス。

```typescript
class BackupSelector {
  static latest(): BackupSelector;
  static nth(n: number): BackupSelector;
  static before(date: Date): BackupSelector;
  static after(date: Date): BackupSelector;
  static closestTo(date: Date): BackupSelector;
  static byId(id: string): BackupSelector;      // タイムスタンプ文字列で直接指定
  static byPath(path: string): BackupSelector;  // 既知パスで直接指定（pre-stash 戻し等・設計 §8）
  scope(scope: BackupScope): BackupSelector;    // スコープ限定
}
```

**使用例:**

```typescript
import { BackupSelector } from 'kijuku-db';

db.restore(BackupSelector.latest());
db.restore(BackupSelector.nth(1));          // 2番目に新しい
db.restore(BackupSelector.byId('20260816-120000'));  // IDで直接指定
```

---

## RemoteBackupSelector

```typescript
type RemoteBackupSelector =
  | { type: 'latest' }
  | { type: 'nth'; n: number }
  | { type: 'byId'; id: string };
```

リモートバックアップの選択条件。

---

## BackupDiff / DiffOptions（バックアップ差分）

`diffWithBackup(selector, options?)` が返す、現在DBとバックアップの差分。

```typescript
type DiffDetail =
  | { type: 'summaryOnly' }
  | { type: 'limited'; n: number }
  | { type: 'full' };

interface DiffOptions {
  detail?: DiffDetail;  // 省略時 limited{n: 100}
}

interface DiffCounts { added: number; removed: number; changed: number; }

interface BackupDiff {
  media:       { added: Media[]; removed: Media[]; changed: { current: Media; backup: Media }[] };
  tags:        { added: Tag[]; removed: Tag[]; changed: { current: Tag; backup: Tag }[] };
  mediaTags:   { added: MediaTagAssoc[]; removed: MediaTagAssoc[] };
  attributes:  { added: MediaAttribute[]; removed: MediaAttribute[]; changed: { current: MediaAttribute; backup: MediaAttribute }[] };
  hashes:      { added: MediaHash[]; removed: MediaHash[]; changed: { current: MediaHash; backup: MediaHash }[] };
  summary:     { media: DiffCounts; tags: DiffCounts; mediaTags: DiffCounts; attributes: DiffCounts; hashes: DiffCounts };
}
```

- `added`: バックアップに在り現在に無い（復元で復活）
- `removed`: 現在に在りバックアップに無い（復元で失われる）
- `changed`: 両方に在り内容が異なる（復元で上書き）

---

## 差分サマリ（ProdStgDiffSummary）

`observe` の gate 評価に使う `BackupDiff` の要約（`summarizeDiff` の出力・クライアント側純粋計算）。

```typescript
interface DiffTotals { added: number; removed: number; changed: number; }

interface BackupDiffSummary {
  media: DiffCounts;
  tags: DiffCounts;
  mediaTags: DiffCounts;  // 紐付けは一致/不一致のみ（changed は常に 0）
  attributes: DiffCounts;
  hashes: DiffCounts;
}

interface DiffDistribution {
  media: MediaDistribution;             // byType / byArtistTop / byFlagExist
  mediaTags: MediaTagAssocDistribution; // byTagTop
  attributes: AttributeDistribution;    // byKey
  hashes: HashDistribution;             // byFilenameTop
}

interface ProdStgDiffSummary {
  counts: BackupDiffSummary;      // 5テーブル別の件数
  totals: DiffTotals;             // 全テーブル横断の合計（gate しきい値比較用）
  distribution: DiffDistribution; // テーブル別の偏り（異常な一括変更の検出）
}
```

---

## promote gate（observe・機械的 gate）

`observe()` / `promote()` の gate 設定と結果（設計 §3.4・Rust `diff.rs` と parity）。

```typescript
interface GoldenAssertion {
  name: string;
  sql: string;          // 最初のカラム・最初の行を件数として実行
  expectedMin?: number;
  expectedMax?: number;
}

type GoldenResult = { ok: true; count: number } | { ok: false; error: string };

interface GateConfig {
  maxAdded: number;    // promote で prod に追加される行数の上限（totals.added）
  maxRemoved: number;  // promote で prod から削除される行数の上限（totals.removed・削除は最も危険なので厳しめ）
  maxChanged: number;  // promote で prod が上書きされる行数の上限（totals.changed）
  goldenAssertions: GoldenAssertion[];  // 運用者定義のドメイン不変条件（デフォルト空）
}

interface GateCheck { name: string; passed: boolean; detail: string; }

interface ObserveResult {
  passed: boolean;             // 全 gate check 合格か（promote 可否の客観判定）
  prodSchemaVersion: number;
  stgSchemaVersion: number;
  summary: ProdStgDiffSummary; // 差分要約（gate の素材）
  checks: GateCheck[];
}

interface ObserveOptions {
  diffOptions?: DiffOptions;
  gateConfig?: GateConfig;
}

interface PromoteOutcome {
  observe: ObserveResult;  // gate 評価結果（全 gate 合格）
  preStashPath?: string;   // §8 即時復旧の戻し先（prod がファイル実体を持たない場合は undefined）
}
```

gate 不合格時、ローカル `promote()` は `PromoteGateFailedError` を throw する（詳細: [backup-sync.md](./backup-sync.md)）。

---

## BackupMetaEntry / LabelSource（事後ラベル/メモ）

```typescript
type LabelSource = 'filename' | 'sidecar';

interface BackupMetaEntry {
  id: string;
  label?: string;
  note?: string;
  updatedAt: string;  // ISO8601
}
```

`setBackupLabel(id, label?)` / `setBackupNote(id, note?)` / `getBackupMeta(id)` で事後付与。
`BackupInfo` は `labelSource` と `note` を追加で返す（サイドカー優先マージ、省略時 `labelSource='filename'`）。

---

## ThumbnailOptions

```typescript
interface ThumbnailOptions {
  dry_run?: boolean;   // DBを更新せず結果を出力のみ
  force?: boolean;     // 既存サムネイルを強制再生成
}
```

サムネイル生成オプション。

---

## CheckThumbnailResult

```typescript
interface CheckThumbnailResult {
  total: number;
  ok: number;
  missing: number;
  file_not_found: number;
  skipped: number;
  details: CheckThumbnailItemResult[];
}

interface CheckThumbnailItemResult {
  id: number;
  title: string;
  path?: string;
  thumbnail_path?: string;
  status: 'ok' | 'missing' | 'file_not_found' | 'skipped';
}
```

---

## UpdateThumbnailResult

```typescript
interface UpdateThumbnailResult {
  total: number;
  generated: number;
  already_exists: number;
  skipped: number;
  errors: number;
  details: UpdateThumbnailItemResult[];
}

interface UpdateThumbnailItemResult {
  id: number;
  title: string;
  path?: string;
  thumbnail_path?: string;
  status: 'generated' | 'already_exists' | 'skipped' | 'error';
  error?: string;
}
```

---

## UpdateExistOptions

```typescript
interface UpdateExistOptions {
  dry_run?: boolean;   // DBを更新せず結果のみを返す
}
```

---

## UpdateExistResult

```typescript
interface UpdateExistResult {
  total: number;
  updated: number;
  updated_ids: number[] | null;
  updated_ids_file: string | null;
  detail_file: string;
}
```

---

## MediaHash

```typescript
interface MediaHash {
  item_uuid: string;
  filename: string;
  time_range: string;
  content_hash: Uint8Array;
  alternative_of?: string;
  embedding?: Uint8Array;
  created_at: string;
  updated_at: string;
}
```

## MediaHashInput

```typescript
interface MediaHashInput {
  item_uuid: string;
  filename: string;
  time_range: string;
  content_hash: Uint8Array;
  alternative_of?: string;
}
```

## ComputeHashResult

```typescript
interface ComputeHashResult {
  item_uuid: string;
  hashes: MediaHash[];
  skipped: boolean;
  skip_reason?: string;
}
```

---

## TableColumnInfo

```typescript
interface TableColumnInfo {
  cid: number;
  name: string;
  type: string;
  notnull: boolean;
  default_value: string | null;
  pk: number;
}
```

テーブルカラムのメタデータ。

---

## RemoteConfig

```typescript
interface RemoteConfig {
  sshHost: string;       // .ssh/configのHost名（必須）
  dbPath?: string;       // リモートの prod DB パス（デフォルト: ~/.local/share/kijuku/kijuku.db）
  stgDbPath?: string;    // リモートの stg DB パス（未指定時は dbPath から <stem>.stg.db を導出）
  binaryPath?: string;   // バイナリパス（デフォルト: ~/.local/bin/kijuku-cli）
  port?: number;         // SSHポート（省略時はSSH設定から読み取り）
  mediaRoot?: string;    // リモートホスト上の media root（ファイル操作APIのサンドボックス境界）
  target?: 'prod' | 'stg';  // 操作対象DB（デフォルト: stg・prod は readonly + migrate skip）
  workDir?: string;      // 作業ディレクトリ（将来の拡張用）
}
```

SSH経由のリモートDB接続設定。

---

