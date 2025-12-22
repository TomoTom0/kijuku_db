# テストガイド

Kijuku DBプロジェクトのテスト戦略とガイドライン

## 目次

1. [テストの分類](#テストの分類)
2. [テスト実行方法](#テスト実行方法)
3. [テストの書き方](#テストの書き方)
4. [CI/CDでの実行](#cicdでの実行)

## テストの分類

### 1. 単体テスト（Unit Tests）

**場所**: `ts-sdk/test/unit/`

**目的**: 個々の関数やクラスの動作を独立してテストする

**特徴**:
- 外部依存をモック化
- 高速に実行可能
- 純粋な関数ロジックのテスト

**例**:
- `parse-db-path.test.ts`: DB path解析関数のテスト
- `create-database.test.ts`: DB作成関数のテスト（モック使用）

### 2. 結合テスト（Integration Tests）

**場所**: `ts-sdk/test/integration/`

**目的**: 複数のコンポーネントの連携動作をテストする

**特徴**:
- 実際のデータベースを使用
- SDKのAPIレベルでのテスト
- インメモリDBで高速実行

**例**:
- `crud.test.ts`: CRUD操作の統合テスト
- `search.test.ts`: 検索機能の統合テスト
- `bulk.test.ts`: バルク操作の統合テスト
- `migration.test.ts`: マイグレーション機能のテスト
- `transaction.test.ts`: トランザクション管理のテスト
- `errors.test.ts`: エラーハンドリングのテスト
- `sdk-cli.test.ts`: SDKとCLIの統合テスト

### 3. E2Eテスト（End-to-End Tests）

**場所**: `ts-sdk/test/e2e/`

**目的**: エンドユーザーの視点から実際の使用シナリオをテストする

**特徴**:
- 実際のCLIコマンドを実行
- 実際のファイルシステムを使用
- 子プロセスでCLIを起動

**ローカルE2E** (`cli-local.test.ts`):
- ローカルDBでのCLI動作確認
- migrate/import/searchコマンドのテスト
- 18テストケース

**リモートE2E** (`cli-remote.test.ts`):
- SSH経由でのリモートDB操作
- 環境変数 `TEST_SSH_HOST` が必要
- 13テストケース

**SDK リモート** (`sdk-remote.test.ts`):
- RemoteKijukuDB SDKの直接テスト
- SSH接続のテスト
- 17テストケース

## テスト実行方法

### 全テスト実行

```bash
# リモートテストを除く全テスト
bun run test:all

# または（全テスト、better-sqlite3エラーは無視）
bun test
```

### カテゴリ別実行

```bash
# 単体テストのみ
bun run test:unit

# 結合テストのみ（better-sqlite3エラーが出る場合があります）
bun run test:integration

# E2Eテスト（ローカルのみ）
bun run test:e2e
```

### リモートテストの実行

リモートテストには SSH 接続が必要です。

```bash
# 環境変数を設定
export TEST_SSH_HOST=as5202  # ~/.ssh/config に設定されているホスト名

# リモートE2Eテストを実行
bun run test:e2e:remote

# または直接実行
TEST_SSH_HOST=as5202 bun test test/e2e/cli-remote.test.ts
TEST_SSH_HOST=as5202 bun test test/e2e/sdk-remote.test.ts
```

**注意**:
- `TEST_SSH_HOST` が設定されていない場合、リモートテストはスキップされます
- SSH接続設定は `~/.ssh/config` に記載されている必要があります

### 開発中のウォッチモード

```bash
# 変更を監視してテストを自動実行
bun run test:watch
```

## テストの書き方

### ディレクトリ構造

```
ts-sdk/
├── test/
│   ├── unit/              # 単体テスト
│   │   ├── parse-db-path.test.ts
│   │   └── create-database.test.ts
│   ├── integration/       # 結合テスト
│   │   ├── crud.test.ts
│   │   ├── search.test.ts
│   │   └── ...
│   ├── e2e/               # E2Eテスト
│   │   ├── fixtures/      # テストデータ
│   │   ├── cli-local.test.ts
│   │   ├── cli-remote.test.ts
│   │   └── sdk-remote.test.ts
│   └── manual/            # 手動テスト用スクリプト
```

### 単体テストの例

```typescript
/**
 * 関数名の単体テスト
 */
import { describe, it, expect, vi } from 'vitest';
import { myFunction } from '../../src/my-module.js';

describe('myFunction', () => {
  it('正常系: 期待される結果を返す', () => {
    const result = myFunction('input');
    expect(result).toBe('expected');
  });

  it('異常系: エラーケースを処理する', () => {
    expect(() => myFunction(null)).toThrow();
  });
});
```

### モックの使用

```typescript
import { vi } from 'vitest';

// モジュール全体をモック
vi.mock('../../src/my-module.js', () => ({
  MyClass: vi.fn(),
}));

// モック関数の定義
const mockFn = vi.fn().mockReturnValue('mocked');
```

### 結合テストの例

```typescript
/**
 * 機能名の結合テスト
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { KijukuDB } from '../../src/index.js';

describe('Feature Integration', () => {
  let db: KijukuDB;

  beforeEach(() => {
    // インメモリDBを使用
    db = new KijukuDB(':memory:');
    db.migrate();
  });

  afterEach(() => {
    db.close();
  });

  it('複数の操作が連携して動作する', () => {
    // テストコード
  });
});
```

### E2Eテストの例

```typescript
/**
 * CLI E2Eテスト
 */
import { describe, it, expect } from 'vitest';
import { execSync } from 'child_process';

function runCli(args: string[]) {
  // CLIを実行
}

describe('CLI E2E', () => {
  it('コマンドが正常に実行される', () => {
    const result = runCli(['command', '--option', 'value']);
    expect(result.exitCode).toBe(0);
  });
});
```

### 命名規則

- **テストファイル名**: `*.test.ts`
- **単体テスト**: `<module-name>.test.ts`
- **結合テスト**: `<feature-name>.test.ts`
- **E2Eテスト**: `<cli/sdk>-<local/remote>.test.ts`

### テストの構造

```typescript
describe('テスト対象のグループ', () => {
  describe('特定の機能やシナリオ', () => {
    it('期待される動作の説明', () => {
      // Arrange（準備）
      // Act（実行）
      // Assert（検証）
    });
  });
});
```

### アサーション

```typescript
// 等価性
expect(value).toBe(expected);
expect(value).toEqual(expected);

// 含む・含まない
expect(array).toContain(item);
expect(string).toContain('substring');

// 真偽値
expect(value).toBeTruthy();
expect(value).toBeFalsy();

// 例外
expect(() => fn()).toThrow();
expect(() => fn()).toThrow(ErrorClass);

// 非同期
await expect(promise).resolves.toBe(value);
await expect(promise).rejects.toThrow();
```

## CI/CDでの実行

### GitHub Actions での例

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: oven-sh/setup-bun@v1

      - name: Install dependencies
        run: bun install
        working-directory: ts-sdk

      - name: Build
        run: bun run build
        working-directory: ts-sdk

      - name: Run unit tests
        run: bun run test:unit
        working-directory: ts-sdk

      - name: Run E2E tests (local)
        run: bun run test:e2e
        working-directory: ts-sdk

      # リモートテストは環境変数が必要なためスキップ
      # または専用のジョブで実行
```

### テストカバレッジ

```bash
# カバレッジ計測（オプション）
bun test --coverage
```

## トラブルシューティング

### better-sqlite3 エラー

結合テストで `better-sqlite3` のエラーが出る場合:

```
error: 'better-sqlite3' is not yet supported in Bun.
```

これは既知の問題です。単体テストとE2Eテストは正常に動作します。

### SSH接続エラー

リモートテストでSSH接続エラーが出る場合:

1. `~/.ssh/config` にホスト設定があることを確認
2. SSH鍵認証が設定されていることを確認
3. `ssh <hostname>` でマニュアル接続が可能か確認

```bash
# SSH設定例
Host as5202
  HostName example.com
  User username
  IdentityFile ~/.ssh/id_rsa
```

## まとめ

- **単体テスト**: 高速、モック使用、関数レベル
- **結合テスト**: 中速、実DB使用、SDKレベル
- **E2Eテスト**: 低速、実環境、CLIレベル

テストを書く際は、適切なカテゴリを選択し、テスト対象のスコープに応じたテストを作成してください。
