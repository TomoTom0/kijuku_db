#!/usr/bin/env node
/**
 * TSV/CSVファイルにmedia_typeカラムを追加するスクリプト
 */
import { parse } from 'csv-parse/sync';
import { stringify } from 'csv-stringify/sync';
import fs from 'fs';
import path from 'path';

const args = process.argv.slice(2);

if (args.length < 3) {
  console.error('使い方: add-media-type.ts <入力ファイル> <出力ファイル> <media_type>');
  console.error('例: add-media-type.ts input.tsv output.tsv video');
  process.exit(1);
}

const [inputFile, outputFile, mediaType] = args;

if (!['comic', 'video', 'music'].includes(mediaType)) {
  console.error(`エラー: media_typeは comic, video, music のいずれかである必要があります（指定値: ${mediaType}）`);
  process.exit(1);
}

if (!fs.existsSync(inputFile)) {
  console.error(`エラー: 入力ファイルが見つかりません: ${inputFile}`);
  process.exit(1);
}

try {
  const ext = path.extname(inputFile).toLowerCase();
  const separator = ext === '.csv' ? ',' : '\t';
  const content = fs.readFileSync(inputFile, 'utf-8');

  // パース
  const records = parse(content, {
    columns: true,
    skip_empty_lines: true,
    delimiter: separator,
    quote: '"',
    escape: '"',
    relax_quotes: true,
    trim: true,
  });

  console.log(`${records.length}件のレコードを読み込みました`);

  // media_typeを追加
  const processedRecords = records.map((record: any) => ({
    media_type: mediaType,
    ...record,
  }));

  // 出力
  const output = stringify(processedRecords, {
    header: true,
    delimiter: separator,
    quote: '"',
    escape: '"',
  });

  fs.writeFileSync(outputFile, output, 'utf-8');
  console.log(`${outputFile} に ${processedRecords.length}件のレコードを出力しました`);
} catch (error) {
  console.error('エラー:', error instanceof Error ? error.message : error);
  process.exit(1);
}
