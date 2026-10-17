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

export default function Itil4IncidentContent() {
  return (
    <>
      <DocsParagraph>
        Incident management bertujuan memulihkan layanan secepat mungkin dan mencatat pengetahuan dari setiap gangguan.
        Targetnya bukan mencari siapa yang salah, melainkan meminimalkan dampak dan mencegah terulang.
      </DocsParagraph>

      <DocsSectionHeading>Di platform ini</DocsSectionHeading>
      <DocsFeatureGrid>
        <DocsFeatureCard title="Intake">
          Pintu masuk laporan: form, email, webhook, dan alert otomatis dari monitoring.
        </DocsFeatureCard>
        <DocsFeatureCard title="Tickets & Services">
          Jenis ticket Incident, prioritas, dan tautan ke layanan terdampak.
        </DocsFeatureCard>
        <DocsFeatureCard title="War Rooms">
          Ruang koordinasi saat major incident: peran, runbook, chat, dan timeline.
        </DocsFeatureCard>
        <DocsFeatureCard title="Queues & Sprints">
          Antrean per layanan untuk triage, dan sprint bila perbaikannya perlu direncanakan.
        </DocsFeatureCard>
      </DocsFeatureGrid>

      <DocsSectionHeading>Alur insiden</DocsSectionHeading>
      <DocsStepList
        steps={[
          {
            title: "Laporan masuk lewat Intake",
            description:
              "Pengguna melapor lewat form atau email, atau alert monitoring masuk otomatis lewat Intake source. Alert dengan pola sama digabung (occurrence count bertambah) alih-alih membuat ticket baru.",
          },
          {
            title: "Triage: klasifikasi, prioritas, layanan",
            description:
              "Tentukan jenis ticket Incident, prioritas (urgent sampai low), dan tautkan Service yang terdampak. Untuk jenis Incident, tautan layanan wajib ada saat accept.",
          },
          {
            title: "Kerjakan dari queue",
            description:
              "Ticket masuk queue tim (misalnya per layanan). Gunakan relasi duplicate atau blocking untuk menandai kaitan antar-insiden.",
          },
          {
            title: "Major incident: nyalakan War Room",
            description:
              "Untuk gangguan berdampak besar, aktifkan War Room pada ticket: severity diturunkan dari prioritas, runbook otomatis disiapkan, dan peran commander/comms/scribe/responder diisi.",
          },
          {
            title: "Resolve dan dokumentasikan",
            description:
              "Catat langkah pemulihan di komentar, buat atau tautkan artikel Knowledge base, lalu tandai ticket selesai.",
          },
          {
            title: "Review dan tindak lanjut",
            description: "Kalau akar masalah belum diketahui, buat ticket Problem dan hubungkan ke insiden terkait.",
          },
        ]}
      />

      <DocsSectionHeading>Peran & ritme</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Helpdesk L1",
            meaning: "Memantau queue triage, mengklasifikasi laporan, dan menyelesaikan insiden sederhana.",
          },
          { term: "Helpdesk L2 / SME", meaning: "Menangani insiden yang butuh keahlian layanan tertentu." },
          {
            term: "Incident commander",
            meaning: "Memimpin War Room saat major incident; perannya diatur di dalam War Room.",
          },
          {
            term: "Problem manager",
            meaning: "Menindaklanjuti insiden berulang sebagai problem (lihat Problem management).",
          },
        ]}
      />

      <DocsCallout variant="warning" title="Kesehatan service belum monitoring nyata">
        Indikator di halaman Services saat ini bersifat katalog (belum terhubung monitoring real-time). Jangan
        mengandalkannya sebagai satu-satunya alarm — sambungkan alert monitoring lewat Intake source.
      </DocsCallout>

      <DocsParagraph>
        Terkait: <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>,{" "}
        <DocsTopicLink slug="intake">Intake</DocsTopicLink>, <DocsTopicLink slug="war-rooms">War Rooms</DocsTopicLink>,{" "}
        <DocsTopicLink slug="queues">Queues</DocsTopicLink>, <DocsTopicLink slug="services">Services</DocsTopicLink>,
        dan <DocsTopicLink slug="knowledge-base">Knowledge base</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
