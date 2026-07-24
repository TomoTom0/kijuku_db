/**
 * メディアタイプの定義
 */
export type MediaType = 'comic' | 'video' | 'music';

/**
 * メディア情報の完全な型定義
 *
 * 注意: volume_numberは自動計算されます。
 * 保存時にvolume_textが整数なら、自動的にvolume_numberに設定されます。
 */
export interface Media {
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
  volume_number?: number;  // volume_textから自動計算（ソート・フィルタ可能）
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

/**
 * メディア作成時の入力型
 *
 * 注意: volume_numberは自動計算されるため、手動設定は無視されます。
 * volume_textに整数を設定すると、保存時に自動的にvolume_numberが計算されます。
 */
export interface MediaInput {
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

/**
 * メディア検索時のフィルタ条件
 */
export interface MediaFilter {
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
  /** IDのIN句フィルタ（複数IDを一括フェッチする場合に使用） */
  id_in?: number[];
  /** 除外IDのNOT IN句フィルタ（視聴済みIDなど少数のIDを除外する場合に使用） */
  exclude_ids?: number[];
  /**
   * OR条件で結合する追加フィルタ
   * 各フィルタ内の条件はAND結合、or_filters間はOR結合される
   */
  or_filters?: MediaFilter[];
}

/**
 * ソートキー（フィールドと方向）
 */
export interface SortKey {
  field: string;
  order?: 'ASC' | 'DESC';
}

/**
 * クエリオプション（ソート、ページネーション）
 */
export interface QueryOptions {
  sortKeys?: SortKey[];
  limit?: number;
  offset?: number;
}

/**
 * 一括更新時の個別アイテム
 */
export interface BulkUpdateItem {
  id: number;
  data: Partial<MediaInput>;
}

/**
 * タグ情報
 */
export interface Tag {
  id: number;
  name: string;
}

/**
 * タグ使用統計情報
 */
export interface TagUsageStats {
  tag_id: number;
  tag_name: string;
  count: number;
}

/**
 * バックアップ設定オプション
 */
export interface BackupOptions {
  /**
   * バックアップファイルの保存先ディレクトリ
   * 省略時はdbPathの親ディレクトリに"backup"フォルダを作成
   */
  backupDir?: string;
  intervalMs?: number;
  enabled?: boolean;
  onProgress?: (info: { totalPages: number; remainingPages: number }) => void;
}

/**
 * サムネイル操作オプション
 */
export interface ThumbnailOptions {
  /** trueの場合、DBを更新せず結果を出力のみ（updateThumbnailのみ有効） */
  dry_run?: boolean;
  /** trueの場合、既にサムネイルが存在しても再生成する（updateThumbnailのみ有効） */
  force?: boolean;
}

export type CheckThumbnailStatus =
  | { type: 'ok' }
  | { type: 'skipped'; reason: string }
  | { type: 'missing' }
  | { type: 'fileNotFound' };

export interface CheckThumbnailItemResult {
  id: number;
  uuid: string;
  title: string;
  expected_path?: string;
  current_path?: string;
  status: CheckThumbnailStatus;
}

export interface CheckThumbnailResult {
  total: number;
  ok: number;
  missing: number;
  file_not_found: number;
  skipped: number;
  details: CheckThumbnailItemResult[];
}

export type UpdateThumbnailStatus =
  | { type: 'generated' }
  | { type: 'alreadyExists' }
  | { type: 'skipped'; reason: string }
  | { type: 'error'; message: string };

export interface UpdateThumbnailItemResult {
  id: number;
  uuid: string;
  title: string;
  thumbnail_path?: string;
  status: UpdateThumbnailStatus;
}

export interface UpdateThumbnailResult {
  total: number;
  generated: number;
  already_exists: number;
  skipped: number;
  errors: number;
  details: UpdateThumbnailItemResult[];
}

/**
 * データベース接続オプション
 */
export interface DBOptions {
  timeout?: number;
  readonly?: boolean;
  verbose?: boolean;
  /**
   * バックアップ設定。
   * - undefined: デフォルトで有効（既定の BackupOptions）
   * - null: バックアップ無効（マネージャーを生成しない）
   * - BackupOptions: 指定の内容で有効
   */
  backup?: BackupOptions | null;
  /**
   * media root ディレクトリ（ファイル操作APIのサンドボックス境界）。
   *
   * このディレクトリ配下のみファイル操作（cp/mv/sync/upload/download 等）を許可し、
   * 外への脱出（`..`・絶対パス・シンボリックリンク経由）を拒否する。
   * 未設定（undefined）の場合、ファイル操作APIはエラーで拒否される。
   */
  mediaRoot?: string;
}

/**
 * メディア属性（EAVモデル）
 */
export interface MediaAttribute {
  media_id: number;
  key: string;
  value?: string;
  value_type: 'string' | 'integer' | 'boolean';
}

/**
 * テーブルカラム情報
 */
export interface TableColumnInfo {
  cid: number;
  name: string;
  type_name: string;
  notnull: boolean;
  dflt_value?: string;
  pk: boolean;
}

/**
 * メディアハッシュ情報
 */
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

/**
 * メディアハッシュ登録時の入力型
 */
export interface MediaHashInput {
  item_uuid: string;
  filename: string;
  time_range: string;
  content_hash: Uint8Array;
  alternative_of?: string;
}

/**
 * ハッシュ計算結果
 */
export interface ComputeHashResult {
  item_uuid: string;
  hashes: MediaHash[];
  skipped: boolean;
  skip_reason?: string;
}

/**
 * メディアとタグの紐付け（media_tags テーブル対応、DBカラム準拠）
 */
export interface MediaTagAssoc {
  media_id: number;
  tag_id: number;
}

// ========== バックアップ差分（diff）==========
// Rust 側 serde(rename_all = camelCase) の JSON と一致させるため camelCase。

/** 差分件数 */
export interface DiffCounts {
  added: number;
  removed: number;
  changed: number;
}

/** 差分の詳細度（省略時 limited{n:100}） */
export type DiffDetail =
  | { type: 'summaryOnly' }
  | { type: 'limited'; n: number }
  | { type: 'full' };

/** 差分取得オプション */
export interface DiffOptions {
  detail?: DiffDetail;
}

export interface MediaChange {
  current: Media;
  backup: Media;
}
export interface TagChange {
  current: Tag;
  backup: Tag;
}
export interface AttributeChange {
  current: MediaAttribute;
  backup: MediaAttribute;
}
export interface HashChange {
  current: MediaHash;
  backup: MediaHash;
}

export interface MediaDiff {
  added: Media[];
  removed: Media[];
  changed: MediaChange[];
}
export interface TagDiff {
  added: Tag[];
  removed: Tag[];
  changed: TagChange[];
}
export interface MediaTagAssocDiff {
  added: MediaTagAssoc[];
  removed: MediaTagAssoc[];
}
export interface AttributeDiff {
  added: MediaAttribute[];
  removed: MediaAttribute[];
  changed: AttributeChange[];
}
export interface HashDiff {
  added: MediaHash[];
  removed: MediaHash[];
  changed: HashChange[];
}

export interface BackupDiffSummary {
  media: DiffCounts;
  tags: DiffCounts;
  /** 紐付けは一致/不一致のみ（changed は常に 0） */
  mediaTags: DiffCounts;
  attributes: DiffCounts;
  hashes: DiffCounts;
}

/**
 * バックアップと現在DBの差分
 *
 * - added:   バックアップに在り現在に無い（復元で復活する）
 * - removed: 現在に在りバックアップに無い（復元で失われる）
 * - changed: 両方に在り内容が異なる（復元で上書きされる）
 */
export interface BackupDiff {
  media: MediaDiff;
  tags: TagDiff;
  mediaTags: MediaTagAssocDiff;
  attributes: AttributeDiff;
  hashes: HashDiff;
  summary: BackupDiffSummary;
}

// ---------- prod/stg 差分の要約・LLM explanation prompt（設計 §4.4/§15-15・TASK-53）----------
// これらは BackupDiff（stg 編集視点）を入力に取るクライアント側の純粋計算結果。
// エンティティ型（Media 等）は Rust serde と一致する snake_case、
// SDK 構造型は BackupDiff 等に倣い camelCase で定義する。

/** 全テーブル横断の合計件数（gate しきい値比較用） */
export interface DiffTotals {
  added: number;
  removed: number;
  changed: number;
}

/** media テーブルの分布 */
export interface MediaDistribution {
  /** media_type 別の件数（"comic"/"video"/"music" ...） */
  byType: Record<string, DiffCounts>;
  /** artist 別上位 N（None は "unknown"）。変動件数（total）降順 */
  byArtistTop: Array<{ artist: string; counts: DiffCounts }>;
  /** flag_exist 別の件数（"true"/"false"）。一括存在フラグ変更の検出 */
  byFlagExist: Record<string, DiffCounts>;
}

/** media_tags（紐付け）の分布。tag_id 別上位 N */
export interface MediaTagAssocDistribution {
  byTagTop: Array<{ tagId: number; counts: DiffCounts }>;
}

/** attributes（EAV）の分布。key 別の件数 */
export interface AttributeDistribution {
  byKey: Record<string, DiffCounts>;
}

/** hashes の分布。filename 別上位 N */
export interface HashDistribution {
  byFilenameTop: Array<{ filename: string; counts: DiffCounts }>;
}

/** テーブル別の偏り（異常な一括変更の検出） */
export interface DiffDistribution {
  media: MediaDistribution;
  mediaTags: MediaTagAssocDistribution;
  attributes: AttributeDistribution;
  hashes: HashDistribution;
}

/** BackupDiff（stg 編集視点）の要約。機械的 gate（TASK-54）や LLM explanation prompt の素材 */
export interface ProdStgDiffSummary {
  /** 5テーブル別の件数（既存 BackupDiffSummary を再利用） */
  counts: BackupDiffSummary;
  /** 全テーブル横断の合計 */
  totals: DiffTotals;
  /** テーブル別の偏り */
  distribution: DiffDistribution;
}

/** buildDiffExplanationPrompt のオプション（既定値は defaultDiffExplanationPromptOptions） */
export interface DiffExplanationPromptOptions {
  /** 各セクションの代表サンプル最大件数（token 節約・既定 10） */
  maxSamplesPerSection: number;
  /** 分布セクションを含めるか（既定 true） */
  includeDistribution: boolean;
  /** 呼出側の任意メタ（session ID 等）。プロンプト先頭に記載 */
  extraContext?: string;
}

// ---------- 機械的 promote gate（observe・設計 §3.4/§15-10・TASK-54）----------
// Rust diff.rs の GateConfig/GoldenAssertion/GoldenResult/GateCheck/ObserveResult/ObserveOptions と parity。

/** golden assertion（運用者定義のドメイン不変条件・設計 §3.4）。
 *  `sql` の最初のカラム・最初の行を件数として実行し、`expectedMin`/`expectedMax` の範囲内なら合格。 */
export interface GoldenAssertion {
  name: string;
  sql: string;
  expectedMin?: number;
  expectedMax?: number;
}

/** golden assertion の SQL 実行結果（evaluateGate に渡す・純粋性担保・wire 非対象）。
 *  SQL エラー時は `ok:false` とし gate を FAIL 扱いにする（安全側）。 */
export type GoldenResult =
  | { ok: true; count: number }
  | { ok: false; error: string };

/** gate 設定（設計 §3.4/§15-10）。閾値は `ProdStgDiffSummary.totals` と比較する。 */
export interface GateConfig {
  /** promote で prod に追加される行数の上限（`totals.added`） */
  maxAdded: number;
  /** promote で prod から削除される行数の上限（`totals.removed`・削除は最も危険なので厳しめ） */
  maxRemoved: number;
  /** promote で prod が上書きされる行数の上限（`totals.changed`） */
  maxChanged: number;
  /** 運用者定義の golden assertion（デフォルト空） */
  goldenAssertions: GoldenAssertion[];
}

/** 個別 gate 検査の結果 */
export interface GateCheck {
  name: string;
  passed: boolean;
  detail: string;
}

/** observe の結果（設計 §3.4/§4.4）。`passed` は全 gate check 合格か（promote 可否の客観判定）。 */
export interface ObserveResult {
  passed: boolean;
  prodSchemaVersion: number;
  stgSchemaVersion: number;
  /** 差分要約（gate の素材・`summarizeDiff` の出力） */
  summary: ProdStgDiffSummary;
  /** 各 gate 検査の結果 */
  checks: GateCheck[];
}

/** observe のオプション（設計 §4.4）。差分取得と gate 評価の設定を束ねる。 */
export interface ObserveOptions {
  diffOptions?: DiffOptions;
  gateConfig?: GateConfig;
}

/** promote（stg→prod 反映）の結果（設計 §4.5・TASK-58）。Rust `PromoteOutcome` と parity。
 *  gate 合格時のみ返る（不合格時は `PromoteGateFailedError`）。 */
export interface PromoteOutcome {
  /** gate 評価結果（全 gate 合格） */
  observe: ObserveResult;
  /** pre-stash パス（§8 即時復旧の戻し先・`tmp/{stem}.{ts}-pre_promote.db`）。
   *  prod がファイル実体を持たない（`:memory:` 等）場合は undefined。 */
  preStashPath?: string;
}
