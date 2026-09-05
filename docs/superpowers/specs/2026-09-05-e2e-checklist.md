# E2E Checklist Manual — Rust Auth Slice 1 (Task 9)

Click-path untuk memverifikasi login lawan Rust API end-to-end via browser.
Diisi manual oleh operator; tiap baris beri ✓ / ✗ + catatan di kolom Hasil.

## Prasyarat

- [ ] Rust `api` hidup di `:8000` dengan PG + Redis terkoneksi (`GET /health` → 200).
- [ ] Web hidup di `:3000` dan mengarah ke Rust API (`:8000`), bukan Django.
- [ ] User uji dengan kredensial KNOWN (email + password) — sama dengan
      `SMOKE_EMAIL` / `SMOKE_PASSWORD` untuk smoke.sh bila ingin konsisten.
- [ ] Untuk cek OAuth _sampai-redirect saja_: `GITHUB_CLIENT_ID` dummy
      terisi (atau kosong — keduanya tetap 302, lihat langkah 7).
- [ ] Project + workspace uji sudah ada (untuk langkah buat-issue).

## Catatan environment

- `COOKIE_SECURE=0` wajib untuk http://localhost (cookie `plane_at`/`plane_rt`
  polos). Bila `COOKIE_SECURE=1`, cookie menjadi `__Host-` + `Secure` dan
  browser di http TIDAK akan menyimpan/mengirimnya → login tampak "gagal".
- Cookie `plane_at`/`plane_rt` = `HttpOnly; SameSite=Lax`
  (`crates/common/src/auth.rs`); tanpa layer CORS di `main.rs` (mutasi
  non-GET dijaga `origin_middleware`). Di localhost lintas-port
  (:3000 → :8000) cookie berbagi host-only — `signOut` memanggil Rust
  `/api/auth/logout/` dengan `credentials:include` best-effort sebelum POST
  Django, jadi satu klik logout membersihkan kedua sesi.
- Rate-limit login (Task 10) BELUM ada: login berulang cepat aman untuk saat ini.
- Kredensial JANGAN di-commit; cukup diingat/dicatat lokal oleh operator.

## Checklist

| No  | Langkah                                                                         | Ekspektasi                                                                                                                                                                                                                      | Hasil |
| --- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----- |
| 1   | Buka `/sign-in`                                                                 | Form login (email + password + tombol OAuth GitHub/Google) tampil, tanpa error                                                                                                                                                  |       |
| 2   | Isi email + password valid → submit                                             | Masuk workspace (redirect `/` atau workspace terakhir), nama/avatar user tampil                                                                                                                                                 |       |
| 3   | Reload halaman (Ctrl+R)                                                         | Tetap login — TIDAK dilempar ke `/sign-in` (sesi persist via cookie + refresh)                                                                                                                                                  |       |
| 4   | Buat 1 issue di project mana pun                                                | Issue tersimpan dan muncul di list; reload tetap ada                                                                                                                                                                            |       |
| 5   | Klik logout                                                                     | Redirect ke `/sign-in`; menu/user terbersih; sesi Django DAN Rust bersih (cookie `plane_at`/`plane_rt` ter-clear + sesi Django mati — langkah 6 membuktikan keduanya) |       |
| 6   | Setelah logout, buka rute privat (`/` atau `/issues`) langsung via address bar  | Redirect ke `/sign-in` (tidak bisa akses tanpa sesi)                                                                                                                                                                            |       |
| 7   | Di `/sign-in`, klik OAuth GitHub (sampai redirect saja, tak perlu login GitHub) | Browser redirect: ke `github.com/login/oauth/authorize` bila kredensial GitHub riil, ATAU kembali ke app dengan error `oauth_disabled` bila dummy/kosong. Yang penting: terjadi redirect (start → 302), bukan halaman blank/500 |       |

## Bersih-bersih

- [ ] Issue uji langkah 4 dihapus (atau biarkan bila workspace khusus uji).
- [ ] Bila memakai user smoke SQL sementara, hapus baris `users` terkait + kunci Redis `auth:refresh:*` / `auth:family:*` bila perlu.
