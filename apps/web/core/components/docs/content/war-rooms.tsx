/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTopicLink } from "../blocks";

export default function WarRoomsContent() {
  return (
    <>
      <DocsParagraph>
        War room adalah ruang khusus untuk menangani insiden besar (major incident): semua koordinasi terjadi di satu
        tempat. War room dibuat dari sebuah ticket — ticket itu berubah menjadi ruang insiden.
      </DocsParagraph>
      <DocsSectionHeading>Alur memulai war room</DocsSectionHeading>
      <DocsStepList
        steps={[
          { title: "Buka ticket insiden", description: "Pilih ticket yang layak ditangani sebagai major incident." },
          {
            title: "Aktifkan toggle War room",
            description: "Ticket berubah menjadi war room dan otomatis masuk daftar insiden.",
          },
          {
            title: "Koordinasi di dalam ruang",
            description: "Chat realtime dengan peserta, ikuti runbook, dan pantau dampak.",
          },
        ]}
      />
      <DocsSectionHeading>Yang ada di dalam war room</DocsSectionHeading>
      <DocsParagraph>
        Blast radius (area dan service yang terdampak), chat realtime dengan peserta, runbook langkah penanganan, serta
        daftar semua war room aktif yang bisa dibuka lewat filter "war room" di halaman Tickets.
      </DocsParagraph>
      <DocsCallout variant="warning">
        War room memicu notifikasi ke peserta — gunakan untuk insiden yang benar-benar perlu penanganan terkoordinasi,
        bukan pekerjaan biasa. Cek dampak layanan di <DocsTopicLink slug="services">Services</DocsTopicLink> sebelum
        menetapkan blast radius.
      </DocsCallout>
    </>
  );
}
