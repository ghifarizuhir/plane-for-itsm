# Rust Auth Sign-up (Invite-only) — Design (2026-09-20)

## Tujuan

Member yang diundang bisa membuat akun tanpa email confirmation. Saat ini form
sign-up web melakukan **native form POST ke `${API_BASE_URL}/auth/sign-up/`**
(`apps/web/core/components/account/auth-forms/password.tsx:205-220`) — endpoint
Django yang **tidak ada di api-rs** dan tidak lagi dilayani (container `api` =
api-rs; Django hanya `migrator`). Hasilnya fallback 404 `{"error":"Page not
found."}` tampil di URL API dan akun tidak pernah terbuat.

Keputusan brainstorming:

1. **Invite-only** — signup hanya untuk email yang punya invite workspace
   pending. Tanpa email confirmation.
2. **Auto-join** — saat signup sukses, invite pending langsung diproses jadi
   `WorkspaceMember` dan row invite dihapus; user tidak perlu accept manual.
3. **Pendekatan A** — handler baru di modul terpisah `routes/auth_signup.rs`;
   frontend sign-up pindah ke JSON fetch seperti sign-in.

Non-goal slice ini: hapus Django, magic-link signup, project-invite auto-join,
open signup.

## Kontrak API

- **Endpoint:** `POST /api/auth/signup/` (JSON, publik — tanpa auth).
- **Request:** `{ "email": string, "password": string }` (bentuk `LoginBody`).
- **Sukses 200:** `{ "id": "<uuid>", "email": "<email>" }` + 2 `Set-Cookie`
  (`plane_at` access JWT 15 mnt, `plane_rt` refresh opaque 30 hari; prefix
  `__Host-` saat `cookie_secure`) — identik jalur login
  (`routes/auth.rs::login`).
- **Error:** `{ "error_code": <int>, "error_message": "<str>" }` (bentuk
  `AuthenticationException.get_error_dict()`, `adapter/error.py`). Tabel kode
  di bawah.
- **Rate limit:** route didaftarkan di `auth_router` (`main.rs:1757-1775`)
  sehingga ikut `IpRateLimiter` 5/menit yang sama dengan login.
- **Origin check:** middleware mutasi yang ada (`Origin`/`Referer` vs
  `FRONTEND_URL`) otomatis berlaku; fetch frontend sudah mengirim `Origin`.
- **Tanpa migrasi DB** — skema tidak berubah.

## Alur handler & aturan bisnis

Urutan validasi (mengikuti `SignUpAuthEndpoint.post` +
`EmailProvider(is_signup=True)` Django, kecuali gate invite):

| #   | Cek                                                                                                              | Gagal →                                       |
| --- | ---------------------------------------------------------------------------------------------------------------- | --------------------------------------------- |
| 1   | Instance setup (`SELECT is_setup_done FROM instances WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1`) | 400 `{5000, INSTANCE_NOT_CONFIGURED}`         |
| 2   | Email & password wajib (nilai raw, belum di-trim)                                                                | 400 `{5040, REQUIRED_EMAIL_PASSWORD_SIGN_UP}` |
| 3   | Normalisasi email (trim + lowercase) lalu `auth::email_valid`                                                    | 400 `{5045, INVALID_EMAIL_SIGN_UP}`           |
| 4   | User belum terdaftar (`users.email`)                                                                             | 409 `{5030, USER_ALREADY_EXIST}`              |
| 5   | Invite-only: ada `workspace_member_invites` pending (`deleted_at IS NULL AND responded_at IS NULL`) untuk email  | 403 `{5015, SIGNUP_DISABLED}`                 |
| 6   | Password kuat (`auth_compat::password_strong_enough`, zxcvbn crate ≥3)                                           | 400 `{5021, PASSWORD_TOO_WEAK}`               |

Kode error dari `plane/authentication/adapter/error.py:7-26`.

### Pembuatan data (satu transaksi Postgres)

1. **`users`** — pola identik `instance_admin::sign_up` (`routes/instance_admin.rs:594-616`):
   `username` = uuid4 hex (32), `password` = `authn::make_django_password`,
   `display_name` = prefix email, `token` = 2×uuid4 hex (64),
   `is_password_autoset=false`, `is_email_verified=false` (default Django untuk
   password signup — branch autoset yang men-set true tidak dipakai),
   `is_active=true`, `is_staff=false`, `is_superuser=false`,
   `last_login_medium='email'`, `last_login_ip`/`last_login_uagent` dari request,
   `last_active`/`last_login_time`/`token_updated_at`=now.
2. **`profiles`** — semua kolom NOT NULL dengan default Django
   (`onboarding_step` = 4 flag false, `mobile_onboarding_step` = 3 flag false,
   `background_color` random hex, `theme='{}'`, `goals='{}'`,
   `product_tour` 5 flag false, `notification_view_mode='full'`,
   `billing_address_country='INDIA'`, `language='en'`, `is_app_rail_docked=true`,
   dst) — pola `instance_admin.rs:617-637`.
3. **Auto-join** — untuk tiap invite pending milik email, urut `created_at`:
   - jika `workspace_members` row ada → `UPDATE is_active=true, role=invite.role`
     (pola `invite.rs::ws_join_post` reactivation);
   - else `INSERT workspace_members` dengan `default_ws_member_props()`
     (`routes/invite.rs:671-682`) + `ON CONFLICT DO NOTHING`;
   - **hard-delete** row invite (pola `ws_join_post` accept, `invite.rs:787-790`).
   - Set `profiles.last_workspace_id` = workspace pertama yang di-join (mirror
     UI `/invitations` yang memanggil `updateUserProfile({last_workspace_id})`).
