# テストガイド

Kijuku DBプロジェクトのテスト戦略とガイドライン。TypeScript SDK（`ts-sdk/`）とRust SDK（`rust-sdk/`）の両方を対象とします。

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

### 3. E2Eテスト（End-to-End Tests）

**場所**: `ts-sdk/test/e2e/`

**目的**: エンドユーザーの視点から実際の使用シナリオをテストする

**特徴**:
- RemoteKijukuDB（SSH経由のリモートDB操作）を実際の接続でテスト
- 環境変数 `TEST_SSH_HOST` が未設定の場合はスキップ

**リモートE2E** (`sdk-remote.test.ts`):
- RemoteKijukuDB SDKの直接テスト
- SSH接続でのCRUD・属性・同期等のテスト
- 21テストケース

### 4. Rust SDK テスト

**場所**: `rust-sdk/tests/`

**目的**: Rust SDK（kijuku-cli バイナリを含む）の機能テスト

**例**:
- `integration_test.rs`: CRUD・検索・トランザクション等の統合テスト
- `d1_test.rs`: D1（Cloudflare）向け実装のテスト
- `remote_test.rs`: リモート接続（SSH）のテスト
- `backup_test.rs` / `promote_test.rs` / `sync_test.rs`: バックアップ・昇格・同期のテスト
- `cli_integration_test.rs`: CLIの統合テスト

**実行**（`.mise.toml` のタスクを使用）:

```bash
# Rust SDK のテストのみ
mise run test:rust

# Rust + TypeScript 両方のテスト
mise run test
```

## テスト実行方法

### 全テスト実行

```bash
# TypeScript SDK の全テスト（unit + integration + e2e）
mise run test:ts

# または直接実行
cd ts-sdk && pnpm run test
```

`mise run test:ts` は `pnpm run test`（`vitest run`）を呼び出し、全テストディレクトリが対象になります。E2Eのリモートテストは `TEST_SSH_HOST` が未設定の場合にスキップされます。

### カテゴリ別実行

```bash
# 単体テストのみ
mise run test:ts:unit

# 結合テストのみ
mise run test:ts:integration

# E2Eテスト（リモート接続テスト。TEST_SSH_HOST 未設定時はスキップ）
mise run test:ts:e2e
```

### リモートテストの実行

リモートテストには SSH 接続が必要です。

```bash
# 環境変数を設定
export TEST_SSH_HOST=as5202  # ~/.ssh/config に設定されているホスト名

# リモートE2Eテストを実行
mise run test:ts:e2e

# または直接実行
cd ts-sdk && TEST_SSH_HOST=as5202 pnpm exec vitest run test/e2e/sdk-remote.test.ts
```

**注意**:
- `TEST_SSH_HOST` が設定されていない場合、リモートテストはスキップされます
- SSH接続設定は `~/.ssh/config` に記載されている必要があります

### 開発中のウォッチモード

```bash
# 変更を監視してテストを自動実行
mise run test:ts:watch
```

## テストの書き方

### 出力先の管理（必須）

テストは管理外の場所（リポジトリ内・cwd）に成果物を作ってはなりません:

- 一時ファイル・一時DBは `mkdtempSync`（TS）/ `tempfile::TempDir`（Rust）等のテンポラリディレクトリ配下に作る
- `:memory:` DB でバックアップを使うテストは `backup: { backupDir: <tempdir> }` を明示指定する（未指定のバックアップ有効化はエラーになる・TASK-95）。`backup` 未指定の `:memory:` はバックアップ無効で動作する
- リポジトリ直下・`ts-sdk/` 直下等への `backup/`・`*.db` の生成が無いことを確認しながらテストを追加する

### ディレクトリ構造

```
ts-sdk/
├── benchmarks/            # パフォーマンスベンチマーク
│   └── performance.bench.ts
└── test/
    ├── unit/              # 単体テスト（13ファイル）
    │   ├── parse-db-path.test.ts
    │   ├── create-database.test.ts
    │   └── ...
    ├── integration/       # 結合テスト（19ファイル）
    │   ├── crud.test.ts
    │   ├── search.test.ts
    │   └── ...
    └── e2e/               # E2Eテスト（リモート接続）
        └── sdk-remote.test.ts

rust-sdk/
└── tests/                 # Rust SDK テスト（13ファイル）
    ├── integration_test.rs
    ├── d1_test.rs
    └── ...
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
 * リモートE2Eテスト（実際のSSH接続）
 */
import { describe, it, expect } from 'vitest';
import { RemoteKijukuDB } from '../../src/remote.js';

const SSH_HOST = process.env.TEST_SSH_HOST;
// SSH_HOSTが設定されていない場合はテストをスキップ
const describeRemote = SSH_HOST ? describe : describe.skip;

describeRemote('RemoteKijukuDB E2E', () => {
  it('リモートDBへの接続と操作が正常に動作する', async () => {
    const db = new RemoteKijukuDB({ sshHost: SSH_HOST!, dbPath: '/tmp/test.db' });
    // テストコード
  });
});
```

### 命名規則

- **テストファイル名**: `*.test.ts`
- **単体テスト**: `<module-name>.test.ts`
- **結合テスト**: `<feature-name>.test.ts`
- **E2Eテスト**: 接続方法を表す名前（例: `sdk-remote.test.ts`）

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
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: ts-sdk/pnpm-lock.yaml

      - name: Install dependencies
        run: pnpm install --frozen-lockfile
        working-directory: ts-sdk

      - name: Build
        run: pnpm run build
        working-directory: ts-sdk

      - name: Run tests
        run: pnpm run test
        working-directory: ts-sdk

      # リモートE2Eテストは TEST_SSH_HOST が未設定のため自動的にスキップされる
```

### テストカバレッジ

```bash
# カバレッジ計測（オプション）
cd ts-sdk && pnpm exec vitest run --coverage
```

## トラブルシューティング

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
- **E2Eテスト**: 低速、実SSH接続、リモートSDKレベル
- **Rust SDKテスト**: `cargo test`（`mise run test:rust`）、機能別に分割された結合テスト

テストを書く際は、適切なカテゴリを選択し、テスト対象のスコープに応じたテストを作成してください。
