# ITSM Demo Makeover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merapikan data live workspace `terraline-demo` / project `Terra` menjadi demo ITSM skenario payment Bahasa Indonesia, via skrip Python API-driven.

**Architecture:** Satu skrip `/tmp/itsm_makeover.py` (tidak masuk repo) memanggil API lokal `http://localhost:8000` dengan kredensial admin, berurutan: backup → auth → types → labels → states → issues → intake → modules/cycles/services → verifikasi. Tiap tahap idempotent (check-before-create) dan mendukung `--dry-run`.

**Tech Stack:** Python 3 + `requests`, API Rust (`api-rs`), Postgres (verifikasi via `docker exec psql`).

**Spec:** `docs/superpowers/specs/2026-09-29-itsm-demo-makeover-design.md`.

**Konstanta global (dipakai semua task):**

- `BASE = http://localhost:8000`
- `WS = terraline-demo`
- `PID = 6debab4c-966e-4e77-aaf3-2d84055208b9` (project `Terra`)
- `DB = docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "<SQL>"`

---

### Task 0: Backup + baseline

**Files:** none (shell only).

- [ ] **Step 1: Backup penuh DB**

Run: `docker exec plane-for-itsm-plane-db-1 pg_dump -U plane -d plane -Fc -f /tmp/plane-pre-makeover.dump && ls -la /tmp/plane-pre-makeover.dump`
Expected: file dump > 0 bytes. Restore point jika skrip salah langkah: `pg_restore -U plane -d plane /tmp/plane-pre-makeover.dump` (hentikan container `api`/`worker` dulu).

- [ ] **Step 2: Catat baseline**

Run: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "SELECT count(*) FROM issues WHERE project_id='6debab4c-966e-4e77-aaf3-2d84055208b9';"` → `10`.
Run: `... -c "SELECT count(*) FROM issues WHERE project_id='6debab4c-966e-4e77-aaf3-2d84055208b9' AND type_id IS NULL;"` → `10`.
Run: `... -c "SELECT name, count(*) FROM states WHERE project_id='6debab4c-966e-4e77-aaf3-2d84055208b9' GROUP BY name HAVING count(*) > 1;"` → duplikat New/In Progress/Resolved/Closed.
Run: `... -c "SELECT count(*) FROM intake_issues WHERE project_id='6debab4c-966e-4e77-aaf3-2d84055208b9' AND status = -2;"` → `2`.
Expected: 10 / 10 / 4 baris duplikat / 2. Simpan output sebagai pembanding akhir.

---

### Task 1: Kredensial skrip (X-Api-Key via SQL)

Alasan: login password admin tidak diketahui; `AuthUser` menerima `X-Api-Key` via lookup `api_tokens`.

- [ ] **Step 1: Periksa bentuk tabel token**

Run: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "\d api_tokens"`
Expected: kolom `token`, `user_id`, `is_active`, `deleted_at`, `expired_at`. Jika nama kolom berbeda, sesuaikan Step 3 (jangan tebak — baca output ini dulu).

- [ ] **Step 2: Pilih user admin workspace demo**

Run: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "SELECT u.email, m.role FROM workspace_members m JOIN users u ON u.id = m.member_id JOIN workspaces w ON w.id = m.workspace_id WHERE w.slug='terraline-demo' AND m.role >= 15 AND m.is_active = true;"`
Expected: ≥1 baris (role 20 = admin, 15 = member). Ambil `member_id` (UUID) baris role tertinggi sebagai `ADMIN_UID`. Jika kosong, hentikan dan tanyakan user.

- [ ] **Step 3: Buat token sekali pakai**

Run (ganti `<ADMIN_UID>`, token bebas unik):
`docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "INSERT INTO api_tokens (id, token, user_id, is_active, created_at, updated_at) VALUES (gen_random_uuid(), 'itsm-makeover-20260929', '<ADMIN_UID>', true, now(), now());"`
Expected: `INSERT 0 1`. Jika kolom wajib lain ditolak NOT NULL, baca pesan error dan tambahkan kolomnya (lihat hasil Step 1).

- [ ] **Step 4: Verifikasi token terhadap API**

Run: `curl -s -H 'X-Api-Key: itsm-makeover-20260929' http://localhost:8000/api/workspaces/terraline-demo/projects/6debab4c-966e-4e77-aaf3-2d84055208b9/states/ | head -c 200`
Expected: JSON array states (200), BUKAN `{"error":"missing or invalid auth"}`.

