# きじゅくDB 統合サンプル

実際のアプリケーションとの統合例を示すサンプルコードです。

## サンプル一覧

### 1. ファイルスキャナー (`file-scanner.ts`)

ディレクトリをスキャンして、メディアファイルを自動的にデータベースに登録します。

#### 機能

- ディレクトリの再帰的スキャン
- ファイル拡張子によるメディアタイプの自動判定
- 重複チェック（既に登録済みのファイルはスキップ）
- ファイルサイズの自動記録

#### 対応拡張子

- **コミック**: `.cbz`, `.cbr`, `.zip`, `.rar`, `.pdf`, `.epub`
- **動画**: `.mp4`, `.mkv`, `.avi`, `.mov`, `.wmv`, `.flv`, `.webm`
- **音楽**: `.mp3`, `.flac`, `.wav`, `.aac`, `.m4a`, `.ogg`, `.wma`

#### 使用例

```typescript
import { KijukuDB } from 'kijuku-db';
import { scanDirectory } from './file-scanner';

const db = new KijukuDB('./media.db');
db.migrate();

const result = scanDirectory(db, './my-media-files', {
  recursive: true,      // サブディレクトリも再帰的にスキャン
  skipExisting: true,   // 既に登録済みのファイルはスキップ
  verbose: true,        // 詳細なログを出力
});

console.log(`追加: ${result.added}件`);
console.log(`スキップ: ${result.skipped}件`);
console.log(`エラー: ${result.errors}件`);

db.close();
```

#### コマンドライン実行

```bash
npx tsx examples/integration/file-scanner.ts [データベースパス] [スキャン対象ディレクトリ]

# 例
npx tsx examples/integration/file-scanner.ts ./media.db ./my-comics
```

### 2. メタデータ抽出器 (`metadata-extractor.ts`)

ファイル名やディレクトリ構造から、メタデータを自動的に抽出します。

#### 機能

- ファイル名からのメタデータ抽出
  - 作者名の抽出（`[作者名] タイトル` 形式）
  - 巻数の抽出（`第01巻`, `Vol.01`, `v01`, `#01` など）
  - シリーズ名の抽出
- ディレクトリ構造からのメタデータ抽出
  - 親ディレクトリ名を作者名として使用
  - ディレクトリ名をシリーズ名として使用

#### 対応パターン

```
ファイル名パターン:
  [作者A] タイトル.cbz           → 作者: 作者A, タイトル: タイトル
  シリーズ名 第01巻.cbz          → シリーズ: シリーズ名, 巻数: 1
  [作者B] シリーズ名 Vol.05.cbz → 作者: 作者B, シリーズ: シリーズ名, 巻数: 5

ディレクトリ構造:
  /media/作者名/シリーズ名/ファイル名
  /media/シリーズ名/ファイル名
```

#### 使用例

```typescript
import { KijukuDB } from 'kijuku-db';
import { createMediaWithMetadata } from './metadata-extractor';

const db = new KijukuDB('./media.db');
db.migrate();

const mediaId = createMediaWithMetadata(
  db,
  '/media/作者A/人気シリーズ/[作者A] 人気シリーズ 第01巻.cbz',
  'comic',
  {
    extractFromFilename: true,
    extractFromPath: true,
  }
);

const media = db.getMedia(mediaId);
console.log(media);
// {
//   title: '人気シリーズ 第01巻',
//   artist: '作者A',
//   series: '人気シリーズ',
//   volume_number: 1,
//   volume_text: '第01巻',
//   ...
// }

db.close();
```

#### コマンドライン実行

```bash
npx tsx examples/integration/metadata-extractor.ts
```

## 統合サンプルの応用

### ファイルスキャナー + メタデータ抽出

2つのサンプルを組み合わせて、より高度な自動登録システムを構築できます。

```typescript
import { KijukuDB } from 'kijuku-db';
import { readdirSync, statSync } from 'fs';
import { join } from 'path';
import { createMediaWithMetadata } from './metadata-extractor';

const db = new KijukuDB('./media.db');
db.migrate();

function smartScan(directory: string) {
  const files = readdirSync(directory);

  for (const file of files) {
    const filePath = join(directory, file);
    const stats = statSync(filePath);

    if (stats.isFile() && filePath.endsWith('.cbz')) {
      // メタデータを自動抽出して登録
      const mediaId = createMediaWithMetadata(db, filePath, 'comic');
      console.log(`登録完了: ID=${mediaId}`);
    }
  }
}

smartScan('./my-comics');
db.close();
```

### 定期的なスキャン

cron や systemd タイマーと組み合わせて、定期的にディレクトリをスキャンすることができます。

```bash
# crontab -e
# 毎日深夜0時にスキャン
0 0 * * * cd /path/to/project && npx tsx examples/integration/file-scanner.ts ./media.db ./media-files
```

### Webアプリケーションとの統合

Express などの Web フレームワークと組み合わせて、Web UI から操作できるようにすることも可能です。

```typescript
import express from 'express';
import { KijukuDB } from 'kijuku-db';

const app = express();
const db = new KijukuDB('./media.db');
db.migrate();

app.get('/api/media', (req, res) => {
  const media = db.findMedia({}, { limit: 100 });
  res.json(media);
});

app.get('/api/scan', (req, res) => {
  const result = scanDirectory(db, './media-files');
  res.json(result);
});

app.listen(3000, () => {
  console.log('Server running on http://localhost:3000');
});
```

## まとめ

これらのサンプルは、きじゅくDBを実際のアプリケーションに統合する際の出発点として利用できます。
必要に応じてカスタマイズして、独自のメディア管理システムを構築してください。
