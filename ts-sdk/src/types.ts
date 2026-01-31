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
}

/**
 * クエリオプション（ソート、ページネーション）
 */
export interface QueryOptions {
  orderBy?: string;
  order?: 'ASC' | 'DESC';
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
 * データベース接続オプション
 */
export interface DBOptions {
  timeout?: number;
  readonly?: boolean;
  verbose?: boolean;
  backup?: BackupOptions;
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
