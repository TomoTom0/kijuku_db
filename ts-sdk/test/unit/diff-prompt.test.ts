/**
 * buildDiffExplanationPrompt（LLM explanation prompt 生成）の単体テスト
 *
 * 設計 docs/design/db-protection.md §4.4/§15-15・TASK-53。
 * 4セクション構造・セマンティクス注記・分布セクション ON/OFF・サンプル上限を検証する。
 */
import { describe, it, expect } from 'vitest';
import { computeBackupDiff, summarizeDiff, buildDiffExplanationPrompt } from '../../src/diff.js';
import type { BackupSnapshot, Media } from '../../src/types.js';

const T = new Date('2026-01-01T00:00:00Z');

function media(id: number): Media {
  return {
    id,
    uuid: `uuid-${id}`,
    title: `title-${id}`,
    media_type: 'comic' as Media['media_type'],
    flag_exist: true,
    created_at: T,
    updated_at: T,
  };
}

const emptySnap = (): BackupSnapshot => ({ media: [], tags: [], mediaTags: [], attributes: [], hashes: [] });
const full = { type: 'full' } as const;

describe('buildDiffExplanationPrompt', () => {
  it('空差分でも4つの説明指示見出しとセマンティクス注記を含む', () => {
    const diff = computeBackupDiff(emptySnap(), emptySnap(), {});
    const prompt = buildDiffExplanationPrompt(diff, summarizeDiff(diff));
    expect(prompt).toContain('# メディアDB変更レビュー');
    expect(prompt).toContain('### 変更内容の要約');
    expect(prompt).toContain('### 影響範囲');
    expect(prompt).toContain('### リスク');
    expect(prompt).toContain('### promote 可否の根拠');
    expect(prompt).toContain('セマンティクス注記');
    expect(prompt).toContain('added=stg新規');
    expect(prompt).toContain('removed=prod のみ');
  });

  it('件数サマリテーブルに5テーブル分の行を含む', () => {
    const diff = computeBackupDiff(emptySnap(), emptySnap(), {});
    const prompt = buildDiffExplanationPrompt(diff, summarizeDiff(diff));
    expect(prompt).toContain('| media ');
    expect(prompt).toContain('| tags ');
    expect(prompt).toContain('| media_tags ');
    expect(prompt).toContain('| attributes ');
    expect(prompt).toContain('| hashes ');
  });

  it('includeDistribution=true で分布セクション(## 3)を含み、false で省略して説明指示が §3 に来る', () => {
    const diff = computeBackupDiff(emptySnap(), emptySnap(), {});
    const withDist = buildDiffExplanationPrompt(diff, summarizeDiff(diff), {
      maxSamplesPerSection: 5,
      includeDistribution: true,
    });
    const noDist = buildDiffExplanationPrompt(diff, summarizeDiff(diff), {
      maxSamplesPerSection: 5,
      includeDistribution: false,
    });
    expect(withDist).toContain('## 3. 分布');
    expect(withDist).toContain('## 4. 説明指示');
    expect(noDist).not.toContain('## 3. 分布');
    expect(noDist).toContain('## 3. 説明指示');
  });

  it('maxSamplesPerSection で代表サンプルの件数を上限に切り詰める', () => {
    const prod = emptySnap();
    const stg = emptySnap();
    prod.media = [media(1)];
    stg.media = [media(1), media(2), media(3), media(4)]; // added 3件
    const diff = computeBackupDiff(prod, stg, { detail: full });
    const prompt = buildDiffExplanationPrompt(diff, summarizeDiff(diff), {
      maxSamplesPerSection: 2,
      includeDistribution: false,
    });
    expect(prompt).toContain('id=2');
    expect(prompt).toContain('id=3');
    expect(prompt).not.toContain('id=4'); // 3件目は上限超過
  });

  it('extraContext を指定すると先頭に記載される', () => {
    const diff = computeBackupDiff(emptySnap(), emptySnap(), {});
    const prompt = buildDiffExplanationPrompt(diff, summarizeDiff(diff), {
      maxSamplesPerSection: 5,
      includeDistribution: true,
      extraContext: 'session-abc',
    });
    expect(prompt).toContain('コンテキスト: session-abc');
  });
});
