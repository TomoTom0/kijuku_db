# パスワードハッシュアルゴリズムの改善

## 現状

現在、パスワードのハッシュ化にSHA-256を使用しています。

**該当ファイル:**
- `rust-sdk/src/server/auth.rs:79-83`
- `ts-sdk/src/server/auth.ts:68`

## 問題点

SHA-256は暗号学的ハッシュ関数ですが、パスワード保管には以下の問題があります：

1. **計算が高速すぎる**: 1秒間に数十億回のハッシュ計算が可能で、ブルートフォース攻撃に脆弱
2. **ソルトがない**: レインボーテーブル攻撃に脆弱
3. **メモリ負荷が低い**: GPU/ASICによる並列攻撃が容易

## 改善案

パスワード専用のハッシュアルゴリズムを使用する：

### Rust側（rust-sdk/src/server/auth.rs）

```rust
// Cargo.tomlに追加
argon2 = { version = "0.5", features = ["std"] }

// hash_password関数を変更
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2
};
use rand::rngs::OsRng;

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(password_hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    let parsed_hash = PasswordHash::new(hash)?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
}
```

### TypeScript側（ts-sdk/src/server/auth.ts）

```typescript
// package.jsonに追加
"argon2": "^0.31.0"

// hash_password関数を変更
import argon2 from 'argon2';

export async function hashPassword(password: string): Promise<string> {
  return await argon2.hash(password, {
    type: argon2.argon2id,
    memoryCost: 65536,
    timeCost: 3,
    parallelism: 4
  });
}

export async function verifyPassword(password: string, hash: string): Promise<boolean> {
  try {
    return await argon2.verify(hash, password);
  } catch {
    return false;
  }
}
```

## マイグレーション戦略

既存のSHA-256ハッシュとの互換性のため、段階的な移行が必要：

1. **ハッシュ形式の検出**: ハッシュ文字列のプレフィックスで形式を判別
   - SHA-256: `sha256:` プレフィックス
   - Argon2: `$argon2id$` プレフィックス（標準形式）

2. **検証時の自動移行**: 古いハッシュで認証成功時に新しいハッシュに更新

3. **移行期間**: 全ユーザーが移行するまで両方式をサポート

## 優先度

**medium** - セキュリティ改善だが、以下の理由で即座の対応は不要：

1. 現在のユースケースは個人利用が主で、大規模な攻撃のターゲットになりにくい
2. 破壊的変更を伴うため、慎重な計画と実装が必要
3. 既存ユーザーへの影響を最小化するマイグレーション戦略が必要

## 関連

- PR: #3
- レビュー指摘: PRRT_kwDOQt7xR85nVwNY, PRRT_kwDOQt7xR85nVwNb
- 参考: [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
