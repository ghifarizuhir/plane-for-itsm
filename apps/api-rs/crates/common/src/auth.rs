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
