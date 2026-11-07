//! 安全相关：密码散列与注册信息校验（REQ-027、REQ-028）。
//!
//! 密码使用「随机盐 + SHA-256」存储，格式为 `sha256$<盐>$<散列>`，
//! 数据库中不出现明文（REQ-027）。
//!
//! 说明：实验环境不引入第三方加密库，SHA-256 在此以标准算法手写实现，
//! 输出与 `sha2` crate 一致，并由测试向量校验。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};

/// SHA-256 轮常量。
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 初始状态。
const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// 计算 SHA-256 摘要。
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = H0;

    // 补位：0x80 + 若干 0x00 + 64 位大端长度。
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut message = Vec::with_capacity(data.len() + 72);
    message.extend_from_slice(data);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    let (chunks, _) = message.as_chunks::<64>();
    for chunk in chunks {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            let start = i * 4;
            *word = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut digest = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// 字节数组转十六进制字符串。
pub fn to_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

// ---------------------------------------------------------------------------
// 伪随机（盐、验证码）：xorshift64*，由时间与进程号播种，够用于演示环境。
// ---------------------------------------------------------------------------

static RNG_STATE: AtomicU64 = AtomicU64::new(0);

fn next_random() -> u64 {
    let mut state = RNG_STATE.load(Ordering::Relaxed);
    if state == 0 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        state = nanos ^ ((std::process::id() as u64) << 32) | 1;
    }
    let mut x = state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    let next = x.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1;
    RNG_STATE.store(next, Ordering::Relaxed);
    next
}

/// 生成随机十六进制盐。
pub fn random_salt(len: usize) -> String {
    let mut bytes = Vec::with_capacity(len);
    while bytes.len() < len {
        bytes.extend_from_slice(&next_random().to_le_bytes());
    }
    bytes.truncate(len);
    to_hex(&bytes)
}

/// 生成 6 位数字验证码（REQ-028）。
pub fn generate_captcha_code() -> String {
    format!("{:06}", next_random() % 1_000_000)
}

// ---------------------------------------------------------------------------
// 密码散列
// ---------------------------------------------------------------------------

/// 散列算法前缀，便于将来升级算法时兼容旧数据。
const SCHEME: &str = "sha256";
const SALT_LEN: usize = 16;

/// 用随机盐散列密码，返回 `sha256$<盐>$<散列>`。
pub fn hash_password(password: &str) -> String {
    let salt = random_salt(SALT_LEN);
    let digest = sha256(format!("{salt}:{password}").as_bytes());
    format!("{SCHEME}${salt}${}", to_hex(&digest))
}

/// 校验明文密码与存储的散列是否匹配。
pub fn verify_password(password: &str, stored: &str) -> bool {
    let mut parts = stored.split('$');
    let (Some(scheme), Some(salt), Some(expected)) = (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if scheme != SCHEME {
        return false;
    }
    let digest = sha256(format!("{salt}:{password}").as_bytes());
    constant_time_eq(&to_hex(&digest), expected)
}

/// 逐字节比较，避免提前返回泄露信息。
fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right) {
        diff |= a ^ b;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// 注册信息校验（REQ-001、REQ-002、REQ-027、REQ-028）
// ---------------------------------------------------------------------------

/// 用户名：3~20 个字符，允许字母、数字、下划线、连字符与汉字，不含空白。
pub fn validate_username(username: &str) -> Result<()> {
    let name = username.trim();
    let length = name.chars().count();
    if !(3..=20).contains(&length) {
        return Err(AppError::InvalidInput("用户名长度需为 3~20 个字符".into()));
    }
    if name.chars().any(|ch| ch.is_whitespace()) {
        return Err(AppError::InvalidInput("用户名不能包含空格".into()));
    }
    if !name
        .chars()
        .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(AppError::InvalidInput(
            "用户名只能包含字母、数字、下划线、连字符或汉字".into(),
        ));
    }
    Ok(())
}

/// 密码：8~32 个字符，且同时包含字母与数字。
pub fn validate_password(password: &str) -> Result<()> {
    let length = password.chars().count();
    if !(8..=32).contains(&length) {
        return Err(AppError::InvalidInput("密码长度需为 8~32 个字符".into()));
    }
    let has_letter = password.chars().any(|ch| ch.is_ascii_alphabetic());
    let has_digit = password.chars().any(|ch| ch.is_ascii_digit());
    if !has_letter || !has_digit {
        return Err(AppError::InvalidInput("密码需同时包含字母和数字".into()));
    }
    Ok(())
}

/// 手机号：11 位数字且以 1 开头。
pub fn validate_phone(phone: &str) -> Result<()> {
    let digits = phone.trim();
    if digits.len() != 11
        || !digits.chars().all(|ch| ch.is_ascii_digit())
        || !digits.starts_with('1')
    {
        return Err(AppError::InvalidInput("手机号格式不正确".into()));
    }
    Ok(())
}

/// 邮箱：简单的 `local@domain.tld` 形式校验。
pub fn validate_email(email: &str) -> Result<()> {
    let email = email.trim();
    if email.len() > 64 || email.chars().any(|ch| ch.is_whitespace()) {
        return Err(AppError::InvalidInput("邮箱格式不正确".into()));
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err(AppError::InvalidInput("邮箱格式不正确".into()));
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return Err(AppError::InvalidInput("邮箱格式不正确".into()));
    }
    let Some((host, tld)) = domain.rsplit_once('.') else {
        return Err(AppError::InvalidInput("邮箱格式不正确".into()));
    };
    if host.is_empty() || tld.len() < 2 {
        return Err(AppError::InvalidInput("邮箱格式不正确".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            to_hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            to_hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // 长度跨过 56 字节补位边界。
        assert_eq!(
            to_hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn password_hash_roundtrip() {
        let stored = hash_password("abcd1234");
        assert!(stored.starts_with("sha256$"));
        assert!(!stored.contains("abcd1234"));
        assert!(verify_password("abcd1234", &stored));
        assert!(!verify_password("abcd12345", &stored));
        // 同一密码两次散列的盐不同，结果也不同。
        assert_ne!(stored, hash_password("abcd1234"));
    }

    #[test]
    fn validation_rules() {
        assert!(validate_username("buyer_01").is_ok());
        assert!(validate_username("二").is_err());
        assert!(validate_username("has space").is_err());
        assert!(validate_password("abcd1234").is_ok());
        assert!(validate_password("abcdefgh").is_err());
        assert!(validate_password("12345678").is_err());
        assert!(validate_phone("13800000000").is_ok());
        assert!(validate_phone("23800000000").is_err());
        assert!(validate_email("a@b.com").is_ok());
        assert!(validate_email("a@b").is_err());
    }
}