- [ ] **Step 5: Hapus token setelah makeover selesai (di Task 9)**

---

### Task 2: Kerangka skrip + mode dry-run

**Files:**

- Create: `/tmp/itsm_makeover.py`

- [ ] **Step 1: Tulis kerangka skrip**

```python
#!/usr/bin/env python3
"""ITSM demo makeover — API-driven, idempotent, --dry-run supported."""
import argparse, sys
import requests

BASE = "http://localhost:8000"
WS = "terraline-demo"
PID = "6debab4c-966e-4e77-aaf3-2d84055208b9"
HDR = {"X-Api-Key": "itsm-makeover-20260929", "Content-Type": "application/json"}
P = f"/api/workspaces/{WS}/projects/{PID}"

ap = argparse.ArgumentParser()
ap.add_argument("--dry-run", action="store_true")
A = ap.parse_args()

def call(method, path, body=None):
    if A.dry_run:
        print(f"[DRY] {method} {path} {body if body else ''}")
        return {}
    r = requests.request(method, BASE + path, headers=HDR, json=body, timeout=30)
    if r.status_code >= 400:
        print(f"FAIL {method} {path} -> {r.status_code} {r.text[:300]}")
        sys.exit(1)
    return r.json() if r.text else {}

def get(path):
    r = requests.get(BASE + path, headers=HDR, timeout=30)
    r.raise_for_status()
    return r.json()

if __name__ == "__main__":
    print("skeleton ok (dry-run)" if A.dry_run else "skeleton ok")
```

- [ ] **Step 2: Jalankan dry-run kerangka**

Run: `python3 /tmp/itsm_makeover.py --dry-run`
Expected: `skeleton ok (dry-run)`, exit 0. Commit tidak diperlukan (file di /tmp, di luar repo).

---

### Task 3: Types — tambah Service Request + Change, import ke project

Endpoint: `POST /api/workspaces/:slug/work-item-types/` (create workspace),
`POST /api/workspaces/:slug/projects/:project_id/import-work-item-types/` (body `{"type_ids": [...]}` — jika ditolak, baca error dan GET `workflow-map` dulu untuk bentuk body yang benar),
`GET .../workflow-map/` (verifikasi).

- [ ] **Step 1: Tambah helper + logika types ke skrip**

```python
def ensure_ws_type(name):
    items = get(f"/api/workspaces/{WS}/work-item-types/")
    arr = items if isinstance(items, list) else items.get("results", items)
    for t in arr:
        if t["name"].lower() == name.lower():
            print(f"keep type {name} {t['id']}")
            return t["id"]
    t = call("POST", f"/api/workspaces/{WS}/work-item-types/", {"name": name})
    print(f"create type {name} {t['id']}")
    return t["id"]

def task_types():
    ids = {n: ensure_ws_type(n) for n in ["Incident", "Problem", "Service Request", "Change"]}
    if not A.dry_run:
        call("POST", f"{P}/import-work-item-types/", {"type_ids": list(ids.values())})
    else:
        print(f"[DRY] POST {P}/import-work-item-types/ {list(ids.values())}")
    return ids
```

Panggil `TYPE_IDS = task_types()` di `__main__` sebelum print akhir.

- [ ] **Step 2: Dry-run**

Run: `python3 /tmp/itsm_makeover.py --dry-run`
Expected: baris `[DRY]`/keep untuk 4 type + import. Jika `GET work-item-types` gagal, baca error — endpoint list adalah `routes::v1::work_item_type::list_workspace`, path di atas sudah sesuai `main.rs`.

- [ ] **Step 3: Eksekusi + verifikasi**

Run: `python3 /tmp/itsm_makeover.py` (tanpa dry-run)
Run: `curl -s -H 'X-Api-Key: itsm-makeover-20260929' http://localhost:8000/api/workspaces/terraline-demo/projects/6debab4c-966e-4e77-aaf3-2d84055208b9/workflow-map/ | python3 -m json.tool | head -30`
Expected: 4 type tampil terikat ke project.

---

### Task 4: Labels — buat 5 label layanan

Endpoint: `GET/POST .../projects/:project_id/labels/` (POST 201, body `{"name","color"}`; duplikat → 400 — helper cek dulu by name), `DELETE .../labels/:pk/` (untuk 2 label lama, setelah tak dipakai issue mana pun — dicek di Task 6).

- [ ] **Step 1: Tambah logika labels**

