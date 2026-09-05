# Paritas Penuh Rust + Auth Asli — Design (2026-09-05)

## Tujuan

Rust Axum menjadi satu-satunya backend (Django pensiun total). Definisi selesai:
E2E manual terpandu hijau lawan Rust (`api:8000`).

Keputusan brainstorming: pendekatan irisan vertikal (A) — auth asli dulu,
bukan dev-login. Pelajaran penetap: verifikasi live mengalahkan asumsi
(bug INSERT write-path lolos dari unit test, tertangkap `smoke.sh`).

## Urutan irisan

1. **Auth asli** (spec ini): login JWT+refresh, OAuth server-side, frontend ikut diubah.
2. **Temuan E2E**: PATCH/DELETE detail, comment/link/relation, webhook create,
   analytic view — diperbaiki saat ditemukan E2E, bukan ditebak di muka.
3. **Boundary**: notification sending → analytic export → asset S3 →
   Unsplash/GPT external → (session/OAuth sudah di irisan 1).
4. **Hapus Django**: `apps/api`, profil legacy, `plane-mq`; semua gate hijau.

## §1 Arsitektur auth

- Cookie-based JWT ganda: access JWT 15 menit + refresh opaque 30 hari (rotating),
  keduanya httpOnly, `SameSite=Lax`, `Secure` + prefix `__Host-` di prod.
  Browser kirim otomatis; tidak ada token di JS/localStorage.
- Pengganti CSRF Django: middleware Origin/Referer check untuk semua
  method mutasi + cookie SameSite.
- Password lama tetap valid: Rust verifikasi hash Django
  (`pbkdf2_sha256$iter$salt$b64`) lawan `users.password`; tanpa reset massal.
  Hash baru ditulis dalam format yang sama (kompatibel dua arah).
- OAuth (GitHub/Google): authorization-code + PKCE, code exchange server-side,
  `state`/`nonce` di Redis TTL 10 menit. Callback sukses = set cookie + redirect.
  Secret provider dari env yang sama dengan Django.
- Kompatibilitas: `X-Api-Key` tetap (mesin-ke-mesin, DB lookup `api_tokens`);
  `Authorization: Bearer` naik dari presence-check ke verifikasi JWT asli.
  Satu middleware, tiga asal kredensial, satu identitas (`user_id`).
- Konsekuensi eksplisit: semua session Django mati saat cutover — user login
  ulang sekali (diumumkan, bukan bug).

## §2 Komponen

Backend (`apps/api-rs`, ikut pola `routes/` yang ada):

- `routes/auth.rs` (baru): `POST /api/auth/login/`, `POST /api/auth/refresh/`,
  `POST /api/auth/logout/`, `GET /api/auth/oauth/:provider/start|callback/`.
  `GET /api/users/me/` tetap, sumber identitas ganti.
- `common::auth` (baru): verifikasi PBKDF2 format-Django, JWT
  (`jsonwebtoken`, HS256, secret dari env), refresh ter-hash di Redis
  (`auth:refresh:{sha256}` → user_id + family, TTL 30 hari).
- Upgrade middleware `AuthUser`: urutan Bearer JWT → cookie access →
  `X-Api-Key`; kembalikan `user_id`. Helper `user_id()` per-route dihapus bertahap.
- Rate-limit login: 5/menit per IP untuk `/login/` + OAuth callback
  (anti brute-force; limiter global 600/mnt tidak berubah).

Frontend (`apps/web`, ubah seperlunya):

- Fetcher/interceptor: hapus header CSRF; `credentials: "include"`;
  pola 401 → `POST /auth/refresh/` sekali → retry; gagal lagi → `/sign-in`.
- Halaman sign-in/sign-up POST ke Rust; tombol OAuth ke URL start Rust.
- Tidak ada kode penyimpanan rahasia baru di JS.

## §3 Data flow, error handling, testing

Flow:

1. Login: `POST /api/auth/login/` {email,password} → verifikasi hash →
   access + refresh (keluarga baru) → `Set-Cookie` ×2 → redirect `/`.
   Gagal → 401 generik (tidak bocorkan keterdaftaran email).
2. Akses: cookie otomatis → middleware → `user_id`. 401 → refresh (rotasi,
   refresh lama hangus saat dipakai) → retry sekali.
3. OAuth: start (state Redis 10 mnt) → provider → callback (validasi state,
   tukar code server-side, cari/buat user by email terverifikasi) →
   set cookie → redirect frontend.

Error: auth gagal = 401 `{error}` (bentuk Rust; frontend baca status saja);
refresh reuse = revoke sekeluarga + 401; rate-limit = 429; OAuth state
invalid = redirect sign-in `?error=oauth`.

Testing (tiga lapis):

1. Unit (cargo): vektor hash Django nyata dari DB dev, JWT roundtrip + expiry,
   rotasi/reuse lawan Redis live-test.
2. Smoke (`smoke.sh` + cek baru): login → me → refresh → retry → logout →
   akses ditolak; OAuth start kembalikan redirect valid (callback penuh manual).
3. E2E manual terpandu: checklist click-path (login form, persist reload,
   logout, proteksi rute, OAuth sampai redirect) = definisi selesai.

## Non-goal irisan 1

Port boundary (notif/export/S3/eksternal), lockout akun beyond rate-limit,
magic-link (ikut pola password+OAuth yang ada), hapus fisik Django
(irisan 4, setelah semua gate hijau).

## Follow-up: OAuth GitLab + Gitea (Task 7 di-SKIP)

Instance ini tidak memakai keduanya (tidak ada `GITLAB_*`/`GITEA_*`/
`IS_GITLAB_ENABLED`/`IS_GITEA_ENABLED` di `.env`/`docker-compose.yml`;
keduanya default off di Django). Bila suatu saat dibutuhkan, port dengan pola
identik Task 6 (`OAuthProvider` baru di `routes/auth.rs`):
- GitLab: `token_url=https://{GITLAB_HOST}/oauth/token`,
  `userinfo=https://{GITLAB_HOST}/api/v4/user` (field `email`, butuh scope
  `read_user`; hormati `confirmed_at` seperti `email_verified`).
- Gitea: `token_url={GITEA_HOST}/login/oauth/access_token`,
  `authorize={GITEA_HOST}/login/oauth/authorize`,
  userinfo `{GITEA_HOST}/api/v1/user` + `/user/emails`
  (lihat `apps/api/plane/authentication/provider/oauth/gitea.py`).
- Tambah `GITLAB_*/GITEA_*` ke `AppConfig` + rute `:provider` yang sama.
