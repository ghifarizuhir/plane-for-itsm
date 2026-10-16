/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading } from "../blocks";

export default function ReportsContent() {
  return (
    <>
      <DocsParagraph>
        Reports menampilkan analitik workspace: tren pekerjaan, sebaran status, dan gambaran beban tim — berguna saat
        rapat mingguan atau evaluasi layanan.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat metrik per project (jumlah ticket, distribusi status dan priority), memfilter berdasarkan rentang waktu,
        serta memantau tren penyelesaian pekerjaan dari waktu ke waktu.
      </DocsParagraph>
      <DocsCallout variant="info">Reports tersedia untuk admin dan member; guest tidak memilikinya.</DocsCallout>
    </>
  );
}
