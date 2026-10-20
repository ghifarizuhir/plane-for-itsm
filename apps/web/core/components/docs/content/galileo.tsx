/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function GalileoContent() {
  return (
    <>
      <DocsParagraph>
        Galileo adalah asisten AI di sisi kanan layar. Ia bisa menjawab pertanyaan dengan konteks pekerjaan yang sedang
        kamu lihat — ticket, service, sprint, track, atau artikel knowledge base.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Mengobrol tentang item yang sedang terbuka (minta ringkasan, draf balasan, atau analisis singkat), membuat dan
        mengubah jadwal berulang, memulai percakapan baru, dan membuka kembali riwayat percakapan sebelumnya.
      </DocsParagraph>
      <DocsCallout variant="info" title="Konteks itu penting">
        Buka ticket atau service dulu, lalu bertanya — jawaban AI mengacu ke data yang sedang kamu lihat. Untuk tugas
        berulang, rangkai dengan <DocsTopicLink slug="scheduler">Scheduler</DocsTopicLink>.
      </DocsCallout>
      <DocsParagraph>
        Contoh: “Ubah laporan harian saya menjadi setiap Selasa pukul 10.00”. Galileo akan mencari jadwal yang cocok,
        menampilkan perbandingan sebelum/sesudah, dan baru menerapkannya setelah kamu menekan Apply.
      </DocsParagraph>
    </>
  );
}