4. **Setelah commit** — terbitkan access JWT + refresh Redis (`SET` hash →
   `uid:family`, `SADD` family, `EXPIRE`) persis `login`; kirim 2 cookie; balas
   200 `{id, email}`.

### Deviasi/skip terdokumentasi

- `ENABLE_EMAIL_PASSWORD` tidak dicek — konsisten dengan `login` Rust yang juga
  tidak mengeceknya; gate invite-only menggantikan fungsi pembatasnya.
- `ProjectMemberInvite` tidak diproses — tetap lewat accept UI yang sudah ada
  (`users_me::join_projects`).
- Tanpa efek email apa pun (SMTP tidak dikonfigurasi; parity dengan jalur
  invite api-rs yang memang skip celery).
- `process_workspace_project_invitations` Django hanya memproses invite
  `accepted=True`; saat signup tidak mungkin ada, jadi tidak ada padanannya.
- Skor zxcvbn crate bisa sedikit berbeda dari zxcvbn-python; frontend juga
  memvalidasi (`getPasswordStrength`).
- Redis gagal setelah commit DB → 500, akun tetap terbuat (user bisa login);
  sama seperti risiko jalur login, didokumentasikan.

## Perubahan frontend

**`apps/web/core/components/account/auth-forms/password.tsx`:**

1. `handleSignUp` (`:155-167`) → `fetch` JSON ke `/api/auth/signup/` dengan
   `credentials: "include"` (mirror `handleSignIn` `:122-145`); `res.ok` →
   `window.location.assign(nextPath || "/")`.
2. Hapus jalur CSRF/native form yang jadi mati: state `csrfPromise`, `useEffect`
   `requestCSRFToken`, `handleCSRFToken`, `formRef`, atribut `method`/`action`,
   hidden input `csrfmiddlewaretoken`/`email`/`next_path`, import + instance
   `AuthService` (di file ini hanya untuk CSRF). `<form onSubmit>` tetap
   (`preventDefault`) agar tombol Enter berfungsi.
3. Banner error dinamis: simpan `errorCode` dari response; map ke i18n —
   5021 → key lama `auth.sign_up.errors.password.strength`; 5030 → key baru
   akun sudah ada; 5015 → key baru invite-only; 5045 → key baru email tidak
   valid; lainnya/network → `something_went_wrong_please_try_again`.
4. i18n: tambah key di `packages/i18n/src/locales/en/auth.json` (locale lain
   fallback ke English); edit file locale mengikuti skill `translate`.

**Catatan:** `apps/web` tidak punya unit-test runner — verifikasi via `tsc` +
`oxlint` + build + E2E manual.

## Testing

1. **Unit (cargo)** di `auth_signup.rs`: parsing body (field wajib), mapping
   `error_code → HTTP status`, predikat invite-only (pending vs responded),
   normalisasi email. Logika DB dibuat tipis (`test_app()` repo ini lazy).
2. **Smoke live** (`apps/api-rs/scripts/smoke.sh`): signup tanpa invite → 403;
   password lemah (dengan invite) → 400; happy path → 200 + cookie →
   `GET /api/users/me/workspaces/` memuat workspace hasil auto-join → row
   invite terhapus → cleanup.
3. **Parity gate:** tidak ada entri baru di `parity-inventory.json` (auth
   endpoints tidak dilacak; `/api/auth/login/` pun tidak ada). Test existing
   tidak boleh regress.
4. **Frontend:** `pnpm --filter=web exec tsc --noEmit`, `oxlint`,
   `pnpm --filter=web build`.
5. **E2E manual tunnel:** admin invite email baru → member signup → onboarding
   dengan workspace hasil auto-join terlihat; negatif: email tanpa invite →
   banner invite-only; tanpa langkah email.

## Rollout

1. `docker compose -f docker-compose-local.yml build api` (image
   `plane-api-rs:local`; worker/beat memakai image yang sama).
2. `docker compose -f docker-compose-local.yml up -d api worker beat-worker`
   lalu cek `/health`.
3. `pnpm --filter=web build` → `systemctl --user restart plane-web-prod.service`
   (prosedur AGENTS.md).
4. Tanpa migrasi DB. Dua invite pending (`ghifari@gmail.com`,
   `andaranata45@gmail.com`) langsung bisa dipakai signup setelah deploy.

**Rollback:** revert kode + rebuild; akun yang sudah terbuat tetap valid (hash
kompatibel dua arah dengan Django).

## File map

- Create: `apps/api-rs/crates/api/src/routes/auth_signup.rs` — handler + helper
  murni + unit test.
- Modify: `apps/api-rs/crates/api/src/routes/mod.rs` — daftarkan modul.
- Modify: `apps/api-rs/crates/api/src/main.rs` — route `/api/auth/signup/` di
  `auth_router` (dekat `:1757`).
- Modify: `apps/api-rs/crates/api/src/routes/auth.rs` — `set_cookies` jadi
  `pub(crate)` (`cookie_pair` tetap privat; hanya dipakai internal).
- Modify: `apps/api-rs/crates/api/src/routes/invite.rs` —
  `default_ws_member_props()` jadi `pub(crate)` (dipakai auto-join).
- Modify: `apps/web/core/components/account/auth-forms/password.tsx` — JSON
  signup + banner error.
- Modify: `packages/i18n/src/locales/en/auth.json` — key error baru.
- Modify (opsional): `apps/api-rs/scripts/smoke.sh` — siklus signup.

Referensi desain: `docs/superpowers/specs/2026-09-05-full-parity-rust-auth-design.md`
(§2 menyatakan halaman sign-up POST ke Rust; slice 1 menundanya dan TODO-nya ada
di `password.tsx:156-158`).
