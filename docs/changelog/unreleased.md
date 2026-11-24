# Unreleased

## Breaking

## Added

## Changed

## Fixed

- CLI (`kijuku-cli`) の `updateExist`（stdin操作）/ `update-exist`（サブコマンド）のレスポンスに含まれる `detail_file` / `updated_ids_file` が死んだパスだったのを修正 (TASK-113)。`UpdateExistResult` の Drop による一時ファイル自動削除がレスポンス出力前に走っていたため、シリアライズ成功後に Drop を回避しファイルが CLI プロセス終了後も実在するようにした（削除責任は呼び出し側・TS SDK `updateExist` と同じ契約）
