-- kijuku_db スキーマ定義（Cloudflare D1 用）
-- Version: 6
--
-- D1 REST API は params を全て TEXT として扱い BLOB 型カラムにバイナリを格納できないため、
-- content_hash / embedding を TEXT（hex 文字列）で保持する。それ以外は schema.sql と同一。
--   - content_hash: BLOB(32byte) → TEXT(64文字 hex)、CHECK は length=64
--   - embedding:   BLOB          → TEXT（NULLable、hex or NULL）

-- 外部キー制約を有効化（D1 は ON 固定だが ON 設定は許容される・実証済み）
PRAGMA foreign_keys = ON;

-- スキーマバージョン管理テーブル
CREATE TABLE IF NOT EXISTS schema_version (
  version INTEGER PRIMARY KEY,
  applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- タグ定義テーブル
CREATE TABLE IF NOT EXISTS tags (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE
);

-- メディア情報テーブル
CREATE TABLE IF NOT EXISTS media (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uuid TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  title_id TEXT,
  path TEXT UNIQUE,
  media_type TEXT NOT NULL CHECK(media_type IN ('comic', 'video', 'music')),
  thumbnail_path TEXT,
  artist TEXT,
  artist_id TEXT,
  description TEXT,
  file_size INTEGER,
  duration_sec INTEGER,
  page_count INTEGER,
  series TEXT,
  volume_number INTEGER,
  volume_text TEXT,
  volume_title TEXT,
  magazine TEXT,
  magazine_id TEXT,
  language TEXT,
  source TEXT,
  external_id TEXT,
  artist_en TEXT,
  title_en TEXT,
  chapters TEXT,
  extension TEXT,
  flag_exist INTEGER DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  title_pron TEXT,
  artist_pron TEXT,
  series_pron TEXT
);

-- メディアとタグの多対多リレーション
CREATE TABLE IF NOT EXISTS media_tags (
  media_id INTEGER NOT NULL,
  tag_id INTEGER NOT NULL,
  PRIMARY KEY (media_id, tag_id),
  FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
  FOREIGN KEY (tag_id) REFERENCES tags(id)
);

-- 追加属性テーブル（EAVモデル）
CREATE TABLE IF NOT EXISTS media_attributes (
  media_id INTEGER NOT NULL,
  key TEXT NOT NULL,
  value TEXT,
  value_type TEXT,
  PRIMARY KEY (media_id, key),
  FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE
);

-- インデックス: ID検索用（完全一致）
CREATE INDEX IF NOT EXISTS idx_media_title_id ON media(title_id);
CREATE INDEX IF NOT EXISTS idx_media_artist_id ON media(artist_id);

-- インデックス: メディアタイプフィルタ
CREATE INDEX IF NOT EXISTS idx_media_media_type ON media(media_type);

-- インデックス: シリーズ検索
CREATE INDEX IF NOT EXISTS idx_media_series ON media(series);

-- インデックス: データソース検索
CREATE INDEX IF NOT EXISTS idx_media_source ON media(source);

-- インデックス: 複合インデックス（メディアタイプ×作成日時）
CREATE INDEX IF NOT EXISTS idx_media_type_created ON media(media_type, created_at DESC);

-- インデックス: タグ検索用
CREATE INDEX IF NOT EXISTS idx_media_tags_tag_id ON media_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_media_tags_media_id ON media_tags(media_id);

-- トリガー: updated_atの自動更新
CREATE TRIGGER IF NOT EXISTS update_media_timestamp
AFTER UPDATE ON media
FOR EACH ROW
BEGIN
  UPDATE media SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

-- メディアハッシュテーブル（D1 用: content_hash/embedding は TEXT(hex)）
CREATE TABLE IF NOT EXISTS media_hashes (
  item_uuid       TEXT NOT NULL,
  filename        TEXT NOT NULL DEFAULT '',
  time_range      TEXT NOT NULL DEFAULT '',
  content_hash    TEXT NOT NULL CHECK(length(content_hash) = 64),
  alternative_of  TEXT,
  embedding       TEXT,
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (item_uuid, filename, time_range),
  FOREIGN KEY (item_uuid) REFERENCES media(uuid) ON DELETE CASCADE
);

-- インデックス: ハッシュ検索用
CREATE INDEX IF NOT EXISTS idx_media_hashes_content ON media_hashes(content_hash);

-- トリガー: media_hashesのupdated_at自動更新
CREATE TRIGGER IF NOT EXISTS update_media_hashes_timestamp
AFTER UPDATE ON media_hashes
FOR EACH ROW
BEGIN
  UPDATE media_hashes SET updated_at = datetime('now')
  WHERE item_uuid = NEW.item_uuid AND filename = NEW.filename AND time_range = NEW.time_range;
END;

-- 初期バージョンを記録
INSERT OR IGNORE INTO schema_version (version) VALUES (6);
