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
  DocsTermTable,
} from "../blocks";

export default function OverviewContent() {
  return (
    <>
      <DocsParagraph>
        Platform ini adalah sistem manajemen layanan IT (IT service management) untuk tim support dan operasional. Di
        dalamnya kamu mengelola insiden, permintaan layanan, dan pengetahuan tim — dari laporan yang masuk lewat service
        desk, dikerjakan sebagai ticket, sampai didokumentasikan sebagai artikel pengetahuan. Basisnya adalah Plane,
        alat manajemen proyek open-source, yang disesuaikan untuk alur kerja ITSM.
      </DocsParagraph>

      <DocsSectionHeading>Satu tempat untuk kerja harian tim</DocsSectionHeading>
      <DocsParagraph>Empat area utama yang akan sering kamu pakai:</DocsParagraph>
      <DocsFeatureGrid>
        <DocsFeatureCard title="Tickets">Pekerjaan operasional harian: insiden, permintaan, dan tugas.</DocsFeatureCard>
        <DocsFeatureCard title="Sprints & Tracks">
          Perencanaan pekerjaan dalam periode berulang atau kelompok besar.
        </DocsFeatureCard>
        <DocsFeatureCard title="Services & War Rooms">
          Katalog layanan beserta ruang koordinasi saat insiden besar.
        </DocsFeatureCard>
        <DocsFeatureCard title="Knowledge base">
          Artikel dan panduan yang bisa dibaca seluruh tim — bahkan dipublikasikan keluar.
        </DocsFeatureCard>
      </DocsFeatureGrid>

      <DocsSectionHeading>Istilah yang kamu temui</DocsSectionHeading>
      <DocsParagraph>
        Beberapa fitur memakai istilah yang berbeda dari produk project management pada umumnya. Berikut padanannya:
      </DocsParagraph>
      <DocsTermTable
        terms={[
          { term: "Ticket", meaning: "Unit kerja utama (work item): satu insiden, permintaan, atau tugas." },
          { term: "Sprint", meaning: "Periode kerja berulang (cycle) dengan tanggal mulai dan selesai." },
          { term: "Track", meaning: "Pengelompokan ticket menuju satu sasaran atau fase (module)." },
          { term: "Queue", meaning: "Daftar ticket yang tersaring dengan filter tersimpan (view)." },
          { term: "Knowledge base", meaning: "Kumpulan halaman dokumen tim (pages)." },
          { term: "Inbox", meaning: "Pemberitahuan: mention, assignment, komentar, perubahan status." },
          { term: "Reports", meaning: "Analitik workspace: tren dan distribusi pekerjaan." },
        ]}
      />

      <DocsSectionHeading>Workspace dan project</DocsSectionHeading>
      <DocsParagraph>
        Workspace adalah ruang kerja organisasimu — di sinilah member, pengaturan global, dan semua project berada.
        Project adalah unit kerja di dalam workspace, misalnya per produk, per layanan, atau per tim. Setiap ticket
        selalu berada di dalam sebuah project.
      </DocsParagraph>

      <DocsSectionHeading>Peran</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Admin",
            meaning: "Akses penuh ke seluruh workspace, termasuk pengaturan member dan konfigurasi global.",
          },
          {
            term: "Member",
            meaning: "Bisa membaca, membuat, dan mengubah pekerjaan di dalam project tempat ia bergabung.",
          },
          {
            term: "Guest",
            meaning: "Akses terbatas untuk pihak eksternal; biasanya hanya melihat project tertentu.",
          },
        ]}
      />

      <DocsCallout variant="info" title="Baru pertama kali?">
        Mulai dari bagian <b>Untuk Member</b> di sidebar kiri untuk mengenal fitur pribadimu, lalu lanjut ke{" "}
        <b>Untuk Project</b> untuk memahami isi setiap project.
      </DocsCallout>
    </>
  );
}
