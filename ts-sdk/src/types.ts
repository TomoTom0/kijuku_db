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
