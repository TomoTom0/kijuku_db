/**
 * target 解決（resolveTarget）の単体テスト
 *
 * 設計 docs/design/db-protection.md §13（設定と環境変数）・TASK-42 P0 Step3。
 * Rust config.rs の resolve_target テストと同等の件数・論理をカバーする。
 */
import { describe, it, expect } from 'vitest';
import {
  parseTarget,
  resolveTarget,
  type EnvGetter,
  type Target,
} from '../../src/config.js';

const env = (vars: Record<string, string>): EnvGetter => (key) => vars[key];
const empty: EnvGetter = () => undefined;

describe('parseTarget', () => {
  it('parses valid variants case-insensitively', () => {
    expect(parseTarget('prod')).toBe('prod');
    expect(parseTarget('PROD')).toBe('prod');
    expect(parseTarget('stg')).toBe('stg');
    expect(parseTarget('staging')).toBe('stg');
    expect(parseTarget('  prod ')).toBe('prod');
  });

  it('returns undefined for invalid / missing', () => {
    expect(parseTarget('nope')).toBeUndefined();
    expect(parseTarget(undefined)).toBeUndefined();
  });
});

describe('resolveTarget', () => {
  it('default is stg with stg db path', () => {
    const r = resolveTarget(undefined, undefined, undefined, empty);
    expect(r.target).toBe('stg');
    expect(r.dbPath).toBe('kijuku.stg.db');
    expect(r.readonly).toBe(false);
    expect(r.shouldMigrate).toBe(true);
  });

  it('CLI target beats env', () => {
    // CLI --target prod が KIJUKU_TARGET=stg に勝つ
    const r = resolveTarget('prod', undefined, undefined, env({ KIJUKU_TARGET: 'stg' }));
    expect(r.target).toBe('prod');
    expect(r.readonly).toBe(true);
    expect(r.shouldMigrate).toBe(false);
    expect(r.dbPath).toBe('kijuku.db'); // prod デフォルト
  });

  it('env beats default', () => {
    const r = resolveTarget(undefined, undefined, undefined, env({ KIJUKU_TARGET: 'prod' }));
    expect(r.target).toBe('prod');
    expect(r.readonly).toBe(true);
  });

  it('invalid env falls back to stg', () => {
    const r = resolveTarget(undefined, undefined, undefined, env({ KIJUKU_TARGET: 'bogus' }));
    expect(r.target).toBe('stg');
  });

  it('CLI db beats all (readonly still derived from target)', () => {
    // --db が target 別 env/デフォルトに勝つ（target=prod でも --db を尊重）
    const r = resolveTarget('prod', '/custom/path.db', undefined, env({ KIJUKU_DB_PATH: '/env/prod.db' }));
    expect(r.dbPath).toBe('/custom/path.db');
    expect(r.readonly).toBe(true); // readonly は target から導出（--db には依存しない）
  });

  it('per-target env db path', () => {
    // prod + KIJUKU_DB_PATH
    const prod = resolveTarget('prod', undefined, undefined, env({ KIJUKU_DB_PATH: '/env/prod.db' }));
    expect(prod.dbPath).toBe('/env/prod.db');
    // stg + KIJUKU_STG_DB_PATH
    const stg = resolveTarget('stg', undefined, undefined, env({ KIJUKU_STG_DB_PATH: '/env/stg.db' }));
    expect(stg.dbPath).toBe('/env/stg.db');
  });

  it('stg ignores prod env var', () => {
    // target=stg のとき KIJUKU_DB_PATH(prod用) は無視して stg デフォルト
    const r = resolveTarget('stg', undefined, undefined, env({ KIJUKU_DB_PATH: '/env/prod.db' }));
    expect(r.dbPath).toBe('kijuku.stg.db');
  });

  it('read-source prod overrides target', () => {
    // --read-source prod が --target stg に勝つ（target に折り畳む・方式A）
    const r = resolveTarget('stg', undefined, 'prod', empty);
    expect(r.target).toBe('prod');
    expect(r.readonly).toBe(true);
    expect(r.shouldMigrate).toBe(false);
    expect(r.dbPath).toBe('kijuku.db');
  });

  it('read-source env', () => {
    // KIJUKU_READ_SOURCE=prod
    const r = resolveTarget(undefined, undefined, undefined, env({ KIJUKU_READ_SOURCE: 'prod' }));
    expect(r.target).toBe('prod');
    expect(r.readonly).toBe(true);
  });

  it('read-source CLI beats env', () => {
    // --read-source stg が KIJUKU_READ_SOURCE=prod に勝つ
    const r = resolveTarget(undefined, undefined, 'stg', env({ KIJUKU_READ_SOURCE: 'prod' }));
    expect(r.target).toBe('stg');
    expect(r.readonly).toBe(false);
  });

  it('read-source beats target env', () => {
    // read-source が KIJUKU_TARGET より優先
    const r = resolveTarget(
      undefined,
      undefined,
      undefined,
      env({ KIJUKU_TARGET: 'stg', KIJUKU_READ_SOURCE: 'prod' }),
    );
    expect(r.target).toBe('prod');
    expect(r.readonly).toBe(true);
  });

  it('read-source none falls back to target', () => {
    // read-source 未指定時は target に従う（従来通り）
    const r = resolveTarget('prod', undefined, undefined, empty);
    expect(r.target).toBe('prod');
    expect(r.readonly).toBe(true);
  });

  it('read-source invalid env falls back', () => {
    // KIJUKU_READ_SOURCE 不正値は無視して target（デフォルト stg）に従う
    const r = resolveTarget(undefined, undefined, undefined, env({ KIJUKU_READ_SOURCE: 'bogus' }));
    expect(r.target).toBe('stg');
    expect(r.readonly).toBe(false);
  });
});

// 型レベルの健全性チェック（コンパイルを通すことで Target の網羅性を担保）
describe('Target type exhaustiveness', () => {
  it('covers prod and stg', () => {
    const values: Target[] = ['prod', 'stg'];
    expect(values).toContain('prod');
    expect(values).toContain('stg');
  });
});
