/**
 * remote の target/DBパス解決の単体テスト（本番DB保護 P0）
 *
 * 設計 docs/design/db-protection.md §4.1・§13・TASK-42 P0。
 * SSH 接続不要の純粋関数のみを検証する（Rust remote.rs の resolve_remote_db_path 等と同等）。
 */
import { describe, it, expect } from 'vitest';
import {
  deriveStgDbPath,
  resolveRemoteDbPath,
  type RemoteConfig,
} from '../../src/remote.js';

describe('deriveStgDbPath', () => {
  it('inserts .stg before the extension', () => {
    expect(deriveStgDbPath('~/.local/share/kijuku/kijuku.db')).toBe(
      '~/.local/share/kijuku/kijuku.stg.db',
    );
    expect(deriveStgDbPath('kijuku.db')).toBe('kijuku.stg.db');
    expect(deriveStgDbPath('/var/db/my.db')).toBe('/var/db/my.stg.db');
  });

  it('appends .stg.db when no extension', () => {
    expect(deriveStgDbPath('/var/db/kijulu')).toBe('/var/db/kijulu.stg.db');
  });

  it('does not treat a dot in a directory name as the extension', () => {
    expect(deriveStgDbPath('/foo.bar/kijuku.db')).toBe('/foo.bar/kijuku.stg.db');
  });
});

const base = (over: Partial<RemoteConfig>): RemoteConfig => ({
  sshHost: 'host',
  ...over,
});

describe('resolveRemoteDbPath', () => {
  it('prod: uses dbPath as-is', () => {
    const cfg = base({
      target: 'prod',
      dbPath: '~/.local/share/kijuku/kijuku.db',
    });
    expect(resolveRemoteDbPath(cfg)).toBe('~/.local/share/kijuku/kijuku.db');
  });

  it('stg: prefers explicit stgDbPath', () => {
    const cfg = base({
      target: 'stg',
      dbPath: '~/.local/share/kijuku/kijuku.db',
      stgDbPath: '/custom/stg.db',
    });
    expect(resolveRemoteDbPath(cfg)).toBe('/custom/stg.db');
  });

  it('stg: derives from dbPath when stgDbPath unset', () => {
    const cfg = base({
      target: 'stg',
      dbPath: '~/.local/share/kijuku/kijuku.db',
    });
    expect(resolveRemoteDbPath(cfg)).toBe('~/.local/share/kijuku/kijuku.stg.db');
  });

  it('stg: falls back to stg default when both unset', () => {
    const cfg = base({ target: 'stg' });
    expect(resolveRemoteDbPath(cfg)).toBe('~/.local/share/kijuku/kijuku.stg.db');
  });

  it('default target is stg (no target specified)', () => {
    const cfg = base({ dbPath: '~/.local/share/kijuku/kijuku.db' });
    expect(resolveRemoteDbPath(cfg)).toBe('~/.local/share/kijuku/kijuku.stg.db');
  });
});
