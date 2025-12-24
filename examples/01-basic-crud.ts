/**
 * 基本的なCRUD操作のサンプル
 *
 * このサンプルでは以下の操作を実行します：
 * 1. データベースの初期化
 * 2. メディアの作成
 * 3. メディアの取得
 * 4. メディアの更新
 * 5. メディアの削除
 * 
 * Note: 外部プロジェクトでは以下のようにインポートしてください:
 * import { KijukuDB } from 'kijuku-db';
 */
import { KijukuDB } from '../ts-sdk/dist/index.js';

// データベースパス
const DB_PATH = './data/example.db';

// メイン処理
function main() {
  console.log('=== 基本的なCRUD操作のサンプル ===\n');

  // 1. データベースを初期化
  console.log('1. データベースを初期化');
  const db = new KijukuDB(DB_PATH);
  db.migrate();
  console.log('   マイグレーション完了\n');

  // 2. メディアを作成（CREATE）
  console.log('2. メディアを作成');
  const media = db.createMedia({
    title: 'サンプルコミック',
    media_type: 'comic',
    artist: '作者名',
    series: 'サンプルシリーズ',
    volume_number: 1,
    description: 'これはサンプルのコミックです',
    page_count: 200,
  });
  console.log('   作成されたメディア:');
  console.log(`   - ID: ${media.id}`);
  console.log(`   - タイトル: ${media.title}`);
  console.log(`   - 作者: ${media.artist}`);
  console.log(`   - シリーズ: ${media.series}\n`);

  // 3. メディアを取得（READ）
  console.log('3. メディアを取得');
  const retrieved = db.getMedia(media.id);
  if (retrieved) {
    console.log(`   取得したメディア: ${retrieved.title}`);
    console.log(`   タイプ: ${retrieved.media_type}`);
    console.log(`   ページ数: ${retrieved.page_count}\n`);
  }

  // 4. メディアを更新（UPDATE）
  console.log('4. メディアを更新');
  db.updateMedia(media.id, {
    description: '更新された説明文です',
    page_count: 250,
  });
  const updated = db.getMedia(media.id);
  if (updated) {
    console.log('   更新後のメディア:');
    console.log(`   - 説明: ${updated.description}`);
    console.log(`   - ページ数: ${updated.page_count}\n`);
  }

  // 5. メディアを削除（DELETE）
  console.log('5. メディアを削除');
  db.deleteMedia(media.id);
  const deleted = db.getMedia(media.id);
  console.log(`   削除後の取得結果: ${deleted === null ? 'null（削除成功）' : '存在する（削除失敗）'}\n`);

  // データベース接続を閉じる
  db.close();
  console.log('=== 完了 ===');
}

main();
