/**
 * SDK バージョン（リモート自動デプロイのバージョン比較用・TASK-69）。
 * package.json の version と同期。リリース時に `update-version` が更新する単一真実源。
 * runtime で package.json を読むと bundle 後に解決できないため定数で保持する。
 */
export const SDK_VERSION = '0.2.4';
