/**
 * RemoteKijukuDB SSH Session 接続プールの単体テスト（TASK-71・Rust parity）。
 *
 * ssh2 / ssh-config / fs を mock し、実 SSH 接続なしでプールの中核挙動を検証する:
 * - 連続 RPC で Session を再利用する（connectCount が増えない）
 * - SshSessionError で slot が無効化され、次回 RPC で再接続する（connectCount が増える）
 * - disconnect() 後の RPC で再接続する
 *
 * Rust `tests/remote_test.rs` の `test_remote_connect_count` / `test_remote_disconnect_then_reconnect`
 * （#[ignore]・実 SSH 必須）と同等の検証を、mock により CI で決定的に実行する。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';

const mock = vi.hoisted(() => {
  let counter = 0;
  const created: { id: number; ended: boolean }[] = [];
  let execFailReal = false; // getServerVersion 以外の exec を失敗させる（SshSessionError 経路）
  let version: string | null = '0.0.0'; // getServerVersion 応答（null ならデプロイ扱い）

  type Listener = (...args: unknown[]) => void;

  /** ssh2 exec stream の最小 mock（data / close / stderr.data / destroy）。 */
  class MockStream {
    private listeners: Record<string, Listener[]> = {};
    private stderrListeners: Record<string, Listener[]> = {};
    stderr = {
      on: (ev: string, fn: Listener) => {
        (this.stderrListeners[ev] ??= []).push(fn);
        return this;
      },
    };
    on(ev: string, fn: Listener) {
      (this.listeners[ev] ??= []).push(fn);
      return this;
    }
    emitData(payload: Buffer) {
      (this.listeners['data'] ?? []).forEach((f) => f(payload));
    }
    emitClose(code: number) {
      (this.listeners['close'] ?? []).forEach((f) => f(code));
    }
    destroy() {
      /* no-op */
    }
  }

  /** ssh2 Client の最小 mock（ready / exec / sftp / end）。 */
  class MockClient {
    id: number;
    ended = false;
    private listeners: Record<string, Listener[]> = {};
    constructor() {
      this.id = ++counter;
      created.push(this);
    }
    on(ev: string, fn: Listener) {
      (this.listeners[ev] ??= []).push(fn);
      return this;
    }
    private emit(ev: string, ...args: unknown[]) {
      (this.listeners[ev] ?? []).forEach((f) => f(...args));
    }
    connect(_opts: unknown) {
      setImmediate(() => this.emit('ready'));
    }
    exec(command: string, cb: (err: Error | null, stream?: MockStream) => void) {
      // echo 経由の JSON 操作のみ応答を返す。シェル補助コマンド（mkdir 等）は exit 0 の空出力。
      const match = command.match(/echo '(\{.*\})' \|/);
      const operation = match ? (JSON.parse(match[1] as string).operation as string) : null;

      if (operation !== 'getServerVersion' && execFailReal) {
        setImmediate(() => cb(new Error('mock channel failure')));
        return;
      }

      const stream = new MockStream();
      setImmediate(() => {
        cb(null, stream);
        let resp: { success: boolean; data?: unknown };
        if (operation === 'getServerVersion') {
          resp = { success: true, data: { version } };
        } else if (operation === 'getSchemaVersion') {
          resp = { success: true, data: { version: 1 } };
        } else if (operation === null) {
          // シェル補助コマンド（deploy の mkdir/chmod/ln）は成功扱い・応答 JSON なし
          stream.emitClose(0);
          return;
        } else {
          resp = { success: true, data: {} };
        }
        stream.emitData(Buffer.from(JSON.stringify(resp)));
        stream.emitClose(0);
      });
    }
    sftp(cb: (err: Error | null, sftp: unknown) => void) {
      setImmediate(() => cb(null, {}));
    }
    end() {
      this.ended = true;
    }
  }

  return {
    MockClient,
    created,
    setFailRealCommands(v: boolean) {
      execFailReal = v;
    },
    setVersion(v: string | null) {
      version = v;
    },
    reset() {
      counter = 0;
      created.length = 0;
      execFailReal = false;
      version = '0.0.0';
    },
  };
});

vi.mock('ssh2', () => ({ Client: mock.MockClient }));
vi.mock('ssh-config', () => ({
  default: {
    parse: () => ({
      compute: () => ({ HostName: 'mock-host', User: 'mock-user', Port: 22, IdentityFile: ['/mock/key'] }),
    }),
  },
}));
vi.mock('fs', () => ({
  readFileSync: (_path: unknown, encoding?: unknown) =>
    typeof encoding === 'string' ? '' : Buffer.from(''),
}));

import { RemoteKijukuDB } from '../../src/remote.js';
import { SDK_VERSION } from '../../src/version.js';

function newRemote(): RemoteKijukuDB {
  return new RemoteKijukuDB({ sshHost: 'mock-host', dbPath: '/tmp/mock.db' });
}

describe('RemoteKijukuDB SSH Session 接続プール（TASK-71）', () => {
  beforeEach(() => {
    mock.reset();
    mock.setVersion(SDK_VERSION); // バージョン一致 → デプロイ未発生で getServerVersion のみ
  });

  it('連続 RPC で Session を再利用し connectCount は増えない', async () => {
    const remote = newRemote();
    expect(remote.connectCount).toBe(0);

    await remote.getSchemaVersion();
    const afterFirst = remote.connectCount;
    expect(afterFirst).toBe(1);

    // 2 回目は既存 Session を使い回す（新規接続しない）
    await remote.getSchemaVersion();
    expect(remote.connectCount).toBe(afterFirst);
    expect(mock.created.length).toBe(1);
  });

  it('SshSessionError で slot が無効化され、次回 RPC で再接続する', async () => {
    const remote = newRemote();
    await remote.getSchemaVersion();
    expect(remote.connectCount).toBe(1);

    // 実コマンド exec を失敗させセッション系エラーを発生 → slot 無効化
    mock.setFailRealCommands(true);
    await expect(remote.getSchemaVersion()).rejects.toThrow('コマンド実行エラー');
    expect(remote.connectCount).toBe(1); // 失敗 RPC 自体は再接続しない

    // 復旧後の RPC で再接続（connectCount 増加・新規 Client）
    mock.setFailRealCommands(false);
    await remote.getSchemaVersion();
    expect(remote.connectCount).toBe(2);
    expect(mock.created.length).toBe(2);
  });

  it('disconnect() 後の RPC は再接続する', async () => {
    const remote = newRemote();
    await remote.getSchemaVersion();
    expect(remote.connectCount).toBe(1);

    await remote.disconnect();
    expect(mock.created[0]?.ended).toBe(true);

    await remote.getSchemaVersion();
    expect(remote.connectCount).toBe(2);
    expect(mock.created.length).toBe(2);
  });
});
