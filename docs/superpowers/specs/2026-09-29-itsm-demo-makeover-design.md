# ITSM Demo Makeover — workspace `terraline-demo` / project `Terra`

Tanggal: 2026-09-29. Status: disetujui user per section (brainstorming).

## 1. Latar belakang

Data demo workspace `terraline-demo` (id `25fb047e-…`) / project `Terraline Demo`
(`Terra`, id `6debab4c-…`) tidak menggambarkan kebutuhan ITSM:

- 10 issues, **semuanya `type_id = NULL`** padahal type `Incident`/`Problem` sudah ada.
- Issue #1–#7 adalah tutorial onboarding bawaan Plane ("Welcome to Terraline 👋",
  "Invite your team 🤜🤛", …), bukan ITSM.
- Issue #8–#10 ad-hoc tanpa kategori/prioritas disiplin (#10 insiden produksi nyangkut di Backlog).
- 16 states dengan duplikat (`New`×2, `In Progress`×2, `Resolved`×2, `Closed`×2).
- Labels cuma `incident`, `service-request` (redundan dengan type).
- Modules/cycles bernama ITSM generik / tutorial tanpa isi bermakna.
- 2 intake pending (VPN + laptop) contoh triage yang lemah untuk demo Jev.
- Estimates 0; services ada tapi tidak ter-link ke issues.
- Workspace dibuat manual — bukan dari `seed.rs` / Django `workspace_seed_task.py`.

Keputusan scope (Q&A user): **rapikan data live saja** (tanpa skrip seed reusable,
tanpa ubah seed bawaan), **full ITSM makeover**, **skenario payment/prepaid Bahasa
Indonesia**, **eksekusi via skrip API-driven**.

## 2. States kanonis (Section 1, disetujui)

16 rows → 10 state tunggal, nama display Bahasa Indonesia, `group` tidak berubah
(kode hanya membaca group, jadi aman):

| State akhir      | Group     | Menggantikan (dihapus setelah remap) |
| ---------------- | --------- | ------------------------------------ |
| Triase           | triage    | Triage                               |
| Baru             | backlog   | Backlog, New ×2                      |
| Siap Dikerjakan  | unstarted | Todo                                 |
| Investigasi      | started   | Investigating                        |
| Dalam Pengerjaan | started   | In Progress ×2                       |
| Ditahan          | started   | On Hold                              |
| Known Error      | started   | dipertahankan (istilah ITIL)         |
| Selesai          | completed | Resolved ×2, Done                    |
| Ditutup          | completed | Closed ×2                            |
| Dibatalkan       | cancelled | Cancelled                            |

Aturan: issue di state duplikat diremap ke pasangan kanonisnya dulu, baru duplikat
di-destroy via API. State default yang ditolak API untuk dihapus dilaporkan eksplisit.

## 3. Types (Section 2, disetujui)

Pertahankan `Incident`, `Problem`; tambah `Service Request`, `Change` (semua EN —
konsisten taksonomi, tanpa risiko rename terhadap workflow-per-type).
Aturan assign: gangguan layanan → Incident; akar masalah berulang → Problem;
permintaan akses/barang/info → Service Request; perubahan konfigurasi/rilis → Change.
Target: **nol issue `type=NULL`**.

## 4. Tiket (Section 3, disetujui)

Hapus permanen #1–#7. Tulis ulang #8–#10 + tambah baru → 9 tiket, tiap tiket
berdeskripsi ID 2–3 kalimat (dampak bisnis + langkah awal):

| Tiket                                                                | Type            | Prioritas | State           | Service                      |
| -------------------------------------------------------------------- | --------------- | --------- | --------------- | ---------------------------- |
| Transaksi QRIS timeout massal di Payment Gateway (#10 ditulis ulang) | Incident        | urgent    | Investigasi     | Payment Gateway              |
| File settlement prepaid gagal terkirim ke bank                       | Incident        | high      | Baru            | prepaid-filetransfer-out-reg |
| Koneksi Postgres Primary sempat terputus saat failover               | Incident        | medium    | Selesai         | Postgres Primary             |
| Timeout berulang ISO8583 ke core banking                             | Problem         | high      | Known Error     | prepaid-iso                  |
| Akses VPN untuk vendor audit (#8 ditulis ulang)                      | Service Request | medium    | Baru            | —                            |
| Pengadaan laptop untuk support desk (#9 ditulis ulang)               | Service Request | low       | Siap Dikerjakan | —                            |
| Reset API key merchant untuk Web API                                 | Service Request | medium    | Triase          | Web API                      |
| Upgrade patch minor Postgres Primary                                 | Change          | medium    | Siap Dikerjakan | Postgres Primary             |
| Perubahan limit transaksi QRIS malam hari                            | Change          | low       | Baru            | Payment Gateway              |

## 5. Labels (Section 4, disetujui menyusul Section 5)

Hapus `incident`, `service-request` setelah dilepas. Ganti 5 label per layanan:
`payment-gateway` (merah), `prepaid-iso` (oranye), `prepaid-filetransfer` (biru),
`postgres` (abu), `web-api` (hijau), ditempel sesuai kolom Service tabel Section 3.

## 6. Intake demo Jev (Section 5, disetujui)

Ganti 2 intake lama (pindah jadi tiket R1/R2) → 4 kasus pending baru:

| Kasus                                                           | Harapan triage Jev       |
| --------------------------------------------------------------- | ------------------------ |
| Dana nasabah terdebet 2x untuk 1 transaksi QRIS                 | Incident / urgent        |
| Minta pembuatan user dashboard settlement untuk tim finance     | Service Request / medium |
| Setiap jam 02:00 settlement tertunda 30 menit, sudah 5 hari     | Problem / high           |
| Minta penjadwalan restart rutin server filetransfer tiap Minggu | Change / low             |

## 7. Modules / cycles / services (Section 6, disetujui)

- Modules: `Katalog Layanan Payment` (tiket ber-label payment), `Pemenuhan Request`
  (R1–R3), `Known Error / Basis Pengetahuan` (P1 + I3).
- Cycles: hapus 2 tutorial → 1 baru `Shift Support Payment — Okt 2026` (aktif, berisi
  tiket urgent/high).
- Estimates: tetap 0 (out of scope — story points tidak relevan untuk ITSM).
- Services: hapus `Svc B` (data sampah, cek referensi dulu); sisanya di-link ke tiket
  via `service_issues` sesuai tabel Section 3.

## 8. Eksekusi & verifikasi (Section 7, disetujui)

1. Backup: `pg_dump` penuh sebelum skrip jalan.
2. Skrip Python API-driven di `/tmp` (tidak masuk repo), berurutan: types → labels →
   states → issues → intake → modules/cycles/services → link service_issues.
3. Mode `--dry-run` dulu (print rencana aksi); eksekusi setelah user melihat output.
4. Verifikasi: nol `type=NULL`, nol state duplikat, intake = 4 pending, panel triage
   tampil, Jev mengklasifikasi 4 kasus baru.

## 9. Out of scope

- Mengubah `seed.rs` / Django `workspace_seed_task.py` (onboarding bawaan tetap).
- Estimates / story points.
- Mengubah grup workflow atau logika Jev/triage.
