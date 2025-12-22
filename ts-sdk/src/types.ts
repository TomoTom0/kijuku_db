/**
 * メディアタイプの定義
 */
export type MediaType = 'comic' | 'video' | 'music';

/**
 * メディア情報の完全な型定義
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

/**
 * メディア作成時の入力型
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
 * タグ情報
 */
export interface Tag {
  id: number;
  name: string;
}

/**
 * データベース接続オプション
 */
export interface DBOptions {
  timeout?: number;
  readonly?: boolean;
  verbose?: boolean;
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
