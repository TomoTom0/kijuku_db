-- kijuku_db スキーマ定義
-- Version: 5

-- 外部キー制約を有効化
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

-- 初期バージョンを記録
INSERT OR IGNORE INTO schema_version (version) VALUES (1);
INSERT OR IGNORE INTO schema_version (version) VALUES (2);
INSERT OR IGNORE INTO schema_version (version) VALUES (3);
INSERT OR IGNORE INTO schema_version (version) VALUES (4);
INSERT OR IGNORE INTO schema_version (version) VALUES (5);
