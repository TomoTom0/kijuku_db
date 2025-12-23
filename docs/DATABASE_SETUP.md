# データベースセットアップガイド

このドキュメントでは、baked.dbとatara.dbの作成、データインポート、パス設定、ファイル存在確認の手順を説明します。

## 概要

- **baked.db**: 1,143件のメディア（comic: 948件、video: 195件）
- **atara.db**: 18,639件のメディア（comic: 18,125件（ja: 8,713件、en: 9,412件）、video: 514件）

## 1. データベース作成とデータインポート

### 1.1. TSVファイルの準備

TSVファイルには以下のカラムが必要です：

- `media_type`: メディアタイプ（comic/video/music）
- その他のメディア情報（title、language、id_old等）

### 1.2. データベース作成とインポート

```bash
# baked.db の作成とインポート
node ts-sdk/dist/cli.js migrate db/baked.db
node ts-sdk/dist/cli.js import db/baked.db tmp/tsv/default/ --skip-attrs

# atara.db の作成とインポート
node ts-sdk/dist/cli.js migrate db/atara.db
node ts-sdk/dist/cli.js import db/atara.db tmp/tsv/atara/ --skip-attrs
```

## 2. パス設定

### 2.1. パス構造

パス構造は以下の規則に従います：

#### baked.db

```
comics: /share/Public/Web/resources/default/comics/content/{id_old}
videos: /share/Public/Web/resources/default/videos/content/{id_old}
```

#### atara.db

```
comics (ja): /share/Public/Web/resources/atara/comics/content/{id_old}
comics (en): /share/Public/Web/resources/atara_en/comics/content/{id_old}
videos: /share/Public/Web/resources/atara/videos/content/{id_old}
```

### 2.2. SQL更新文

#### baked.db

```sql
-- comics のパス設定（id_oldはdescriptionに格納）
UPDATE media
SET
  path = '/share/Public/Web/resources/default/comics/content/' || description,
  thumbnail_path = '/share/Public/Web/resources/default/comics/content/' || description || '/thumb.jpg'
WHERE media_type = 'comic';

-- videos のパス設定
UPDATE media
SET
  path = '/share/Public/Web/resources/default/videos/content/' ||
    (SELECT value FROM media_attributes WHERE media_attributes.media_id = media.id AND key = 'id_old'),
  thumbnail_path = '/share/Public/Web/resources/default/videos/content/' ||
    (SELECT value FROM media_attributes WHERE media_attributes.media_id = media.id AND key = 'id_old') || '/thumb.jpg'
WHERE media_type = 'video';
```

#### atara.db

```sql
-- comics (ja) のパス設定
UPDATE media
SET
  path = '/share/Public/Web/resources/atara/comics/content/' ||
    (SELECT value FROM media_attributes WHERE media_attributes.media_id = media.id AND key = 'id_old'),
  thumbnail_path = '/share/Public/Web/resources/atara/comics/content/' ||
    (SELECT value FROM media_attributes WHERE media_attributes.media_id = media.id AND key = 'id_old') || '/thumb.jpg'
WHERE media_type = 'comic' AND language = 'ja';

-- comics (en) のパス設定
UPDATE media
SET
  path = '/share/Public/Web/resources/atara_en/comics/content/' ||
    (SELECT value FROM media_attributes WHERE media_attributes.media_id = media.id AND key = 'id_old'),
  thumbnail_path = '/share/Public/Web/resources/atara_en/comics/content/' ||
    (SELECT value FROM media_attributes WHERE media_attributes.media_id = media.id AND key = 'id_old') || '/thumb.jpg'
WHERE media_type = 'comic' AND language = 'en';

-- videos のパス設定（言語条件なし）
UPDATE media
SET
  path = '/share/Public/Web/resources/atara/videos/content/' ||
    (SELECT value FROM media_attributes WHERE media_attributes.media_id = media.id AND key = 'id_old'),
  thumbnail_path = '/share/Public/Web/resources/atara/videos/content/' ||
    (SELECT value FROM media_attributes WHERE media_attributes.media_id = media.id AND key = 'id_old') || '/thumb.jpg'
WHERE media_type = 'video';
```

## 3. ファイル存在確認

### 3.1. チェック用TSVのエクスポート

```bash
# baked.db
sqlite3 -header -separator $'\t' db/baked.db \
  "SELECT media_type, id, path FROM media ORDER BY id;" \
  > tmp/wip/baked_export.tsv

# atara.db
sqlite3 -header -separator $'\t' db/atara.db \
  "SELECT media_type, id, path FROM media ORDER BY id;" \
  > tmp/wip/atara_export.tsv
```

### 3.2. リモートでのパス存在確認

`tmp/wip/check-paths.sh` スクリプトを使用してリモートサーバー上でパスの存在を確認します。

