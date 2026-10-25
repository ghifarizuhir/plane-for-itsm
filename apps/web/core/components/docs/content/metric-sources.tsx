/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import {
  DocsCallout,
  DocsCodeBlock,
  DocsInlineCode,
  DocsParagraph,
  DocsSectionHeading,
  DocsStepList,
  DocsTermTable,
  DocsTopicLink,
} from "../blocks";

export default function MetricSourcesContent() {
  return (
    <>
      <DocsParagraph>
        Metric sources menghubungkan platform dengan sistem monitoring eksternal. Platform tidak menyimpan sampel
        mentah; yang dikirim adalah bucket metrik yang sudah diagregasi — <DocsInlineCode>count</DocsInlineCode>,{" "}
        <DocsInlineCode>sum</DocsInlineCode>, <DocsInlineCode>min</DocsInlineCode>, <DocsInlineCode>max</DocsInlineCode>
        , <DocsInlineCode>avg</DocsInlineCode>, dan <DocsInlineCode>last</DocsInlineCode> untuk satu rentang waktu. Data
        yang masuk langsung tampil di tab <strong>Monitoring</strong> pada halaman service.
      </DocsParagraph>
      <DocsCallout variant="info" title="Khusus admin workspace">
        Halaman <DocsInlineCode>Settings → Metric sources</DocsInlineCode> hanya bisa dibuka admin workspace. Token
        source bersifat rahasia dan hanya ditampilkan lengkap di halaman tersebut — perlakukan seperti password.
      </DocsCallout>

      <DocsSectionHeading>Alur integrasi</DocsSectionHeading>
      <DocsStepList
        steps={[
          {
            title: "Buat metric source",
            description:
              "Buka Settings → Metric sources, klik Add source, lalu beri nama yang mudah dikenali (misalnya “Prometheus production”).",
          },
          {
            title: "Petakan key eksternal ke service",
            description:
              "Tambahkan mapping dari key yang dikirim agent (misalnya “payment-api”) ke service di workspace. Fallback dipakai bila key tidak ada di mapping.",
          },
          {
            title: "Salin token",
            description:
              "Token berformat plane_ms_… dan hanya terlihat lengkap di halaman ini. Simpan di secret manager agent Anda.",
          },
          {
            title: "Kirim bucket pertama",
            description:
              "Gunakan contoh curl di bawah untuk memastikan token dan payload benar, lalu ganti dengan agent pengirim berkala.",
          },
          {
            title: "Verifikasi",
            description:
              "Buka service tujuan → tab Monitoring. Metrik muncul otomatis setelah request pertama diterima.",
          },
        ]}
      />

      <DocsSectionHeading>Format payload</DocsSectionHeading>
      <DocsParagraph>
        Kirim satu request <DocsInlineCode>POST</DocsInlineCode> berisi array <DocsInlineCode>metrics</DocsInlineCode>{" "}
        (1–1000 item). Seluruh batch divalidasi lebih dulu: jika satu item salah, tidak ada data yang ditulis.
      </DocsParagraph>
      <DocsCodeBlock title="Contoh request">
        {`curl -X POST "\${API_BASE_URL}/api/inbound/metrics/<token>/" \\
  -H 'Content-Type: application/json' \\
  -d '{
    "metrics": [{
      "service": "payment-api",
      "key": "http.server.duration.p95",
      "display_name": "HTTP latency p95",
      "unit": "ms",
      "resolution": 60,
      "bucket_start": "2026-10-24T10:00:00Z",
      "count": 60,
      "sum": 7200,
      "min": 80,
      "max": 250,
      "avg": 120,
      "last": 130
    }]
  }'`}
      </DocsCodeBlock>
      <DocsTermTable
        terms={[
          {
            term: "service / service_id",
            meaning:
              "Pilih salah satu. service_id adalah UUID service di workspace dan menang atas service. service adalah key dari sistem monitoring yang di-resolve lewat mapping source.",
          },
          {
            term: "key",
            meaning:
              "Nama metrik, 1–200 karakter, hanya huruf, angka, titik, titik dua, garis bawah, dan strip; harus diawali huruf atau angka.",
          },
          { term: "display_name", meaning: "Nama tampilan metrik, maksimal 255 karakter (opsional)." },
          { term: "unit", meaning: "Satuan metrik, misalnya ms, %, atau req/s. Maksimal 50 karakter (opsional)." },
          {
            term: "resolution",
            meaning: "Lebar bucket dalam detik: 60, 300, 600, 1800, atau 3600.",
          },
          {
            term: "bucket_start",
            meaning:
              "Awal bucket dalam format RFC3339. Harus sejajar dengan resolution (detik ke-0, menit kelipatan 5/10/30, atau jam penuh).",
          },
          {
            term: "count, sum, min, max, avg, last",
            meaning:
              "Statistik bucket. Semua opsional, tetapi minimal satu harus terisi. min harus ≤ max, dan avg harus berada di antara keduanya.",
          },
        ]}
      />

      <DocsSectionHeading>Resolusi, alignment, dan retensi</DocsSectionHeading>
      <DocsParagraph>
        Setiap seri diidentifikasi oleh <DocsInlineCode>key</DocsInlineCode>,{" "}
        <DocsInlineCode>resolution</DocsInlineCode>, dan <DocsInlineCode>bucket_start</DocsInlineCode>. Kirim ulang
        bucket yang sama akan menimpa data lama (upsert), jadi retry aman dan idempoten.
      </DocsParagraph>
      <DocsTermTable
        terms={[
          { term: "60 detik", meaning: "Disimpan 48 jam." },
          { term: "300 detik", meaning: "Disimpan 14 hari." },
          { term: "600 detik", meaning: "Disimpan 30 hari." },
          { term: "1800 detik", meaning: "Disimpan 90 hari." },
          { term: "3600 detik", meaning: "Disimpan 365 hari." },
        ]}
      />
      <DocsParagraph>
        <DocsInlineCode>bucket_start</DocsInlineCode> tidak boleh lebih dari 5 menit di masa depan, dan tidak boleh
        lebih tua dari masa retensi resolusinya. Kirim bucket tepat setelah periode selesai — jangan menunggu terlalu
        lama.
      </DocsParagraph>

      <DocsSectionHeading>Mapping service</DocsSectionHeading>
      <DocsParagraph>
        Agent cukup mengirim key yang dikenal sistem monitoring; pemetaan ke service dilakukan di form source. Urutan
        resolusi: <DocsInlineCode>service_id</DocsInlineCode> → mapping <DocsInlineCode>service_map</DocsInlineCode> →{" "}
        <DocsInlineCode>fallback_service_id</DocsInlineCode>. Service tujuan harus masih ada di workspace; jika tidak,
        request ditolak dengan error <DocsInlineCode>service</DocsInlineCode>.
      </DocsParagraph>
      <DocsCallout variant="tip" title="Satu metrik, banyak service">
        Gunakan nama metrik yang konsisten antar service. Definisi metrik dibuat otomatis saat pertama kali muncul, dan
        nama tampilan atau satuan bisa dilengkapi belakangan tanpa mengubah key.
      </DocsCallout>

      <DocsSectionHeading>Contoh agent Python</DocsSectionHeading>
      <DocsParagraph>
        Skrip berikut hanya memakai standard library. Ganti <DocsInlineCode>collect_buckets()</DocsInlineCode> dengan
        query ke sistem monitoring Anda, lalu jalankan lewat cron atau sebagai service.
      </DocsParagraph>
      <DocsCodeBlock title="metrics-agent.py">
        {`import json
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone

API_BASE_URL = "https://api.example.com"
TOKEN = "plane_ms_..."
INTERVAL_SECONDS = 60


def collect_buckets():
    # Ganti dengan query ke sistem monitoring Anda.
    bucket_start = datetime.now(timezone.utc).replace(second=0, microsecond=0)
    return [
        {
            "service": "payment-api",
            "key": "http.server.duration.p95",
            "display_name": "HTTP latency p95",
            "unit": "ms",
            "resolution": 60,
            "bucket_start": bucket_start.isoformat().replace("+00:00", "Z"),
            "count": 60,
            "sum": 7200,
            "min": 80,
            "max": 250,
            "avg": 120,
            "last": 130,
        }
    ]


def send(metrics):
    request = urllib.request.Request(
        f"{API_BASE_URL}/api/inbound/metrics/{TOKEN}/",
        data=json.dumps({"metrics": metrics}).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            return response.status, response.read().decode()
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode()


while True:
    status, body = send(collect_buckets())
    print(status, body)
    time.sleep(INTERVAL_SECONDS)`}
      </DocsCodeBlock>

      <DocsSectionHeading>Respons dan troubleshooting</DocsSectionHeading>
      <DocsTermTable
        terms={[
          { term: "202 Accepted", meaning: '{"accepted": N, "metrics_created": N} — batch tersimpan.' },
          {
            term: "400 Bad Request",
            meaning:
              "Ada item tidak valid. Body berisi errors[{index, field, message}], maksimal 50 error, dengan truncated: true bila dipotong.",
          },
          { term: "403 Forbidden", meaning: "Token valid tetapi source sedang nonaktif." },
          {
            term: "404 Not Found",
            meaning: "Token tidak dikenal atau source sudah dihapus. Periksa token atau rotasi.",
          },
        ]}
      />
      <DocsCallout variant="warning" title="Batch bersifat all-or-nothing">
        Perbaiki semua item yang dilaporkan sebelum mengirim ulang. Karena upsert idempoten, mengirim ulang batch yang
        sama tidak menghasilkan duplikasi.
      </DocsCallout>

      <DocsSectionHeading>Operasional</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Rotate token",
            meaning: "Menerbitkan token baru dan langsung mematikan token lama. Perbarui agent segera setelah rotate.",
          },
          {
            term: "Nonaktifkan source",
            meaning: "Agent berhenti diterima (403), tetapi konfigurasi dan bucket lama tetap tersimpan.",
          },
          {
            term: "Hapus source",
            meaning: "Ingest berhenti permanen. Bucket lama tetap tersedia sampai masa retensinya habis.",
          },
        ]}
      />
      <DocsCallout variant="tip">
        Setelah data mengalir, buka <DocsTopicLink slug="services">Services</DocsTopicLink> untuk melihat tab Monitoring
        dan membandingkan metrik dengan status kesehatan service.
      </DocsCallout>
    </>
  );
}
