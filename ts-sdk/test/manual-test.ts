#!/usr/bin/env node
/**
 * KijukuDBの手動テストスクリプト
 */
import { KijukuDB } from '../src/index.js';
import fs from 'fs';

const testDbPath = '/tmp/manual_test.db';

// 既存のDBファイルを削除
if (fs.existsSync(testDbPath)) {
  fs.unlinkSync(testDbPath);
}

console.log('=== KijukuDB 手動テスト ===\n');

try {
  // 1. DB初期化とマイグレーション
  console.log('1. DB初期化とマイグレーション');
  const db = new KijukuDB(testDbPath);
  db.migrate();
  console.log(`✓ マイグレーション完了 (バージョン: ${db.getSchemaVersion()})\n`);

  // 2. メディア作成
  console.log('2. メディア作成');
  const media1 = db.createMedia({
    title: 'テストコミック1',
    media_type: 'comic',
    artist: 'テスト作者A',
    series: 'テストシリーズ',
    page_count: 20,
  });
  console.log(`✓ メディア作成 (ID: ${media1.id})`);

  const media2 = db.createMedia({
    title: 'テストビデオ1',
    media_type: 'video',
    artist: 'テスト作者B',
    duration_sec: 120,
  });
  console.log(`✓ メディア作成 (ID: ${media2.id})\n`);

  // 3. メディア取得
  console.log('3. メディア取得');
  const fetched = db.getMedia(media1.id);
  console.log(`✓ メディア取得: ${fetched?.title} (タイプ: ${fetched?.media_type})\n`);

  // 4. メディア更新
  console.log('4. メディア更新');
  db.updateMedia(media1.id, {
    description: '更新されたディスクリプション',
    page_count: 25,
  });
  const updated = db.getMedia(media1.id);
  console.log(`✓ メディア更新: ページ数 ${updated?.page_count}, 説明 "${updated?.description}"\n`);

  // 5. タグ作成と関連付け
  console.log('5. タグ作成と関連付け');
  const tag1 = db.createTag('アクション');
  const tag2 = db.createTag('コメディ');
  console.log(`✓ タグ作成: ${tag1.name}, ${tag2.name}`);

  db.addTagToMedia(media1.id, tag1.id);
  db.addTagToMedia(media1.id, tag2.id);
  const mediaTags = db.getMediaTags(media1.id);
  console.log(`✓ タグ関連付け: ${mediaTags.map(t => t.name).join(', ')}\n`);

  // 6. 追加属性の設定と取得
  console.log('6. 追加属性の設定と取得');
  db.setMediaAttribute(media1.id, 'custom_field1', 'カスタム値1');
  db.setMediaAttribute(media1.id, 'custom_field2', 'カスタム値2');

  const attr1 = db.getMediaAttribute(media1.id, 'custom_field1');
  console.log(`✓ 属性取得: ${attr1?.key} = ${attr1?.value}`);

  const allAttrs = db.getMediaAttributes(media1.id);
  console.log(`✓ 全属性取得: ${allAttrs.length}件`);
  allAttrs.forEach(attr => {
    console.log(`  - ${attr.key} = ${attr.value}`);
  });
  console.log('');

  // 7. 検索・フィルタリング
  console.log('7. 検索・フィルタリング');

  // 追加データ
  db.createMedia({
    title: 'テストコミック2',
    media_type: 'comic',
    artist: 'テスト作者A',
    series: 'テストシリーズ',
  });

  const searchResults = db.findMedia({ media_type: 'comic' });
  console.log(`✓ メディアタイプ検索 (comic): ${searchResults.length}件`);

  const artistResults = db.findMedia({ artist: 'テスト作者A' });
  console.log(`✓ 作者検索 (テスト作者A): ${artistResults.length}件`);

  const seriesResults = db.findMedia({ series: 'テストシリーズ' }, { limit: 1 });
  console.log(`✓ シリーズ検索 (limit 1): ${seriesResults.length}件\n`);

  // 8. 一括作成
  console.log('8. 一括作成');
  const bulkData = [
    { title: '一括1', media_type: 'comic' as const },
    { title: '一括2', media_type: 'video' as const },
    { title: '一括3', media_type: 'music' as const },
  ];
  const bulkResults = db.bulkCreateMedia(bulkData);
  console.log(`✓ 一括作成: ${bulkResults.length}件\n`);

  // 9. タグ操作
  console.log('9. タグ操作');
  const allTags = db.getAllTags();
  console.log(`✓ 全タグ取得: ${allTags.length}件`);

  const foundTag = db.getTagByName('アクション');
  console.log(`✓ タグ名検索: ${foundTag?.name} (ID: ${foundTag?.id})`);

  db.removeTagFromMedia(media1.id, tag2.id);
  const remainingTags = db.getMediaTags(media1.id);
  console.log(`✓ タグ削除後: ${remainingTags.map(t => t.name).join(', ')}\n`);

  // 10. 属性削除
  console.log('10. 属性削除');
  db.deleteMediaAttribute(media1.id, 'custom_field2');
  const attrsAfterDelete = db.getMediaAttributes(media1.id);
  console.log(`✓ 属性削除後: ${attrsAfterDelete.length}件\n`);

  // 11. メディア削除
  console.log('11. メディア削除');
  db.deleteMedia(media2.id);
  const deletedMedia = db.getMedia(media2.id);
  console.log(`✓ メディア削除: ${deletedMedia === null ? '成功' : '失敗'}\n`);

  // 12. 統計情報
  console.log('12. 統計情報');
  const allMedia = db.findMedia({});
  console.log(`✓ 全メディア数: ${allMedia.length}件`);
  const comicCount = db.findMedia({ media_type: 'comic' }).length;
  const videoCount = db.findMedia({ media_type: 'video' }).length;
  const musicCount = db.findMedia({ media_type: 'music' }).length;
  console.log(`  - comic: ${comicCount}件`);
  console.log(`  - video: ${videoCount}件`);
  console.log(`  - music: ${musicCount}件\n`);

  db.close();
  console.log('=== 全テスト完了 ===');
} catch (error) {
  console.error('エラー:', error);
  process.exit(1);
}
