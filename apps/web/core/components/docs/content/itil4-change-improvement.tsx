/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTermTable, DocsTopicLink } from "../blocks";

export default function Itil4ChangeImprovementContent() {
  return (
    <>
      <DocsParagraph>
        Dua praktik yang saling melengkapi: change enablement memastikan perubahan pada layanan direncanakan dan
        dikomunikasikan, sedangkan continual improvement memastikan organisasi belajar dan memperbaiki diri.
      </DocsParagraph>

      <DocsSectionHeading>Change enablement</DocsSectionHeading>
      <DocsParagraph>
        Setiap perubahan pada layanan — upgrade, perubahan konfigurasi, penambahan akses — dicatat sebagai ticket Change
        supaya ada jejak, rencana, dan titik koordinasi.
      </DocsParagraph>
      <DocsStepList
        steps={[
          {
            title: "Buat ticket Change",
            description: "Tulis apa yang berubah, mengapa, dan layanan mana yang terdampak (tautkan Service).",
          },
          {
            title: "Nilai risiko",
            description:
              "Pakai prioritas dan deskripsi: rencana langkah, cara rollback, dan jendela waktu yang diinginkan.",
          },
          {
            title: "Rencanakan timing",
            description: "Kelompokkan perubahan terkait dalam Track, lalu jadwalkan eksekusinya lewat Sprint.",
          },
          {
            title: "Komunikasikan dan eksekusi",
            description: "Koordinasi lewat komentar ticket; kalau perubahan memicu insiden, hubungkan lewat relasi.",
          },
          {
            title: "Catat hasil",
            description: "Setelah selesai, catat hasilnya di ticket — termasuk perbedaan dari rencana semula.",
          },
        ]}
      />

      <DocsSectionHeading>Continual improvement</DocsSectionHeading>
      <DocsParagraph>
        Temuan dari review insiden, problem, dan retrospektif sprint tidak berhenti sebagai catatan — jadikan ticket
        Improvement yang dikerjakan seperti pekerjaan lain.
      </DocsParagraph>
      <DocsStepList
        steps={[
          {
            title: "Kumpulkan temuan",
            description: "Sumbernya bisa review problem, checklist pasca-insiden, atau retrospektif Sprint.",
          },
          {
            title: "Buat ticket Improvement",
            description: "Rumuskan perbaikan sekonkret mungkin: kondisi sekarang, target, dan ukurannya.",
          },
          {
            title: "Kelola dalam Track",
            description: "Kelompokkan improvement terkait sebagai satu inisiatif agar progresnya terlihat.",
          },
          {
            title: "Kerjakan dalam Sprint",
            description: "Sisihkan porsi kapasitas tiap sprint untuk improvement — jangan menunggu waktu luang.",
          },
          {
            title: "Ukur dan lanjutkan",
            description: "Tinjau tren di Reports; tetapkan improvement berikutnya dari hasil pengukuran.",
          },
        ]}
      />

      <DocsSectionHeading>Peran & ritme</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Change owner",
            meaning: "Orang yang mengajukan dan bertanggung jawab atas eksekusi sebuah perubahan.",
          },
          {
            term: "Improvement lead",
            meaning: "Member yang menjaga backlog improvement tetap hidup dan terukur.",
          },
          {
            term: "Ritme",
            meaning: "Tinjau backlog improvement di awal sprint; review perubahan yang selesai di akhir sprint.",
          },
        ]}
      />

      <DocsCallout variant="info" title="Belum ada board persetujuan (CAB)">
        Platform ini tidak punya alur approval formal untuk perubahan. Sepakati bersama tim siapa yang boleh menyetujui
        apa, lalu catat persetujuannya di komentar ticket.
      </DocsCallout>

      <DocsParagraph>
        Terkait: <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>,{" "}
        <DocsTopicLink slug="sprints">Sprints</DocsTopicLink>, <DocsTopicLink slug="tracks">Tracks</DocsTopicLink>,{" "}
        <DocsTopicLink slug="services">Services</DocsTopicLink>, dan{" "}
        <DocsTopicLink slug="reports">Reports</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
