/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTermTable, DocsTopicLink } from "../blocks";

export default function ProjectSettingsContent() {
  return (
    <>
      <DocsParagraph>
        Setiap project punya pengaturan sendiri, diakses lewat ikon gear di halaman project. Perubahan di sini berlaku
        untuk semua anggota project — biasanya hanya admin project yang mengubahnya.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa diatur</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "General",
            meaning: "Nama project, identifier (kode ticket), deskripsi, zona waktu, dan akses jaringan.",
          },
          { term: "Members", meaning: "Undang atau kelola anggota project beserta perannya." },
          {
            term: "Features",
            meaning: "Menyalakan atau mematikan fitur: Sprints, Tracks, Queues, Knowledge base, Intake.",
          },
          { term: "States", meaning: "Alur status ticket; bisa dikustomisasi, termasuk grup statusnya." },
          { term: "Work item types", meaning: "Jenis ticket di project ini dan pengaturannya." },
          { term: "Labels", meaning: "Penanda untuk mengategorikan ticket." },
          { term: "Estimates", meaning: "Skala perkiraan ukuran pekerjaan." },
          { term: "Intake sources", meaning: "Sumber masuknya permintaan ke Intake (email, webhook, form)." },
          { term: "Automations", meaning: "Aturan otomatis, misalnya mengubah status atau menambah assignee." },
        ]}
      />
      <DocsCallout variant="warning">
        Berhati-hatilah menghapus state, label, atau work item type yang sedang dipakai ticket — dampaknya berlaku ke
        data yang ada, bukan hanya ke ticket baru.
      </DocsCallout>
      <DocsParagraph>
        Efek dari pengaturan ini terlihat di <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>,{" "}
        <DocsTopicLink slug="intake">Intake</DocsTopicLink>, dan <DocsTopicLink slug="queues">Queues</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
