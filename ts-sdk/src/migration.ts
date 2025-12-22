/**
 * マイグレーション機能
 */
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
  const schemaPath = getSchemaPath();

  if (!fs.existsSync(schemaPath)) {
    throw new Error(`Schema file not found: ${schemaPath}`);
  }

  const schemaSql = fs.readFileSync(schemaPath, 'utf-8');

  // スキーマSQLを実行
  db.exec(schemaSql);
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
