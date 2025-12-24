#!/bin/bash
# CLIツールの使用例
#
# このスクリプトでは以下のCLI操作を実行します：
# 1. データベースのマイグレーション
# 2. JSONファイルからデータをインポート
# 3. メディアの検索
# 4. 検索結果の絞り込み

echo "=== CLIツールの使用例 ==="
echo ""

# データベースパス
DB_PATH="./data/example-cli.db"

# 1. データベースのマイグレーション
echo "1. データベースのマイグレーション"
kijuku-cli migrate --db "$DB_PATH"
echo ""

# 2. JSONファイルからデータをインポート
echo "2. デモデータをインポート"
kijuku-cli import --file ./data/demo-media.json --db "$DB_PATH"
echo ""

# 3. 全てのコミックを検索
echo "3. 全てのコミックを検索"
kijuku-cli search --type comic --db "$DB_PATH"
echo ""

# 4. タイトルで検索
echo "4. タイトルに'ワンピース'を含むメディアを検索"
kijuku-cli search --title "ワンピース" --db "$DB_PATH"
echo ""

# 5. 作者で検索
echo "5. 作者が'新海誠'のメディアを検索"
kijuku-cli search --artist "新海誠" --db "$DB_PATH"
echo ""

# 6. ビデオタイプで最新5件を取得
echo "6. ビデオタイプで最新5件を取得"
kijuku-cli search --type video --orderBy created_at --order DESC --limit 5 --db "$DB_PATH"
echo ""

# 7. シリーズで検索
echo "7. シリーズが'鬼滅の刃'のメディアを検索"
kijuku-cli search --series "鬼滅の刃" --db "$DB_PATH"
echo ""

echo "=== 完了 ==="
