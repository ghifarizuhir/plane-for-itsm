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
  DocsTermTable,
  DocsTopicLink,
} from "../blocks";
import { ITIL4_MATRIX_COLUMNS, ITIL4_MATRIX_ROWS } from "./itil4-matrix-data";

export default function Itil4OverviewContent() {
  return (
    <>
      <DocsParagraph>
        ITIL4 adalah kerangka kerja pengelolaan layanan TI yang menekankan alur nilai: dari permintaan atau gangguan,
        sampai layanan pulih dan pengetahuan bertambah. Platform ini menerjemahkan praktik-praktik ITIL4 menjadi fitur
        yang bisa langsung dipakai tim support dan operasional.
      </DocsParagraph>

      <DocsSectionHeading>Peta praktik ke fitur</DocsSectionHeading>
      <DocsParagraph>
        Lima praktik utama ITIL4 dan fitur platform yang mendukungnya. Tanda ● berarti sering dipakai, ○ kadang dipakai,
        dan – tidak dipakai.
      </DocsParagraph>
      <DocsPracticeMatrix columns={ITIL4_MATRIX_COLUMNS} rows={ITIL4_MATRIX_ROWS} />

      <DocsSectionHeading>Istilah ITIL4 ↔ istilah di app</DocsSectionHeading>
      <DocsTermTable
        terms={[
          { term: "Incident", meaning: "Gangguan layanan yang perlu dipulihkan — jenis ticket Incident di project." },
          { term: "Problem", meaning: "Akar masalah di balik insiden berulang — jenis ticket Problem." },
          {
            term: "Known error",
            meaning: "Problem yang akarnya sudah diketahui — artikel Knowledge base yang ditautkan ke ticket.",
          },
          {
            term: "Service request",
            meaning: "Permintaan standar dari pengguna — alur Intake dengan jenis ticket Request.",
          },
          {
            term: "Change",
            meaning: "Perubahan terencana pada layanan — jenis ticket Change, direncanakan lewat Sprint dan Track.",
          },
          {
            term: "Continual improvement",
            meaning: "Perbaikan berkelanjutan — jenis ticket Improvement, dikelola lewat Track.",
          },
          { term: "Queue", meaning: "Antrean kerja tersaring: daftar ticket dengan filter tersimpan (view)." },
        ]}
      />

      <DocsSectionHeading>Mulai dari mana</DocsSectionHeading>
      <DocsParagraph>
        Setiap praktik punya halaman sendiri dengan alur langkah, peran, dan fitur yang dipakai. Mulailah dari{" "}
        <DocsTopicLink slug="itil4-incident">Incident management</DocsTopicLink> karena alur itulah yang paling sering
        dijalankan, lalu pelajari <DocsTopicLink slug="itil4-problem">Problem management</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-service-request">Service request</DocsTopicLink>,{" "}
        <DocsTopicLink slug="itil4-change-improvement">Change &amp; improvement</DocsTopicLink>, dan terakhir{" "}
        <DocsTopicLink slug="itil4-flow">alur organisasi end-to-end</DocsTopicLink>.
      </DocsParagraph>

      <DocsCallout variant="tip" title="Mulai kecil">
        Tidak perlu mengaktifkan semua praktik sekaligus. Mulai dari satu project, satu layanan, dan satu queue — tambah
        praktik lain saat tim sudah nyaman.
      </DocsCallout>
    </>
  );
}