```python
LABELS = {
    "payment-gateway": "#EF4444",
    "prepaid-iso": "#F59E0B",
    "prepaid-filetransfer": "#0693e3",
    "postgres": "#64748B",
    "web-api": "#16A34A",
}

def task_labels():
    existing = {l["name"]: l["id"] for l in get(f"{P}/labels/")}
    out = {}
    for name, color in LABELS.items():
        if name in existing:
            print(f"keep label {name}")
            out[name] = existing[name]
        else:
            l = call("POST", f"{P}/labels/", {"name": name, "color": color})
            print(f"create label {name}")
            out[name] = l["id"]
    return out
```

Panggil `LABEL_IDS = task_labels()` di `__main__`.

- [ ] **Step 2: Dry-run + eksekusi + verifikasi**

Run: `python3 /tmp/itsm_makeover.py --dry-run` → baris DRY 5 label.
Run: `python3 /tmp/itsm_makeover.py` lalu `curl -s -H 'X-Api-Key: itsm-makeover-20260929' .../labels/ | python3 -c "import json,sys; print([l['name'] for l in json.load(sys.stdin)])"`
Expected: 5 nama label tampil (plus 2 lama yang dihapus di Task 6).

---

### Task 5: States — rename kanonis, remap, destroy duplikat

Endpoint: `GET .../states/` (lihat flag default tiap state), `PATCH .../states/:pk/` (body `{"name": ...}`), `PATCH .../issues/:id/` (body `{"state_id": ...}` — cek handler `issue_update`; jika field bernama `state`, sesuaikan dari error/tes), `DELETE .../states/:pk/`.

Peta rename (nama lama → nama kanonis), survivor = satu id per nama akhir:
Baru←{Backlog, New}, Siap Dikerjakan←{Todo}, Investigasi←{Investigating},
Dalam Pengerjaan←{In Progress}, Ditahan←{On Hold}, Selesai←{Resolved, Done},
Ditutup←{Closed}, Dibatalkan←{Cancelled}, Triase←{Triage}, Known Error tetap.

- [ ] **Step 1: Tambah logika states**

```python
RENAME = {"Backlog": "Baru", "New": "Baru", "Todo": "Siap Dikerjakan",
          "Investigating": "Investigasi", "In Progress": "Dalam Pengerjaan",
          "On Hold": "Ditahan", "Resolved": "Selesai", "Done": "Selesai",
          "Closed": "Ditutup", "Cancelled": "Dibatalkan", "Triage": "Triase"}

def task_states():
    states = get(f"{P}/states/")
    by_name = {}
    for s in states:
        by_name.setdefault(s["name"], []).append(s)
    canon = {}  # nama kanonis -> survivor id
    for name, group in by_name.items():
        target = RENAME.get(name, name)
        surv = group[0]
        if surv["name"] != target:
            call("PATCH", f"{P}/states/{surv['id']}/", {"name": target})
            print(f"rename {name} -> {target}")
        canon.setdefault(target, surv["id"])
        for dup in group[1:]:
            print(f"remap issues {dup['name']}({dup['id']}) -> {canon[target]} then destroy")
            if not A.dry_run:
                issues = get(f"{P}/issues/")  # list lalu filter state di bawah
                arr = issues if isinstance(issues, list) else issues.get("results", [])
                for i in arr:
                    sid = i.get("state_id") or i.get("state")
                    if sid == dup["id"]:
                        call("PATCH", f"{P}/issues/{i['id']}/", {"state_id": canon[target]})
                call("DELETE", f"{P}/states/{dup['id']}/")
    # duplikat lintas-nama (mis. dua "New" sudah tertangani via group[1:]; dua survivor
    # beda-nama-lama yang targetnya sama, mis. Backlog+New): samakan ke canon[target]
    return canon
```

CATATAN EKSEKUSI: jika `DELETE` state ditolak karena default, JANGAN paksa via SQL — catat id + namanya ke output dan lanjut; laporkan ke user sebagai sisa manual.

- [ ] **Step 2: Dry-run, baca rencana remap, eksekusi**

Run: `python3 /tmp/itsm_makeover.py --dry-run` → periksa tiap baris rename/remap masuk akal.
Run: `python3 /tmp/itsm_makeover.py`
Verifikasi SQL: `SELECT name, count(*) FROM states WHERE project_id='...' GROUP BY name HAVING count(*)>1;` → 0 baris; `SELECT count(*) ... states` → 10.

---

### Task 6: Issues — hapus tutorial, tulis ulang, buat baru

