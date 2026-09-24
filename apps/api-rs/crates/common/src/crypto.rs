//! Fernet-compatible decrypt + env helpers shared by api/worker/beat.
//! Dipindah dari `crates/api/src/routes/instance_admin.rs` (move-only).

/// `encryption.py:13-16`: PBKDF2-HMAC-SHA256(SECRET_KEY, salt=b"salt", 100000).
pub fn derive_fernet_key(secret: &str) -> [u8; 32] {
    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;
    let mut dk = [0u8; 32];
    pbkdf2_hmac::<Sha256>(secret.as_bytes(), b"salt", 100_000, &mut dk);
    dk
}

pub fn fernet_secret() -> String {
    // Django `settings.SECRET_KEY` (`settings/common.py:32` — env
    // `SECRET_KEY`). Must match Django's value or stored secrets won't
    // decrypt (documented wiring requirement, not a fallback).
    //
    // Shared-secret wiring (single value, no second secret): `setup.sh`
    // appends the generated `SECRET_KEY` to `apps/api/.env`, and
    // `docker-compose.yml` gives the Rust `api`/`worker`/`beat` services
    // that SAME file via `env_file`, so this reads the exact value Django
    // used to encrypt. Local `cargo` runs must export `SECRET_KEY` from
    // that file (api-rs loads no `.env` by itself).
    std::env::var("SECRET_KEY").unwrap_or_default()
}

fn pkcs7_unpad(data: &[u8]) -> Option<Vec<u8>> {
    let &last = data.last()?;
    if last == 0 || last > 16 || data.len() < last as usize {
        return None;
    }
    if !data[data.len() - last as usize..]
        .iter()
        .all(|&b| b == last)
    {
        return None;
    }
    Some(data[..data.len() - last as usize].to_vec())
}

fn aes128_cbc_decrypt(key: &[u8; 16], iv: &[u8; 16], ciphertext: &[u8]) -> Option<Vec<u8>> {
    use aes::cipher::{BlockDecryptMut, KeyInit};
    use aes::Aes128;
    if ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return None;
    }
    let mut cipher = Aes128::new_from_slice(key).expect("16-byte key");
    let mut out = Vec::with_capacity(ciphertext.len());
    let mut prev = *iv;
    for block in ciphertext.chunks(16) {
        let mut g = aes::cipher::generic_array::GenericArray::from_slice(block).clone();
        cipher.decrypt_block_mut(&mut g);
        let mut plain = [0u8; 16];
        for (i, b) in g.iter().enumerate() {
            plain[i] = b ^ prev[i];
        }
        out.extend_from_slice(&plain);
        prev.copy_from_slice(block);
    }
    pkcs7_unpad(&out)
}

/// Shared with the encrypt path, which still lives in
/// `crates/api/src/routes/instance_admin.rs`.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `encryption.py:34-44`: decrypt; empty input → `""`; ANY failure → `""`
/// (Django logs + returns `""`).
pub fn decrypt_data(enc: &str, secret: &str) -> String {
    use base64::Engine;
    if enc.is_empty() {
        return String::new();
    }
    let raw = match base64::engine::general_purpose::URL_SAFE.decode(enc.trim()) {
        Ok(r) => r,
        Err(_) => return String::new(),
    };
    if raw.len() < 1 + 8 + 16 + 16 + 32 || raw[0] != 0x80 {
        return String::new();
    }
    let key = derive_fernet_key(secret);
    let (signing, encryption): ([u8; 16], [u8; 16]) = (
        key[..16].try_into().expect("split"),
        key[16..].try_into().expect("split"),
    );
    let (payload, sig) = raw.split_at(raw.len() - 32);
    if !ct_eq(&hmac_sha256(&signing, payload), sig) {
        return String::new();
    }
    let iv: [u8; 16] = payload[9..25].try_into().expect("iv slice");
    let ct = &payload[25..];
    match aes128_cbc_decrypt(&encryption, &iv, ct) {
        Some(pt) => String::from_utf8(pt).unwrap_or_default(),
        None => String::new(),
    }
}

/// `settings/common.py:365`: `SKIP_ENV_VAR = env("SKIP_ENV_VAR","1")=="1"`.
/// True (default) → read from the DB (`instance_value.py:19-33`, decrypting
/// secrets); False → read env directly (`instance_value.py:34-38`).
pub fn skip_env_vars() -> bool {
    std::env::var("SKIP_ENV_VAR")
        .map(|v| v == "1")
        .unwrap_or(true)
}
