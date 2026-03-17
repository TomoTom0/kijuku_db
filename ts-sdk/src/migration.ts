/**
 * マイグレーション機能
 */
import { randomUUID } from 'node:crypto';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import type Database from 'better-sqlite3';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

/**
 * スキーマファイルのパスを取得
 */
function getSchemaPath(): string {
  // ts-sdk/src/ から ../../schema/schema.sql を参照
  return path.join(__dirname, '../../schema/schema.sql');
}

/**
 * マイグレーションを実行
 */
export function migrate(db: Database.Database): void {
  const currentVersion = getSchemaVersion(db);
  const targetVersion = 4;

  if (currentVersion === 0) {
    // 初回マイグレーション: schema.sqlを実行
    const schemaPath = getSchemaPath();
    if (!fs.existsSync(schemaPath)) {
      throw new Error(`Schema file not found: ${schemaPath}`);
    }
    const schemaSql = fs.readFileSync(schemaPath, 'utf-8');
    db.exec(schemaSql);
  } else {
    // 段階的マイグレーション
    for (let version = currentVersion + 1; version <= targetVersion; version++) {
      applyMigration(db, version);
    }
  }
}

/**
 * 特定バージョンへのマイグレーションを適用
 */
function applyMigration(db: Database.Database, version: number): void {
  switch (version) {
    case 2:
      // volume_numberカラムを削除（バージョン2では削除していた）
      db.exec(`
        -- volume_numberカラムを削除
        ALTER TABLE media DROP COLUMN volume_number;

        -- バージョンを記録
        INSERT OR IGNORE INTO schema_version (version) VALUES (2);
      `);
      break;
    case 3:
      // volume_numberカラムを再追加（保存時自動計算方式）
      db.exec(`
        -- volume_numberカラムを再追加
        ALTER TABLE media ADD COLUMN volume_number INTEGER;

        -- 既存データのvolume_numberを計算して設定
        UPDATE media
        SET volume_number = CAST(volume_text AS INTEGER)
        WHERE volume_text IS NOT NULL
          AND volume_text <> ''
          AND CAST(volume_text AS INTEGER) IS NOT NULL
          AND TRIM(volume_text) = CAST(CAST(volume_text AS INTEGER) AS TEXT);

        -- バージョンを記録
        INSERT OR IGNORE INTO schema_version (version) VALUES (3);
      `);
      break;
    case 4: {
      // uuid列をNOT NULL制約付きで追加するため、テーブルを再作成する

      // 1. 一時的にuuid列をNULLableで追加
      db.exec('ALTER TABLE media ADD COLUMN uuid TEXT;');

      // 2. 既存データにUUID v4を生成して設定（トランザクションでパフォーマンス改善）
      const ids = (db.prepare('SELECT id FROM media').all() as { id: number }[]);
      const updateStmt = db.prepare('UPDATE media SET uuid = ? WHERE id = ?');
      const updateMany = db.transaction((items: { id: number }[]) => {
        for (const item of items) {
          updateStmt.run(randomUUID(), item.id);
        }
      });
      updateMany(ids);

      // 3. テーブルを再作成してNOT NULL制約を付与（SQLiteではALTER TABLEでNOT NULL追加不可）
      db.exec(`
        PRAGMA foreign_keys = OFF;

        CREATE TABLE media_new (
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

        INSERT INTO media_new SELECT
          id, uuid, title, title_id, path, media_type, thumbnail_path,
          artist, artist_id, description, file_size, duration_sec,
          page_count, series, volume_number, volume_text, volume_title,
          magazine, magazine_id, language, source, external_id,
          artist_en, title_en, chapters, extension, flag_exist,
          created_at, updated_at, title_pron, artist_pron, series_pron
        FROM media;

        DROP TABLE media;
        ALTER TABLE media_new RENAME TO media;

        CREATE INDEX IF NOT EXISTS idx_media_title_id ON media(title_id);
        CREATE INDEX IF NOT EXISTS idx_media_artist_id ON media(artist_id);
        CREATE INDEX IF NOT EXISTS idx_media_media_type ON media(media_type);
        CREATE INDEX IF NOT EXISTS idx_media_series ON media(series);
        CREATE INDEX IF NOT EXISTS idx_media_source ON media(source);
        CREATE INDEX IF NOT EXISTS idx_media_type_created ON media(media_type, created_at DESC);

        CREATE TRIGGER IF NOT EXISTS update_media_timestamp
        AFTER UPDATE ON media
        FOR EACH ROW
        BEGIN
          UPDATE media SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
        END;

        PRAGMA foreign_keys = ON;
      `);

      db.exec('INSERT OR IGNORE INTO schema_version (version) VALUES (4);');
      break;
    }
    default:
      throw new Error(`Unknown migration version: ${version}`);
  }
}

/**
 * 現在のスキーマバージョンを取得
 */
export function getSchemaVersion(db: Database.Database): number {
  try {
    // schema_versionテーブルが存在するか確認
    const table = db
      .prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_version'"
      )
      .get() as { name: string } | undefined;

    if (!table) {
      return 0;
    }

    // 最新のバージョンを取得
    const row = db
      .prepare('SELECT MAX(version) as version FROM schema_version')
      .get() as { version: number | null };

    return row.version ?? 0;
  } catch (error) {
    return 0;
  }
}

/**
 * テーブル一覧を取得（テスト用）
 */
export function getTables(db: Database.Database): string[] {
  const rows = db
    .prepare(
      "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
    )
    .all() as { name: string }[];

  return rows.map((row) => row.name);
}

/**
 * 外部キー制約が有効化されているか確認（テスト用）
 */
export function isForeignKeysEnabled(db: Database.Database): boolean {
  const row = db.prepare('PRAGMA foreign_keys').get() as { foreign_keys: number };
  return row.foreign_keys === 1;
}