Endpoint: `DELETE .../bulk-delete-issues/` (body `{"issue_ids": [...]}` — jika ditolak, baca `bulk_delete` handler/test `bulk_delete_scope_sql` untuk bentuk body), `PATCH .../issues/:id/` (fields: `name, description_html, priority, type_id, state_id, label_ids`), `POST .../issues/` (struct `CreateIssue`: `name, label_ids, state_id, description_html, priority, type_id`).

Konten (dari spec Section 3 + tabel di bawah; `canon` dari Task 5, `TYPE_IDS` Task 3, `LABEL_IDS` Task 4):

Rewrite (by sequence_id):

- #8 → "Akses VPN untuk vendor audit", Service Request, medium, Baru, desc: "Vendor audit …<2 kalimat dampak + langkah>".
- #9 → "Pengadaan laptop untuk support desk", Service Request, low, Siap Dikerjakan.
- #10 → "Transaksi QRIS timeout massal di Payment Gateway", Incident, urgent, Investigasi, label payment-gateway.

Baru (6): settlement gagal (Incident/high/Baru/prepaid-filetransfer), failover Postgres (Incident/medium/Selesai/postgres), ISO8583 berulang (Problem/high/Known Error/prepaid-iso), reset API key (Service Request/medium/Triase/web-api), upgrade patch Postgres (Change/medium/Siap Dikerjakan/postgres), limit QRIS (Change/low/Baru/payment-gateway). Tiap `description_html` = `<p>`2–3 kalimat dampak bisnis + langkah awal`</p>`.

