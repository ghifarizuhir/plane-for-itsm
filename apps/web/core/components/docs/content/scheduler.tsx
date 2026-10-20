/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function SchedulerContent() {
  return (
    <>
      <DocsParagraph>
        Scheduler menjalankan tugas AI secara terjadwal: laporan rutin, rangkuman mingguan, atau pengingat otomatis —
        tanpa kamu harus memicunya manual.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat jadwal dengan prompt dan frekuensi (harian, mingguan, dan seterusnya), mengedit nama, resep, dan cadence
        jadwal yang sudah tersimpan, mengaktifkan atau menonaktifkannya, serta melihat riwayat eksekusi beserta
        hasilnya.
      </DocsParagraph>
      <DocsCallout variant="tip" title="Edit dari dua tempat">
        Buka detail jadwal lalu pilih <strong>Edit</strong>, atau minta Galileo mengubahnya. Perubahan dari Galileo
        selalu tampil sebagai kartu before/after yang harus kamu konfirmasi.
      </DocsCallout>
      <DocsCallout variant="tip">
        Scheduler memakai kemampuan yang sama dengan <DocsTopicLink slug="galileo">Galileo</DocsTopicLink>. Tulis prompt
        sejelas mungkin: sebutkan project dan jenis laporan yang kamu inginkan.
      </DocsCallout>
    </>
  );
}
