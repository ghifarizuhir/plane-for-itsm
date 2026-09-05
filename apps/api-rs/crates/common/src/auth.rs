use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

const ITERATIONS: u32 = 1_000_000;

/// Verifikasi hash format Django `pbkdf2_sha256$iter$salt$b64`.
/// False untuk format tak dikenal — tidak pernah panic pada input asing.
pub fn verify_django_password(password: &str, encoded: &str) -> bool {
    let mut parts = encoded.split('$');
    match (parts.next(), parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("pbkdf2_sha256"), Some(iter), Some(salt), Some(hash), None) => {
            let iter: u32 = match iter.parse() {
                Ok(n) => n,
                Err(_) => return false,
            };
            let expected = match base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                hash,
            ) {
                Ok(b) => b,
                Err(_) => return false,
            };
            // Django SHA-256 digests are always 32 bytes: reject empty and
            // wrong-length digests (empty vec compared true via empty fold).
            if expected.len() != 32 /* SHA-256 output */ {
                return false;
            }
            let mut out = [0u8; 32];
            pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), iter, &mut out);
            out.iter().zip(expected.iter()).fold(0, |a, (x, y)| a | (x ^ y)) == 0
        }
        _ => false,
    }
}

/// Hash password baru dalam format Django (agar kompatibel dua arah).
pub fn make_django_password(password: &str) -> String {
    let salt: String = uuid::Uuid::new_v4().simple().to_string()[..12].into();
    let mut out = vec![0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), ITERATIONS, &mut out);
    format!(
        "pbkdf2_sha256${ITERATIONS}${salt}${}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, out)
    )
}

#[derive(Debug, Serialize, Deserialize)]
struct AccessClaims {
    sub: String,
    exp: usize,
    iat: usize,
    jti: String,
}

pub fn encode_access(user_id: &uuid::Uuid, secret: &str, ttl_secs: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let claims = AccessClaims {
        sub: user_id.to_string(),
        exp: (now + ttl_secs) as usize,
        iat: now as usize,
        jti: uuid::Uuid::new_v4().to_string(),
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .expect("jwt encode")
}

pub fn decode_access(token: &str, secret: &str) -> Result<uuid::Uuid, String> {
    let mut v = Validation::default();
    v.validate_exp = true;
    decode::<AccessClaims>(token, &DecodingKey::from_secret(secret.as_bytes()), &v)
        .map_err(|e| e.to_string())?
        .claims
        .sub
        .parse()
        .map_err(|e| format!("bad sub: {e}"))
}

pub fn cookie_headers(name: &str, value: &str, max_age: i64, secure: bool) -> String {
    let mut h = format!("{name}={value}; HttpOnly; Path=/; Max-Age={max_age}; SameSite=Lax");
    if secure {
        h.push_str("; Secure");
    }
    h
}

pub fn clear_cookie_header(name: &str, secure: bool) -> String {
    let mut h = format!("{name}=; HttpOnly; Path=/; Max-Age=0; SameSite=Lax");
    if secure {
        h.push_str("; Secure");
    }
    h
}
