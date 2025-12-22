#!/usr/bin/env node
/**
 * Kijuku DB CLI ツール
 */
import { KijukuDB, RemoteKijukuDB } from './index.js';
import fs from 'fs';
import path from 'path';
import { parse } from 'csv-parse/sync';

const COMMANDS = {
  migrate: 'データベースのマイグレーションを実行',
  search: 'メディアを検索',
  import: 'JSON/CSV/TSVファイルからメディアをインポート',
  help: 'ヘルプを表示',
};

// mediaテーブルの標準カラム（追加属性として扱わないカラム）
const STANDARD_MEDIA_COLUMNS = new Set([
  'id', 'title', 'title_id', 'path', 'media_type', 'thumbnail_path',
  'artist', 'artist_id', 'description', 'file_size', 'duration_sec',
  'page_count', 'series', 'volume_number', 'volume_text', 'volume_title',
  'magazine', 'magazine_id', 'language', 'source', 'external_id',
  'artist_en', 'title_en', 'chapters', 'extension', 'flag_exist',
  'created_at', 'updated_at', 'title_pron', 'artist_pron', 'series_pron',
]);

/**
 * ヘルプメッセージを表示
 */
function showHelp(): void {
  console.log('Kijuku DB CLI');
  console.log('');
  console.log('使い方:');
  console.log('  kijuku-cli <command> [options]');
  console.log('');
  console.log('コマンド:');
  Object.entries(COMMANDS).forEach(([cmd, desc]) => {
    console.log(`  ${cmd.padEnd(15)} ${desc}`);
  });
  console.log('');
  console.log('オプション:');
  console.log('  --db <path>              データベースファイルのパス（デフォルト: ./kijuku.db）');
  console.log('                           リモートDB: host:path 形式（例: as5202:/home/user/kijuku.db）');
  console.log('  --additional-columns <cols>  追加カラムのリスト（カンマ区切り、importコマンドのみ）');
  console.log('');
  console.log('例（ローカルDB）:');
  console.log('  kijuku-cli migrate --db ./data/kijuku.db');
  console.log('  kijuku-cli search --title "コミック" --db ./kijuku.db');
  console.log('  kijuku-cli import --file data.json --db ./kijuku.db');
  console.log('  kijuku-cli import --file data.tsv --db ./kijuku.db --additional-columns "id_old,custom_field"');
  console.log('');
  console.log('例（リモートDB）:');
  console.log('  kijuku-cli migrate --db as5202:/home/user/kijuku.db');
  console.log('  kijuku-cli search --title "コミック" --db as5202:/home/user/kijuku.db');
  console.log('  kijuku-cli import --file data.json --db as5202:~/.local/share/kijuku/kijuku.db');
  console.log('');
  console.log('注意:');
  console.log('  - リモートDBを使用する場合、~/.ssh/config にホスト設定が必要です');
}

/**
 * コマンドライン引数をパース
 */
function parseArgs(args: string[]): {
  command: string;
  options: Record<string, string>;
} {
  const command = args[0] || 'help';
  const options: Record<string, string> = {};

  for (let i = 1; i < args.length; i++) {
    if (args[i].startsWith('--')) {
      const key = args[i].slice(2);
      const value = args[i + 1] || '';
      options[key] = value;
      i++;
    }
  }

  return { command, options };
}

/**
 * データベースパスを取得
 */
function getDbPath(options: Record<string, string>): string {
  return options.db || process.env.DATABASE_PATH || './kijuku.db';
}

/**
 * DB pathをパースしてローカル/リモートを判定
 *
 * @param dbPath - データベースパス（host:path または ローカルパス）
 * @returns { isRemote, sshHost?, remotePath?, localPath? }
 */
export function parseDbPath(dbPath: string): {
  isRemote: boolean;
  sshHost?: string;
  remotePath?: string;
  localPath?: string;
} {
  // host:path 形式をチェック（Windowsドライブレター C: を除外）
  const remoteMatch = dbPath.match(/^([^:]+):(.+)$/);
  if (remoteMatch && remoteMatch[1].length > 1) {
    // リモートパス
    return {
      isRemote: true,
      sshHost: remoteMatch[1],
      remotePath: remoteMatch[2],
    };
  }

  // ローカルパス
  return {
    isRemote: false,
    localPath: dbPath,
  };
}

