/**
 * バルク操作のサンプル
 *
 * このサンプルでは以下のバルク操作を実行します：
 * 1. JSONファイルからデータを読み込み
 * 2. 複数のメディアを一括作成
 * 3. トランザクションを使った複数操作
 * 
 * Note: 外部プロジェクトでは以下のようにインポートしてください:
 * import { KijukuDB } from 'kijuku-db';
 */
import { KijukuDB } from '../ts-sdk/dist/index.js';
import fs from 'fs';

// データベースパス
const DB_PATH = './data/example.db';
const DEMO_DATA_PATH = './data/demo-media.json';

// メイン処理
function main() {
  console.log('=== バルク操作のサンプル ===\n');

  const db = new KijukuDB(DB_PATH);
  db.migrate();

  // 1. JSONファイルからデータを読み込み
  console.log('1. デモデータを読み込み');
  const demoData = JSON.parse(fs.readFileSync(DEMO_DATA_PATH, 'utf-8'));
  console.log(`   読み込んだデータ: ${demoData.length}件\n`);

  // 2. 複数のメディアを一括作成
  console.log('2. 複数のメディアを一括作成');
  const startTime = Date.now();
  const created = db.bulkCreateMedia(demoData);
  const endTime = Date.now();

  console.log(`   作成されたメディア: ${created.length}件`);
  console.log(`   処理時間: ${endTime - startTime}ms\n`);

  // 3. トランザクションを使った複数操作
  console.log('3. トランザクションを使った複数操作');
  console.log('   - 新しいメディアを作成');
  console.log('   - タグを作成');
  console.log('   - メディアにタグを追加\n');

  try {
    db.transaction(() => {
      // メディアを作成
      const newMedia = db.createMedia({
        title: 'トランザクションテスト',
        media_type: 'comic',
        artist: 'テスト作者',
      });

      // タグを作成
      const tag1 = db.createTag('新着');
      const tag2 = db.createTag('おすすめ');

      // メディアにタグを追加
      db.addTagToMedia(newMedia.id, tag1.id);
      db.addTagToMedia(newMedia.id, tag2.id);

      console.log(`   トランザクション成功: メディアID ${newMedia.id}`);
    });
  } catch (error) {
    console.error('   トランザクション失敗:', error);
  }
  console.log('');

  // 4. メディアタイプ別の統計を表示
  console.log('4. メディアタイプ別の統計');
  const comicCount = db.findMedia({ media_type: 'comic' }).length;
  const videoCount = db.findMedia({ media_type: 'video' }).length;
  const musicCount = db.findMedia({ media_type: 'music' }).length;

  console.log(`   - コミック: ${comicCount}件`);
  console.log(`   - ビデオ: ${videoCount}件`);
  console.log(`   - 音楽: ${musicCount}件`);
  console.log(`   - 合計: ${comicCount + videoCount + musicCount}件\n`);

  // 5. 全てのタグを表示
  console.log('5. 登録されているタグ');
  const allTags = db.getAllTags();
  console.log(`   タグ数: ${allTags.length}件`);
  allTags.forEach((tag) => console.log(`   - ${tag.name} (ID: ${tag.id})`));
  console.log('');

  // データベース接続を閉じる
  db.close();
  console.log('=== 完了 ===');
}

main();
