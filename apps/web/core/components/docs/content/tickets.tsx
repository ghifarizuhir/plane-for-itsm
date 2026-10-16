/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTermTable, DocsTopicLink } from "../blocks";

export default function TicketsContent() {
  return (
    <>
      <DocsParagraph>
        Ticket adalah jantung platform ini. Setiap pekerjaan — dari gangguan kecil sampai major incident — dicatat,
        di-assign, dan dilacak progresnya sebagai ticket di dalam sebuah project.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat ticket dengan cepat (tombol New ticket atau shortcut C), mengatur propertinya, berdiskusi lewat komentar
        dengan mention, melampirkan file, menautkan ticket dengan relasi, memecah pekerjaan menjadi sub-items, dan
        menandai ticket sebagai war room untuk insiden besar.
      </DocsParagraph>
      <DocsSectionHeading>Properti ticket</DocsSectionHeading>
      <DocsTermTable
        terms={[
          { term: "State", meaning: "Status ticket di alur kerja project (bisa dikustomisasi admin)." },
          { term: "Assignee", meaning: "Penanggung jawab ticket — bisa lebih dari satu." },
          { term: "Priority", meaning: "Tingkat urgensi: urgent, high, medium, low, none." },
          { term: "Start & Target date", meaning: "Kapan pekerjaan dimulai dan ditargetkan selesai." },
          { term: "Labels", meaning: "Penanda bebas untuk kategorisasi (mis. layanan atau jenis gangguan)." },
          { term: "Estimate", meaning: "Perkiraan ukuran pekerjaan sesuai skala project." },
          { term: "Sprints / Tracks", meaning: "Keanggotaan ticket dalam periode atau kelompok pekerjaan." },
          { term: "Parent", meaning: "Ticket induk saat pekerjaan dipecah menjadi sub-items." },
        ]}
      />
      <DocsSectionHeading>Alur hidup ticket</DocsSectionHeading>
      <DocsStepList
        steps={[
          { title: "Dibuat", description: "Ticket baru masuk dari kamu, dari draft, atau dari Intake." },
          {
            title: "Dikerjakan",
            description: "Status berjalan (mis. In Progress); assignee dan prioritas sudah jelas.",
          },
          { title: "Selesai", description: "Status masuk ke grup Done, atau Cancelled bila tidak dikerjakan." },
        ]}
      />
      <DocsCallout variant="tip">
        Gunakan relasi (duplicate, blocking, blocked by) saat menangani insiden berantai — ini membantu tim melihat
        ketergantungan antar pekerjaan. Untuk insiden besar, lanjutkan ke{" "}
        <DocsTopicLink slug="war-rooms">War Rooms</DocsTopicLink>.
      </DocsCallout>
    </>
  );
}
