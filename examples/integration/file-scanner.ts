/**
 * ファイルスキャナーとの統合サンプル
 *
 * ディレクトリをスキャンして、メディアファイルをデータベースに登録します。
 */

import { KijukuDB } from '../../ts-sdk/src/index';
import { readdirSync, statSync } from 'fs';
import { join, extname, basename } from 'path';

// スキャン対象のメディアタイプとその拡張子のマッピング
const MEDIA_EXTENSIONS = {
  comic: ['.cbz', '.cbr', '.zip', '.rar', '.pdf', '.epub'],
  video: ['.mp4', '.mkv', '.avi', '.mov', '.wmv', '.flv', '.webm'],
  music: ['.mp3', '.flac', '.wav', '.aac', '.m4a', '.ogg', '.wma'],
} as const;

interface ScanOptions {
  recursive?: boolean;
  skipExisting?: boolean;
  verbose?: boolean;
}

/**
 * ファイルパスからメディアタイプを判定
 */
function detectMediaType(filePath: string): 'comic' | 'video' | 'music' | null {
  const ext = extname(filePath).toLowerCase();

  for (const [type, extensions] of Object.entries(MEDIA_EXTENSIONS)) {
    if (extensions.includes(ext as any)) {
      return type as 'comic' | 'video' | 'music';
    }
  }

  return null;
}

/**
 * ファイル名からタイトルを抽出
 * 例: "sample_comic_vol01.cbz" -> "sample comic vol01"
 */
function extractTitle(filePath: string): string {
  const fileName = basename(filePath, extname(filePath));
  return fileName.replace(/_/g, ' ').replace(/\./g, ' ').trim();
}

/**
 * ディレクトリをスキャンしてメディアファイルを登録
 */
export function scanDirectory(
  db: KijukuDB,
  directoryPath: string,
  options: ScanOptions = {}
): { added: number; skipped: number; errors: number } {
  const { recursive = true, skipExisting = true, verbose = false } = options;

  let added = 0;
  let skipped = 0;
  let errors = 0;

  function scanDir(dirPath: string) {
    try {
      const entries = readdirSync(dirPath);

      for (const entry of entries) {
        const fullPath = join(dirPath, entry);
        const stats = statSync(fullPath);

        if (stats.isDirectory() && recursive) {
          scanDir(fullPath);
        } else if (stats.isFile()) {
          const mediaType = detectMediaType(fullPath);

          if (!mediaType) {
            continue; // メディアファイルではない
          }

          // 既存チェック
          if (skipExisting) {
            const existing = db.findMedia({ path: fullPath });
            if (existing.length > 0) {
              if (verbose) {
                console.log(`スキップ: ${fullPath} (既に登録済み)`);
              }
              skipped++;
              continue;
            }
          }

          try {
            const title = extractTitle(fullPath);

            db.createMedia({
              title,
              path: fullPath,
              media_type: mediaType,
              file_size: stats.size,
              extension: extname(fullPath),
              flag_exist: true,
            });

            if (verbose) {
              console.log(`追加: ${title} (${mediaType})`);
            }

            added++;
          } catch (error: any) {
            console.error(`エラー: ${fullPath} - ${error.message}`);
            errors++;
          }
        }
      }
    } catch (error: any) {
      console.error(`ディレクトリスキャンエラー: ${dirPath} - ${error.message}`);
      errors++;
    }
  }

  scanDir(directoryPath);

  return { added, skipped, errors };
}

// サンプル実行
if (import.meta.url === `file://${process.argv[1]}`) {
  const dbPath = process.argv[2] || './media.db';
  const scanPath = process.argv[3] || './sample-media';

  console.log('ファイルスキャナー統合サンプル');
  console.log('================================\n');
  console.log(`データベース: ${dbPath}`);
  console.log(`スキャン対象: ${scanPath}\n`);

  const db = new KijukuDB(dbPath);
  db.migrate();

  const result = scanDirectory(db, scanPath, {
    recursive: true,
    skipExisting: true,
    verbose: true,
  });

  console.log('\nスキャン完了:');
  console.log(`  追加: ${result.added}件`);
  console.log(`  スキップ: ${result.skipped}件`);
  console.log(`  エラー: ${result.errors}件`);

  db.close();
}
