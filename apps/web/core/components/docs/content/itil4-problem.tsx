/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import {
  DocsCallout,
  DocsFeatureCard,
  DocsFeatureGrid,
  DocsParagraph,
  DocsSectionHeading,
  DocsStepList,
  DocsTermTable,
  DocsTopicLink,
} from "../blocks";

export default function Itil4ProblemContent() {
  return (
    <>
      <DocsParagraph>
        Problem management mencari akar masalah di balik insiden yang berulang atau berdampak besar. Tujuannya bukan
        memadamkan kebakaran, melainkan menghilangkan sumbernya — lalu mengubah temuannya menjadi pengetahuan tim.
      </DocsParagraph>

      <DocsSectionHeading>Di platform ini</DocsSectionHeading>
      <DocsFeatureGrid>
        <DocsFeatureCard title="Tickets">
          Jenis ticket Problem, dengan relasi ke setiap insiden yang terkait.
        </DocsFeatureCard>
        <DocsFeatureCard title="Tracks">
          Investigasi akar masalah sebagai track tersendiri dengan langkah-langkahnya.
        </DocsFeatureCard>
        <DocsFeatureCard title="Knowledge base">
          Artikel known error: gejala, akar masalah, dan workaround.
        </DocsFeatureCard>
        <DocsFeatureCard title="Queues">
          Antrean masalah yang sedang diselidiki dan yang menunggu perbaikan.
        </DocsFeatureCard>
      </DocsFeatureGrid>

      <DocsSectionHeading>Alur problem</DocsSectionHeading>
      <DocsStepList
        steps={[
          {
            title: "Deteksi pola",
            description:
              "Tandai insiden yang saling terkait lewat relasi duplicate atau relates to, atau simpan queue khusus untuk insiden dengan gejala sama.",
          },
          {
            title: "Buat ticket Problem",
            description:
              "Dokumentasikan gejala yang berulang, dampak, dan insiden yang sudah terjadi. Hubungkan ticket Problem ke insiden-insiden sumbernya.",
          },
          {
            title: "Investigasi akar masalah",
            description:
              "Kelola investigasi sebagai Track (misalnya “Investigasi gangguan login”) dan pecah langkah penyelidikan menjadi sub-items yang bisa ditugaskan.",
          },
          {
            title: "Catat known error",
            description:
              "Begitu akar masalah diketahui, tulis artikel di Knowledge base dan tautkan ke ticket Problem agar tim punya satu rujukan.",
          },
          {
            title: "Rencanakan perbaikan permanen",
            description:
              "Buat ticket Improvement — atau Change, jika mengubah layanan — dan jadwalkan pengerjaannya lewat Sprint.",
          },
          {
            title: "Tutup problem",
            description:
              "Tandai problem selesai setelah perbaikan terpasang dan tidak ada insiden serupa yang terulang.",
          },
        ]}
      />

      <DocsSectionHeading>Peran & ritme</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Problem manager",
            meaning:
              "Member yang ditunjuk memiliki queue problem, memimpin investigasi, dan memutuskan kapan problem ditutup.",
          },
          { term: "SME / tim layanan", meaning: "Membantu analisis akar masalah di area keahliannya." },
          { term: "Ritme", meaning: "Review queue problem setiap minggu; tinjau problem terbuka di awal sprint." },
        ]}
      />

      <DocsCallout variant="info" title="Tidak semua insiden perlu RCA">
        Prioritaskan problem untuk insiden yang berulang, berdampak besar, atau tidak punya workaround. Insiden sekali
        lewat cukup diselesaikan sebagai incident.
      </DocsCallout>

      <DocsParagraph>
        Terkait: <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>,{" "}
        <DocsTopicLink slug="tracks">Tracks</DocsTopicLink>,{" "}
        <DocsTopicLink slug="knowledge-base">Knowledge base</DocsTopicLink>,{" "}
        <DocsTopicLink slug="services">Services</DocsTopicLink>, dan{" "}
        <DocsTopicLink slug="itil4-change-improvement">Change &amp; improvement</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