- [ ] **Step 1: Tambah logika issues (hapus #1–#7 by sequence, rewrite, create)**

```python
def seq_map():
    issues = get(f"{P}/issues/")
    arr = issues if isinstance(issues, list) else issues.get("results", [])
    return {i["sequence_id"]: i for i in arr}

def task_issues(canon, types, labels):
    m = seq_map()
    if not A.dry_run:
        ids = [m[s]["id"] for s in range(1, 8)]
        call("DELETE", f"{P}/bulk-delete-issues/", {"issue_ids": ids})
        print("deleted seq 1-7")
    else:
        print("[DRY] DELETE bulk seq 1-7")
    rewrites = {
        8: dict(name="Akses VPN untuk vendor audit", type_id=types["Service Request"],
                 priority="medium", state_id=canon["Baru"],
                 description_html="<p>Vendor audit membutuhkan akses VPN read-only ke dashboard settlement selama periode audit Q4. Tanpa akses ini jadwal audit molor.</p>"),
        9: dict(name="Pengadaan laptop untuk support desk", type_id=types["Service Request"],
                 priority="low", state_id=canon["Siap Dikerjakan"],
                 description_html="<p>Tim support desk butuh 2 unit laptop pengganti; unit lama lemot dan menghambat respons tiket shift malam.</p>"),
        10: dict(name="Transaksi QRIS timeout massal di Payment Gateway", type_id=types["Incident"],
                  priority="urgent", state_id=canon["Investigasi"], label_ids=[labels["payment-gateway"]],
                  description_html="<p>Sejak 08:00 WIB >40% transaksi QRIS timeout di sisi gateway; merchant melaporkan dana terdebet tanpa notifikasi sukses. Investigasi awal mengarah ke koneksi pool ke core banking.</p>"),
    }
    for seq, body in rewrites.items():
        call("PATCH", f"{P}/issues/{m[seq]['id']}/", body)
        print(f"rewrite seq {seq}")
    news = [
        dict(name="File settlement prepaid gagal terkirim ke bank", type_id=types["Incident"],
             priority="high", state_id=canon["Baru"], label_ids=[labels["prepaid-filetransfer"]],
             description_html="<p>File settlement H+0 tidak terkirim ke bank mitra; finance tidak bisa rekonsiliasi. Cek terakhir log filetransfer menunjukkan koneksi SFTP terputus.</p>"),
        dict(name="Koneksi Postgres Primary sempat terputus saat failover", type_id=types["Incident"],
             priority="medium", state_id=canon["Selesai"], label_ids=[labels["postgres"]],
             description_html="<p>Failover otomatis 03:12 menyebabkan jeda koneksi ±90 detik; transaksi antre lalu pulih sendiri. Ditutup setelah verifikasi tidak ada data ganda.</p>"),
        dict(name="Timeout berulang ISO8583 ke core banking", type_id=types["Problem"],
             priority="high", state_id=canon["Known Error"], label_ids=[labels["prepaid-iso"]],
             description_html="<p>Timeout ISO8583 berulang tiap beban puncak; insiden pemicu sudah 3x bulan ini. Dugaan root cause: connection pool kecil di sisi adapter.</p>"),
        dict(name="Reset API key merchant untuk Web API", type_id=types["Service Request"],
             priority="medium", state_id=canon["Triase"], label_ids=[labels["web-api"]],
             description_html="<p>Merchant meminta rotasi API key karena personel lama resign. Perlu verifikasi kepemilikan sebelum reset.</p>"),
        dict(name="Upgrade patch minor Postgres Primary", type_id=types["Change"],
             priority="medium", state_id=canon["Siap Dikerjakan"], label_ids=[labels["postgres"]],
             description_html="<p>Patch minor menutup celah keamanan pada replikasi. Rencana eksekusi di window maintenance Minggu 01:00–03:00 dengan rollback snapshot.</p>"),
        dict(name="Perubahan limit transaksi QRIS malam hari", type_id=types["Change"],
             priority="low", state_id=canon["Baru"], label_ids=[labels["payment-gateway"]],
             description_html="<p>Usulan menaikkan limit QRIS pukul 22:00–06:00 mengikuti kebijakan bank. Butuh persetujuan risk sebelum CAB.</p>"),
    ]
    for b in news:
        r = call("POST", f"{P}/issues/", b)
        print(f"create {b['name']} {(r.get('sequence_id') if r else '')}")
```

- [ ] **Step 2: Dry-run → eksekusi → verifikasi**

Run dry-run, periksa 3 rewrite + 6 create.
Run eksekusi.
Verifikasi SQL: total issues = 9; `... AND type_id IS NULL` = 0; `SELECT sequence_id, name FROM issues WHERE project_id='...' ORDER BY 1;` → 9 baris sesuai tabel.

- [ ] **Step 3: Hapus 2 label lama** (sekarang tak dipakai): `DELETE .../labels/<id>/` untuk `incident` dan `service-request` (resolve id via GET labels). Verifikasi: tinggal 5 label.

---

### Task 7: Intake — ganti 2 lama dengan 4 kasus Jev

Endpoint: `DELETE .../intake-issues/:pk/` = `destroy_issue` (pk = workitem/record id dari list; jika butuh `issue_id`, baca dari GET list dulu), `POST .../intake-issues/` (`CreateIntakeIssue`: `name, description?, priority?` — jika field ditolak, baca struct di `intake.rs:40`).

Kasus (spec Section 5): debet ganda QRIS (Incident/urgent — harapan), user dashboard settlement (Service Request/medium), settlement tertunda tiap 02:00 (Problem/high), restart rutin filetransfer (Change/low). Deskripsi 2 kalimat tiap kasus.

- [ ] **Step 1: Tambah logika intake + jalankan**

```python
INTAKES = [
    ("Dana nasabah terdebet 2x untuk 1 transaksi QRIS",
     "Nasabah melaporkan saldo terpotong dua kali untuk satu pembayaran QRIS pagi ini. Butuh cek log gateway dan Until rekonsiliasi sebelum refund.", "urgent"),
    ("Minta pembuatan user dashboard settlement untuk tim finance",
     "Tim finance butuh 3 akun read-only dashboard settlement untuk rekonsiliasi harian. Akun diminta aktif minggu depan.", "medium"),
    ("Setiap jam 02:00 settlement tertunda 30 menit, sudah 5 hari",
     "Job settlement selalu molor 30 menit tiap pukul 02:00 selama 5 hari berturut-turut. Pola berulang, indikasi masalah kapasitas.", "high"),
    ("Minta penjadwalan restart rutin server filetransfer tiap Minggu",
     "Usulan restart rutin tiap Minggu 04:00 untuk mencegah penumpukan memori di server filetransfer. Minta dijadwalkan sebagai perubahan berulang.", "low"),
]

def task_intake():
    cur = get(f"{P}/intake-issues/")
    arr = cur if isinstance(cur, list) else cur.get("results", [])
    for it in arr:
        if it.get("status") == -2 or it.get("status") == "pending":
            call("DELETE", f"{P}/intake-issues/{it['id']}/")
            print(f"destroy intake {it['id']}")
    for name, desc, prio in INTAKES:
        call("POST", f"{P}/intake-issues/", {"name": name, "description": desc, "priority": prio})
        print(f"create intake {name}")
```

- [ ] **Step 2: Verifikasi 4 pending**

SQL: `SELECT count(*) FROM intake_issues WHERE project_id='...' AND status = -2;` → 4.

---

### Task 8: Modules, cycles, services

Endpoint: modules `PATCH .../modules/:pk/` (rename) + `POST .../modules/:module_id/issues/` (body `{"issue_ids"}` — jika ditolak baca `module.rs` create handler); cycles `DELETE .../cycles/:pk/` + `POST .../cycles/` (body `{"name","start_date","end_date"}` tanggal ISO `YYYY-MM-DD`; cycle baru Okt 2026) + `POST .../cycles/:cycle_id/cycle-issues/`; services `DELETE .../services/:pk/` (Svc B, setelah cek `service_issues` kosong) + `POST .../service-issues/` (body merujuk service+issue — jika ditolak baca `service::issues_create` struct).

- [ ] **Step 1: Modules — rename + isi**

Rename: "Service Catalog (System)"→"Katalog Layanan Payment", "Request Fulfilment (Process)"→"Pemenuhan Request", "Knowledge Base (Area)"→"Known Error / Basis Pengetahuan" (resolve id via GET modules, match nama lama).
Isi (resolve issue id via `seq_map` Task 6): Katalog ← tiket ber-label payment (QRIS timeout, limit QRIS); Pemenuhan ← 3 Service Request; KEDB ← Problem + failover Postgres.

- [ ] **Step 2: Cycles — hapus tutorial + buat shift**

DELETE 2 cycle lama ("Week 1…", "Week 2…"). POST baru `{"name": "Shift Support Payment — Okt 2026", "start_date": "2026-10-01", "end_date": "2026-10-31"}`. Tambahkan 3 tiket urgent/high (QRIS timeout, settlement gagal, ISO8583) via cycle-issues.

- [ ] **Step 3: Services — bersih + link**

Cek: `SELECT service_id, count(*) FROM service_issues GROUP BY service_id;` — jika `Svc B` punya link, JANGAN hapus (catat, lanjut). Jika kosong: `DELETE .../services/<svc-b-id>/`.
Link (service name → tiket, resolve id via GET services + seq_map): Payment Gateway ← {QRIS timeout, limit QRIS}; prepaid-filetransfer-out-reg ← {settlement gagal}; prepaid-iso ← {ISO8583}; Postgres Primary ← {failover, upgrade patch}; Web API ← {reset API key}.

- [ ] **Step 4: Verifikasi**

SQL: modules = 3 nama baru; cycles aktif = 1 ("Shift Support…"); `SELECT count(*) FROM service_issues;` ≥ 7 link.

---

### Task 9: Verifikasi akhir + bersih-bersih

- [ ] **Step 1: Pemeriksaan SQL final**

```bash
PID=6debab4c-966e-4e77-aaf3-2d84055208b9
Q(){ docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -t -c "$1"; }
Q "SELECT count(*) FROM issues WHERE project_id='$PID';"            # → 9
Q "SELECT count(*) FROM issues WHERE project_id='$PID' AND type_id IS NULL;"  # → 0
Q "SELECT count(*) FROM (SELECT name FROM states WHERE project_id='$PID' GROUP BY name HAVING count(*)>1) d;"  # → 0
Q "SELECT count(*) FROM labels WHERE project_id='$PID';"           # → 5
Q "SELECT count(*) FROM intake_issues WHERE project_id='$PID' AND status=-2;" # → 4
```

- [ ] **Step 2: Panel triage tampil**

Buka intake di browser (hard refresh), klik tiap 4 kasus → panel "Triage suggestion" muncul (data sudah di-seed Jev otomatis; jika kosong >3 menit, cek `SELECT issue_id, category, severity FROM intake_triage_suggestions;` dan log worker).

- [ ] **Step 3: Jev mengklasifikasi 4 kasus baru**

SQL: `SELECT i.sequence_id, s.category, s.severity, s.confidence FROM intake_triage_suggestions s JOIN issues i ON i.id = s.issue_id ORDER BY i.sequence_id;`
Expected: 4 baris untuk kasus baru dengan kategori Incident/Service Request/Problem/Change. (Klasifikasi jalan otomatis via push job saat intake dibuat + sweep beat 1 menit; tunggu ≤3 menit.)

- [ ] **Step 4: Hapus token skrip + laporkan**

Run: `docker exec plane-for-itsm-plane-db-1 psql -U plane -d plane -c "DELETE FROM api_tokens WHERE token='itsm-makeover-20260929';"`
Laporkan ke user: ringkasan perubahan + sisa manual (jika ada state default yang menolak dihapus di Task 5).
