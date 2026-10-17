/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTermTable, DocsTopicLink } from "../blocks";

export default function Itil4ServiceRequestContent() {
  return (
    <>
      <DocsParagraph>
        Service request adalah permintaan standar dari pengguna — akses, perangkat, informasi, atau layanan lain yang
        alurnya sudah diketahui. Praktik ini membuat permintaan seperti itu konsisten, cepat, dan tercatat.
      </DocsParagraph>

      <DocsSectionHeading>Siapkan jenis ticket Request</DocsSectionHeading>
      <DocsParagraph>
        Jenis ticket bawaan tidak memuat "Request" — buat sekali di pengaturan workspace, lalu aktifkan di project yang
        membutuhkannya.
      </DocsParagraph>
      <DocsStepList
        steps={[
          {
            title: "Buat jenis ticket",
            description:
              "Buka pengaturan workspace → Work item types, tambah jenis baru bernama “Request”, dan aktifkan switch “requires service” agar setiap permintaan wajib menunjuk layanan.",
          },
          {
            title: "Aktifkan di project",
            description: "Buka pengaturan project → Work item types, lalu enable jenis “Request” yang baru dibuat.",
          },
          {
            title: "Sambungkan pintu masuk",
            description:
              "Aktifkan Intake di project dan tambahkan channel: form untuk pengguna, email, atau webhook/intake source.",
          },
        ]}
      />

      <DocsSectionHeading>Alur permintaan</DocsSectionHeading>
      <DocsStepList
        steps={[
          {
            title: "Permintaan masuk",
            description: "Permintaan datang lewat Intake dan menunggu di antrean triage.",
          },
          {
            title: "Triase dan accept",
            description:
              "Pilih jenis Request dan tautkan Service yang relevan — accept memindahkan ticket ke state awal project.",
          },
          {
            title: "Fulfillment",
            description:
              "Kerjakan dari queue per layanan. Untuk permintaan berulang (onboarding, pengadaan), simpan checklist sebagai sub-items.",
          },
          {
            title: "Selesai dan dokumentasikan",
            description:
              "Tandai ticket selesai setelah pengguna terlayani; jika pertanyaannya berulang, tulis artikel di Knowledge base.",
          },
        ]}
      />

      <DocsSectionHeading>Peran & ritme</DocsSectionHeading>
      <DocsTermTable
        terms={[
          {
            term: "Helpdesk L1",
            meaning: "Menjaga pintu masuk, memverifikasi permintaan, dan menuntaskan permintaan standar.",
          },
          { term: "Service owner", meaning: "Pemilik layanan yang menerima eskalasi permintaan khusus." },
          {
            term: "Ritme",
            meaning: "Permintaan terjadwal (misalnya onboarding batch) direncanakan dalam Sprint.",
          },
        ]}
      />

      <DocsCallout variant="warning" title="Jangan aktifkan requires service sembarangan">
        Switch "requires service" memaksa setiap ticket jenis itu menunjuk layanan. Pasang hanya untuk jenis yang memang
        terikat layanan, bukan untuk tugas internal biasa.
      </DocsCallout>

      <DocsParagraph>
        Terkait: <DocsTopicLink slug="intake">Intake</DocsTopicLink>,{" "}
        <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>, <DocsTopicLink slug="services">Services</DocsTopicLink>,{" "}
        <DocsTopicLink slug="queues">Queues</DocsTopicLink>, <DocsTopicLink slug="sprints">Sprints</DocsTopicLink>, dan{" "}
        <DocsTopicLink slug="knowledge-base">Knowledge base</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
