/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import {
  DocsCallout,
  DocsParagraph,
  DocsPracticeMatrix,
  DocsSectionHeading,
  DocsStepList,
  DocsTopicLink,
} from "../blocks";
import { ITIL4_MATRIX_COLUMNS, ITIL4_MATRIX_ROWS } from "./itil4-matrix-data";

export default function Itil4FlowContent() {
  return (
    <>
      <DocsParagraph>
        Halaman ini merangkum contoh penerapan dari nol untuk satu organisasi — misalnya perusahaan dengan tiga layanan
        internal: email, aplikasi keuangan, dan jaringan kantor.
      </DocsParagraph>

      <DocsSectionHeading>Langkah penerapan</DocsSectionHeading>
      <DocsStepList
        steps={[
          {
            title: "Tentukan struktur",
            description:
              "Satu workspace untuk organisasi; satu project per layanan besar, atau satu project operasional dengan Service catalog untuk rinciannya.",
          },
          {
            title: "Bangun service catalog",
            description:
              "Daftarkan setiap layanan dengan owner dan criticality; catat dependency antar-layanan supaya dampak gangguan terlihat.",
          },
          {
            title: "Siapkan jenis ticket",
            description:
              "Aktifkan Incident, Problem, Change, dan Improvement di project; buat jenis Request dengan requires service untuk permintaan standar.",
          },
          {
            title: "Buka pintu masuk",
            description: "Aktifkan Intake; sediakan form untuk pengguna dan intake source untuk alert monitoring.",
          },
          {
            title: "Susun queue standar",
            description:
              "Buat queue: Triage, Incident aktif, Problem, Request per layanan, dan Change mendatang — jadikan public agar dipakai seluruh tim.",
          },
          {
            title: "Tetapkan ritme",
            description:
              "Sprint dua minggu untuk pekerjaan terjadwal dan improvement; Track untuk investigasi problem dan inisiatif besar; War Room saat major incident.",
          },
          {
            title: "Tutup loop pengetahuan",
            description:
              "Setelah insiden atau problem selesai, perbarui Knowledge base dan tautkan artikelnya ke ticket.",
          },
          {
            title: "Perbaiki berulang",
            description: "Jadikan temuan review sebagai ticket Improvement; pantau tren di Reports setiap bulan.",
          },
        ]}
      />

      <DocsSectionHeading>Peta praktik ke fitur (ringkasan)</DocsSectionHeading>
      <DocsPracticeMatrix columns={ITIL4_MATRIX_COLUMNS} rows={ITIL4_MATRIX_ROWS} />

      <DocsCallout variant="tip" title="Mulai kecil, ulangi">
        Satu layanan, satu queue, satu sprint sudah cukup untuk mulai. Tambah praktik dan otomasi setelah alur dasarnya
        jalan — dokumentasi dan kebiasaan lebih penting daripada konfigurasi lengkap di hari pertama.
      </DocsCallout>

      <DocsParagraph>
        Baca ulang detail tiap praktik: <DocsTopicLink slug="itil4-overview">ITIL4 di platform ini</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-incident">Incident</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-problem">Problem</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-service-request">Service request</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-change-improvement">Change &amp; improvement</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
