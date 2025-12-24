/**
 * 検索・フィルタのサンプル
 *
 * このサンプルでは以下の検索操作を実行します：
 * 1. タイトルでの検索
 * 2. メディアタイプでのフィルタ
 * 3. シリーズでの検索
 * 4. 作者での検索
 * 5. ソート順の指定
 * 6. ページネーション（limit/offset）
 * 7. タグでの検索
 * 
 * Note: 外部プロジェクトでは以下のようにインポートしてください:
 * import { KijukuDB } from 'kijuku-db';
 */
import { KijukuDB } from '../ts-sdk/dist/index.js';

// データベースパス
const DB_PATH = './data/example.db';

// メイン処理
function main() {
  console.log('=== 検索・フィルタのサンプル ===\n');

  const db = new KijukuDB(DB_PATH);
  db.migrate();

  // テストデータを作成
  console.log('テストデータを作成中...\n');
  const media1 = db.createMedia({
    title: 'ワンピース 1巻',
    media_type: 'comic',
    artist: '尾田栄一郎',
    series: 'ワンピース',
  });

  const media2 = db.createMedia({
    title: 'ワンピース 2巻',
    media_type: 'comic',
    artist: '尾田栄一郎',
    series: 'ワンピース',
  });

  const media3 = db.createMedia({
    title: 'ドラゴンボール 1巻',
    media_type: 'comic',
    artist: '鳥山明',
    series: 'ドラゴンボール',
  });

  db.createMedia({
    title: '君の名は。',
    media_type: 'video',
    artist: '新海誠',
  });

  db.createMedia({
    title: 'RADWIMPS - 前前前世',
    media_type: 'music',
    artist: 'RADWIMPS',
  });

  // 1. タイトルでの検索
  console.log('1. タイトルに"ワンピース"を含む検索');
  const byTitle = db.findMedia({ title: 'ワンピース' });
  console.log(`   検索結果: ${byTitle.length}件`);
  byTitle.forEach((m) => console.log(`   - ${m.title}`));
  console.log('');

  // 2. メディアタイプでのフィルタ
  console.log('2. メディアタイプが"comic"のフィルタ');
  const byType = db.findMedia({ media_type: 'comic' });
  console.log(`   検索結果: ${byType.length}件`);
  byType.forEach((m) => console.log(`   - ${m.title} (${m.artist})`));
  console.log('');

  // 3. シリーズでの検索
  console.log('3. シリーズが"ワンピース"の検索');
  const bySeries = db.findMedia({ series: 'ワンピース' });
  console.log(`   検索結果: ${bySeries.length}件`);
  bySeries.forEach((m) => console.log(`   - ${m.title}`));
  console.log('');

  // 4. 作者での検索
  console.log('4. 作者が"尾田栄一郎"の検索');
  const byArtist = db.findMedia({ artist: '尾田栄一郎' });
  console.log(`   検索結果: ${byArtist.length}件`);
  byArtist.forEach((m) => console.log(`   - ${m.title}`));
  console.log('');

  // 5. ソート順の指定
  console.log('5. タイトルで昇順ソート');
  const sorted = db.findMedia(
    { media_type: 'comic' },
    { orderBy: 'title', order: 'ASC' }
  );
  console.log(`   検索結果: ${sorted.length}件`);
  sorted.forEach((m) => console.log(`   - ${m.title}`));
  console.log('');

  // 6. ページネーション
  console.log('6. ページネーション（最初の2件）');
  const paginated = db.findMedia({}, { limit: 2, offset: 0 });
  console.log(`   検索結果: ${paginated.length}件`);
  paginated.forEach((m) => console.log(`   - ${m.title}`));
  console.log('');

  console.log('7. ページネーション（次の2件）');
  const paginatedNext = db.findMedia({}, { limit: 2, offset: 2 });
  console.log(`   検索結果: ${paginatedNext.length}件`);
  paginatedNext.forEach((m) => console.log(`   - ${m.title}`));
  console.log('');

  // 7. タグでの検索
  console.log('8. タグでの検索');
  const tag = db.createTag('お気に入り');
  db.addTagToMedia(media1.id, tag.id);
  db.addTagToMedia(media3.id, tag.id);

  const byTag = db.findMedia({ tag_ids: [tag.id] });
  console.log(`   タグ"お気に入り"を持つメディア: ${byTag.length}件`);
  byTag.forEach((m) => console.log(`   - ${m.title}`));
  console.log('');

  // データベース接続を閉じる
  db.close();
  console.log('=== 完了 ===');
}

main();
