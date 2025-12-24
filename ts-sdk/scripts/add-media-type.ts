/**
 * TSVファイルにmedia_typeカラムを追加するスクリプト
 */
import fs from 'fs';
import path from 'path';
import { parse } from 'csv-parse/sync';
import { stringify } from 'csv-stringify/sync';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const files = [
  { path: path.join(__dirname, '../../tmp/tsv/default/comics.tsv'), mediaType: 'comic' },
  { path: path.join(__dirname, '../../tmp/tsv/default/videos.tsv'), mediaType: 'video' },
  { path: path.join(__dirname, '../../tmp/tsv/atara/comics.tsv'), mediaType: 'comic' },
  { path: path.join(__dirname, '../../tmp/tsv/atara/comics_en.tsv'), mediaType: 'comic' },
  { path: path.join(__dirname, '../../tmp/tsv/atara/videos.tsv'), mediaType: 'video' },
];

for (const { path: filePath, mediaType } of files) {
  if (!fs.existsSync(filePath)) {
    console.log(`スキップ: ${filePath} (ファイルが存在しません)`);
    continue;
  }

  console.log(`処理中: ${filePath}`);

  const content = fs.readFileSync(filePath, 'utf-8');

  // TSVをパース
  const data = parse(content, {
    columns: true,
    skip_empty_lines: true,
    delimiter: '\t',
    quote: '"',
    escape: '"',
    relax_quotes: true,
  });

  // media_typeが既に存在するかチェック
  if (data.length > 0 && 'media_type' in data[0]) {
    console.log(`  → すでにmedia_typeカラムが存在します`);
    continue;
  }

  // media_typeを追加
  const processedData = data.map((row: any) => ({
    ...row,
    media_type: mediaType,
  }));

  // TSVとして出力
  const output = stringify(processedData, {
    header: true,
    delimiter: '\t',
    quote: '"',
    escape: '"',
    quoted_string: true,  // 改行を含むフィールドを自動的にクォート
  });

  // 元のファイルを上書き
  fs.writeFileSync(filePath, output, 'utf-8');
  console.log(`  → ${data.length}件のレコードを処理しました`);
}

console.log('完了');
