#!/usr/bin/env node
/**
 * Kijuku DB CLI ツール
 */
import { KijukuDB } from './index.js';
import fs from 'fs';
import path from 'path';

const COMMANDS = {
  migrate: 'データベースのマイグレーションを実行',
  search: 'メディアを検索',
  import: 'JSON/CSV/TSVファイルからメディアをインポート',
  help: 'ヘルプを表示',
};

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
  console.log('  --db <path>    データベースファイルのパス（デフォルト: ./kijuku.db）');
  console.log('');
  console.log('例:');
  console.log('  kijuku-cli migrate --db ./data/kijuku.db');
  console.log('  kijuku-cli search --title "コミック" --db ./kijuku.db');
  console.log('  kijuku-cli import --file data.json --db ./kijuku.db');
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
 * migrateコマンドを実行
 */
function runMigrate(options: Record<string, string>): void {
  const dbPath = getDbPath(options);

  console.log(`データベース: ${dbPath}`);
  console.log('マイグレーションを実行中...');

  try {
    const db = new KijukuDB(dbPath);
    db.migrate();

    const version = db.getSchemaVersion();
    console.log(`マイグレーション完了 (バージョン: ${version})`);

    db.close();
  } catch (error) {
    console.error('エラー:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

/**
 * searchコマンドを実行
 */
function runSearch(options: Record<string, string>): void {
  const dbPath = getDbPath(options);

  try {
    const db = new KijukuDB(dbPath);

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

    const results = db.findMedia(filter, queryOptions);

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

    db.close();
  } catch (error) {
    console.error('エラー:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

/**
 * importコマンドを実行
 */
function runImport(options: Record<string, string>): void {
  const dbPath = getDbPath(options);
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
    const db = new KijukuDB(dbPath);
    const ext = path.extname(filePath).toLowerCase();
    const content = fs.readFileSync(filePath, 'utf-8');

    let data: any[];

    if (ext === '.json') {
      data = JSON.parse(content);
    } else if (ext === '.csv' || ext === '.tsv') {
      const separator = ext === '.csv' ? ',' : '\t';
      const lines = content.trim().split('\n');
      const headers = lines[0].split(separator);

      data = lines.slice(1).map((line) => {
        const values = line.split(separator);
        const obj: any = {};
        headers.forEach((header, i) => {
          obj[header.trim()] = values[i]?.trim();
        });
        return obj;
      });
    } else {
      console.error('エラー: サポートされていないファイル形式です（.json, .csv, .tsvのみ）');
      process.exit(1);
    }

    console.log(`${data.length}件のメディアをインポート中...`);

    const results = db.bulkCreateMedia(data);

    console.log(`インポート完了: ${results.length}件`);

    db.close();
  } catch (error) {
    console.error('エラー:', error instanceof Error ? error.message : error);
    process.exit(1);
  }
}

/**
 * メイン処理
 */
function main(): void {
  const args = process.argv.slice(2);
  const { command, options } = parseArgs(args);

  switch (command) {
    case 'migrate':
      runMigrate(options);
      break;
    case 'search':
      runSearch(options);
      break;
    case 'import':
      runImport(options);
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

main();
