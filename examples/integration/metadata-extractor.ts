/**
 * メタデータ自動取得との統合サンプル
 *
 * ファイル名やディレクトリ構造からメタデータを自動的に抽出します。
 */

import { KijukuDB } from '../../ts-sdk/src/index';
import { basename, dirname } from 'path';
import type { MediaInput } from '../../ts-sdk/src/types';

interface ExtractedMetadata {
  title: string;
  artist?: string;
  series?: string;
  volumeNumber?: number;
  volumeText?: string;
}

/**
 * ファイル名からメタデータを抽出
 *
 * 想定パターン:
 * - "[作者名] タイトル.cbz"
 * - "シリーズ名 第01巻.cbz"
 * - "[作者名] シリーズ名 Vol.01.cbz"
 */
function extractMetadataFromFilename(filePath: string): ExtractedMetadata {
  const fileName = basename(filePath).replace(/\.[^.]+$/, ''); // 拡張子を除去

  const metadata: ExtractedMetadata = {
    title: fileName,
  };

  // パターン1: [作者名] タイトル
  const pattern1 = /^\[([^\]]+)\]\s*(.+)$/;
  const match1 = fileName.match(pattern1);
  if (match1) {
    metadata.artist = match1[1].trim();
    metadata.title = match1[2].trim();
  }

  // パターン2: 巻数の抽出
  const volumePatterns = [
    /第(\d+)巻/,
    /第(\d+)話/,
    /Vol\.?\s*(\d+)/i,
    /v(\d+)/i,
    /#(\d+)/,
  ];

  for (const pattern of volumePatterns) {
    const match = metadata.title.match(pattern);
    if (match) {
      metadata.volumeNumber = parseInt(match[1], 10);
      metadata.volumeText = match[0];
      break;
    }
  }

  // シリーズ名の抽出（巻数より前の部分）
  if (metadata.volumeText) {
    const seriesMatch = metadata.title.split(metadata.volumeText)[0].trim();
    if (seriesMatch) {
      metadata.series = seriesMatch;
    }
  }

  return metadata;
}

/**
 * ディレクトリ構造からメタデータを抽出
 *
 * 想定構造:
 * - /media/作者名/シリーズ名/ファイル名
 * - /media/シリーズ名/ファイル名
 */
function extractMetadataFromPath(filePath: string): Partial<ExtractedMetadata> {
  const dirName = basename(dirname(filePath));
  const parentDirName = basename(dirname(dirname(filePath)));

  const metadata: Partial<ExtractedMetadata> = {};

  // ディレクトリ名がシリーズ名の可能性
  if (dirName && dirName !== 'media' && dirName !== '.') {
    metadata.series = dirName;
  }

  // 親ディレクトリ名が作者名の可能性
  if (parentDirName && parentDirName !== 'media' && parentDirName !== '.') {
    metadata.artist = parentDirName;
  }

  return metadata;
}

/**
 * メタデータを抽出してメディアを作成
 */
export function createMediaWithMetadata(
  db: KijukuDB,
  filePath: string,
  mediaType: 'comic' | 'video' | 'music',
  options: {
    extractFromFilename?: boolean;
    extractFromPath?: boolean;
  } = {}
): number {
  const { extractFromFilename: useFilename = true, extractFromPath: usePath = true } = options;

  let metadata: ExtractedMetadata = {
    title: basename(filePath),
  };

  // ファイル名からメタデータ抽出
  if (useFilename) {
    metadata = { ...metadata, ...extractMetadataFromFilename(filePath) };
  }

  // ディレクトリ構造からメタデータ抽出
  if (usePath) {
    const pathMetadata = extractMetadataFromPath(filePath);
    metadata = {
      ...metadata,
      artist: metadata.artist || pathMetadata.artist,
      series: metadata.series || pathMetadata.series,
    };
  }

  // MediaInputに変換
  const input: MediaInput = {
    title: metadata.title,
    media_type: mediaType,
    path: filePath,
    artist: metadata.artist,
    series: metadata.series,
    volume_number: metadata.volumeNumber,
    volume_text: metadata.volumeText,
    flag_exist: true,
  };

  const media = db.createMedia(input);
  return media.id;
}

// サンプル実行
if (import.meta.url === `file://${process.argv[1]}`) {
  console.log('メタデータ抽出サンプル');
  console.log('======================\n');

  // テストケース
  const testFiles = [
    '/media/作者A/人気シリーズ/[作者A] 人気シリーズ 第01巻.cbz',
    '/media/作者B/新作/新作コミック Vol.05.cbz',
    '/media/standalone/[作者C] 単行本.cbz',
  ];

  for (const filePath of testFiles) {
    console.log(`ファイル: ${filePath}`);

    const fileMetadata = extractMetadataFromFilename(filePath);
    const pathMetadata = extractMetadataFromPath(filePath);

    console.log('  ファイル名から抽出:');
    console.log(`    タイトル: ${fileMetadata.title}`);
    if (fileMetadata.artist) console.log(`    作者: ${fileMetadata.artist}`);
    if (fileMetadata.series) console.log(`    シリーズ: ${fileMetadata.series}`);
    if (fileMetadata.volumeNumber) console.log(`    巻数: ${fileMetadata.volumeNumber}`);

    console.log('  パスから抽出:');
    if (pathMetadata.artist) console.log(`    作者: ${pathMetadata.artist}`);
    if (pathMetadata.series) console.log(`    シリーズ: ${pathMetadata.series}`);

    console.log();
  }
}