```bash
# ローカルで準備
mkdir -p /tmp/check-work
cp tmp/wip/baked_export.tsv /tmp/check-work/
cp tmp/wip/atara_export.tsv /tmp/check-work/
cp tmp/wip/check-paths.sh /tmp/check-work/

# リモートにアップロード
scp -r /tmp/check-work remote-host:/tmp/

# リモートで実行
ssh remote-host
cd /tmp/check-work
chmod +x check-paths.sh
./check-paths.sh --tsv-dir /tmp/check-work --output-dir /tmp/check-results

# 結果をダウンロード
scp -r remote-host:/tmp/check-results/* tmp/check-path/output/
```

#### チェック結果の形式

`*_check_result.tsv` ファイルには以下のカラムが含まれます：

- `media_type`: メディアタイプ
- `id`: メディアID
- `path`: チェック対象のパス
- `exists`: パスが存在するか（true/false）
- `is_dir`: ディレクトリかどうか（true/false）
- `file_count`: ファイル数（ディレクトリの場合は内部のファイル数、ファイルの場合は1）
- `error`: エラー情報（存在しない場合は "not_found"）

## 4. flag_exist の更新

### 4.1. SDK を使用した更新

`ts-sdk/tmp/update-flag-exist-sdk.ts` スクリプトを使用して、チェック結果を基に `flag_exist` フィールドを更新します。

```typescript
import { KijukuDB } from '../src/index.js';
import fs from 'fs';
import { parse } from 'csv-parse/sync';

function updateFlagExist(dbPath: string, checkResultPath: string) {
  const db = new KijukuDB(dbPath);

  const content = fs.readFileSync(checkResultPath, 'utf-8');
  const results = parse(content, {
    columns: true,
    skip_empty_lines: true,
    delimiter: '\t',
    relax_column_count: true,
    skip_records_with_error: true,
  });

  let updated = 0;
  for (const row of results) {
    const id = parseInt(row.id, 10);
    if (isNaN(id)) continue;

    const exists = row.exists === 'true';
    const media = db.getMedia(id);
    if (!media) continue;

    if (media.flag_exist !== exists) {
      db.updateMedia(id, {
        ...media,
        flag_exist: exists,
      });
      updated++;
    }
  }

  console.log(`更新完了: ${updated}件`);
  db.close();
}

// 実行
updateFlagExist('db/baked.db', 'tmp/check-path/output/baked_check_result.tsv');
updateFlagExist('db/atara.db', 'tmp/check-path/output/atara_check_result.tsv');
```

### 4.2. 実行

```bash
cd ts-sdk
tsx tmp/update-flag-exist-sdk.ts
```

### 4.3. 更新結果の確認

```bash
# baked.db
sqlite3 db/baked.db "SELECT flag_exist, COUNT(*) as count FROM media GROUP BY flag_exist;"

# atara.db
sqlite3 db/atara.db "SELECT flag_exist, COUNT(*) as count FROM media GROUP BY flag_exist;"
```

#### 期待される結果

**baked.db**:
- 存在する: 947件（82.1%）
- 存在しない: 196件（17.0%）

**atara.db**:
- 存在する: 9,372件（49.8%）
- 存在しない: 9,267件（49.2%）

## 5. volume_number の自動計算

`volume_number` フィールドは、`volume_text` から自動的に計算されます。

### 5.1. 動作仕様

- `volume_text` が整数（例: "5"）の場合、`volume_number` にその値が設定されます
- `volume_text` が整数でない場合、`volume_number` は `null` になります
- この計算は保存時に自動的に行われます

### 5.2. 使用例

```typescript
import { KijukuDB } from './src/index.js';

const db = new KijukuDB('db/baked.db');

// volume_textを設定すると、volume_numberが自動計算される
db.createMedia({
  media_type: 'comic',
  title: 'Example Comic',
  volume_text: '5',  // volume_number は 5 に設定される
});

db.createMedia({
  media_type: 'comic',
  title: 'Another Comic',
  volume_text: '5-6',  // volume_number は null（整数でないため）
});

// 検索時にvolume_numberでソート可能
const results = db.findMedia(
  { media_type: 'comic' },
  { sort: [{ field: 'volume_number', direction: 'asc' }] }
);
```

## トラブルシューティング

### TSVファイルのインポートエラー

- **エラー**: カラム数が一致しない
  - **対処**: TSVファイルに `media_type` カラムが含まれているか確認してください
  - **注意**: 複数行フィールド（章情報など）を含むTSVは `csv-parse` ライブラリで適切にパースしてください

### パス設定エラー

- **エラー**: id_old が見つからない
  - **対処**: baked.dbのcomicsでは `description` フィールドに id_old が格納されています
  - **対処**: その他のメディアタイプでは `media_attributes` テーブルを確認してください

### flag_exist 更新エラー

- **エラー**: TSVパースエラー
  - **対処**: `relax_column_count: true` と `skip_records_with_error: true` オプションを使用してください
  - **対処**: 進捗メッセージなどの無効な行は自動的にスキップされます

## 参考資料

- [テストガイド](TESTING.md) - テスト実行方法
- [パフォーマンスガイド](PERFORMANCE.md) - パフォーマンステストとベンチマーク
- [API仕様書](api.md) - 詳細なAPI仕様
- [リモートテスト手順](manual-testing-remote.md) - リモート環境でのテスト方法
