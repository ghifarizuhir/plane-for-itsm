/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function SprintsContent() {
  return (
    <>
      <DocsParagraph>
        Sprint adalah periode kerja berulang untuk merencanakan dan menyelesaikan pekerjaan dalam rentang waktu tetap —
        misalnya satu atau dua minggu.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat sprint dengan tanggal mulai dan target selesai, menambahkan ticket dari backlog, memantau progress dan
        burndown sprint, lalu menutup sprint — ticket yang belum selesai dipindahkan ke sprint berikutnya atau kembali
        ke backlog.
      </DocsParagraph>
      <DocsCallout variant="info">
        Sprint hanya tampil bila fitur ini diaktifkan admin project di pengaturan Features. Melihat pekerjaan lintas
        sprint? Gunakan <DocsTopicLink slug="queues">Queues</DocsTopicLink>.
      </DocsCallout>
    </>
  );
}