/**
 * DBインスタンスを作成（ローカルまたはリモート）
 *
 * @param dbPath - データベースパス
 * @returns KijukuDB または RemoteKijukuDB
 */
export function createDatabase(dbPath: string): KijukuDB | RemoteKijukuDB {
  const parsed = parseDbPath(dbPath);

  if (parsed.isRemote) {
    // リモートDB
    return new RemoteKijukuDB({
      sshHost: parsed.sshHost!,
      dbPath: parsed.remotePath,
    });
  } else {
    // ローカルDB
    return new KijukuDB(parsed.localPath!);
  }
}

/**
 * migrateコマンドを実行
 */
async function runMigrate(options: Record<string, string>): Promise<void> {
  const dbPath = getDbPath(options);
  const parsed = parseDbPath(dbPath);

  console.log(`データベース: ${dbPath}`);
  console.log('マイグレーションを実行中...');

  try {
    if (parsed.isRemote) {
      // リモートDB
      const db = new RemoteKijukuDB({
        sshHost: parsed.sshHost!,
        dbPath: parsed.remotePath,
      });

      await db.migrate();
      const version = await db.getSchemaVersion();
      console.log(`マイグレーション完了 (バージョン: ${version})`);
    } else {
      // ローカルDB
      const db = new KijukuDB(parsed.localPath!);
      db.migrate();

      const version = db.getSchemaVersion();
      console.log(`マイグレーション完了 (バージョン: ${version})`);

      db.close();
    }
  } catch (error) {
    console.error('エラー:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

/**
 * searchコマンドを実行
 */
async function runSearch(options: Record<string, string>): Promise<void> {
  const dbPath = getDbPath(options);
  const parsed = parseDbPath(dbPath);

  try {
    const db = createDatabase(dbPath);

    // フィルタ条件を構築
    const filter: any = {};
    if (options.title) filter.title = options.title;
    if (options.artist) filter.artist = options.artist;
    if (options.type) filter.media_type = options.type;
    if (options.series) filter.series = options.series;
    if (options.source) filter.source = options.source;

    // クエリオプションを構築
    const queryOptions: any = {};
    if (options.limit) queryOptions.limit = parseInt(options.limit, 10);
    if (options.offset) queryOptions.offset = parseInt(options.offset, 10);
    if (options.orderBy) queryOptions.orderBy = options.orderBy;
    if (options.order) queryOptions.order = options.order;

    // リモート/ローカルで処理を分岐
    const results = parsed.isRemote
      ? await (db as RemoteKijukuDB).findMedia(filter, queryOptions)
      : (db as KijukuDB).findMedia(filter, queryOptions);

    console.log(`検索結果: ${results.length}件`);
    console.log('');

    results.forEach((media) => {
      console.log(`ID: ${media.id}`);
      console.log(`  タイトル: ${media.title}`);
      if (media.artist) console.log(`  作者: ${media.artist}`);
      console.log(`  タイプ: ${media.media_type}`);
      if (media.series) console.log(`  シリーズ: ${media.series}`);
      if (media.path) console.log(`  パス: ${media.path}`);
      console.log('');
    });

    // ローカルDBの場合のみclose（リモートは自動切断）
    if (!parsed.isRemote) {
      (db as KijukuDB).close();
    }
  } catch (error) {
    console.error('エラー:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

/**
 * importコマンドを実行
 */
async function runImport(options: Record<string, string>): Promise<void> {
  const dbPath = getDbPath(options);
  const parsed = parseDbPath(dbPath);
  const filePath = options.file;

  if (!filePath) {
    console.error('エラー: --file オプションが必要です');
    process.exit(1);
  }

  if (!fs.existsSync(filePath)) {
    console.error(`エラー: ファイルが見つかりません: ${filePath}`);
    process.exit(1);
  }

  try {
    const db = createDatabase(dbPath);
    const ext = path.extname(filePath).toLowerCase();
    const content = fs.readFileSync(filePath, 'utf-8');

    let data: any[];

    if (ext === '.json') {
      data = JSON.parse(content);
    } else if (ext === '.csv' || ext === '.tsv') {
      const separator = ext === '.csv' ? ',' : '\t';
      data = parse(content, {
        columns: true,
        skip_empty_lines: true,
        delimiter: separator,
        quote: '"',
        escape: '"',
        relax_quotes: true,  // フィールド内の引用符を許容
        trim: true,
      });
    } else {
      console.error('エラー: サポートされていないファイル形式です（.json, .csv, .tsvのみ）');
      process.exit(1);
    }

    console.log(`${data.length}件のメディアをインポート中...`);

    // 追加カラムのリストを取得
    const additionalColumns = options['additional-columns']
      ? options['additional-columns'].split(',').map((c: string) => c.trim())
      : [];

    // データを変換
    const processedData = data.map((item) => {
      const processed: any = { ...item };
      const attributes: Record<string, string> = {};

      // flag_existの変換
      if (typeof processed.flag_exist === 'string') {
        processed.flag_exist = processed.flag_exist.toUpperCase() === 'TRUE';
      }

      // 追加カラムを抽出（media_attributesに保存するため）
      for (const col of additionalColumns) {
        if (processed[col] !== undefined) {
          attributes[col] = String(processed[col]);
          delete processed[col];
        }
      }

      // tagsは一旦除外（後で個別に処理）
      const tags = processed.tags;
      delete processed.tags;

      return { ...processed, _tags: tags, _attributes: attributes };
    });

    // メディアを一括作成
    const results = parsed.isRemote
      ? await (db as RemoteKijukuDB).bulkCreateMedia(processedData)
      : (db as KijukuDB).bulkCreateMedia(processedData);

    // タグを処理
    let tagCount = 0;
    let attributeCount = 0;

    for (let index = 0; index < results.length; index++) {
      const media = results[index];
      const tagsStr = processedData[index]._tags;

      if (tagsStr && typeof tagsStr === 'string') {
        const tagNames = tagsStr.split(',').map((t) => t.trim()).filter((t) => t);

        for (const tagName of tagNames) {
          // タグを取得または作成
          let tag = parsed.isRemote
            ? await (db as RemoteKijukuDB).getTagByName(tagName)
            : (db as KijukuDB).getTagByName(tagName);

          if (!tag) {
            tag = parsed.isRemote
              ? await (db as RemoteKijukuDB).createTag(tagName)
              : (db as KijukuDB).createTag(tagName);
          }

          // メディアにタグを関連付け
          if (parsed.isRemote) {
            await (db as RemoteKijukuDB).addTagToMedia(media.id, tag.id);
          } else {
            (db as KijukuDB).addTagToMedia(media.id, tag.id);
          }
          tagCount++;
        }
      }

      // 追加属性を保存
      const attributes = processedData[index]._attributes as Record<string, string> | undefined;
      if (attributes && Object.keys(attributes).length > 0) {
        for (const [key, value] of Object.entries(attributes)) {
          if (parsed.isRemote) {
            await (db as RemoteKijukuDB).setMediaAttribute(media.id, key, value as string);
          } else {
            (db as KijukuDB).setMediaAttribute(media.id, key, value as string);
          }
          attributeCount++;
        }
      }
    }

    console.log(`インポート完了: ${results.length}件のメディア、${tagCount}件のタグ関連付け、${attributeCount}件の追加属性`);

    // ローカルDBの場合のみclose
    if (!parsed.isRemote) {
      (db as KijukuDB).close();
    }
  } catch (error) {
    console.error('エラー:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

/**
 * メイン処理
 */
async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const { command, options } = parseArgs(args);

  switch (command) {
    case 'migrate':
      await runMigrate(options);
      break;
    case 'search':
      await runSearch(options);
      break;
    case 'import':
      await runImport(options);
      break;
    case 'help':
      showHelp();
      break;
    default:
      console.error(`エラー: 不明なコマンド: ${command}`);
      console.log('');
      showHelp();
      process.exit(1);
  }
}

main().catch((error) => {
  console.error('エラー:', error instanceof Error ? error.message : error);
  process.exit(1);
});
