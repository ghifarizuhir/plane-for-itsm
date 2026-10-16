/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function QueuesContent() {
  return (
    <>
      <DocsParagraph>
        Queue adalah antrean kerja tersaring: kumpulan ticket dengan filter yang kamu simpan sebagai tampilan agar bisa
        dibuka berulang kali olehmu dan tim.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat view dengan filter (status, assignee, priority, label, tanggal, dan lainnya), berganti tampilan antara
        list, kanban, calendar, dan spreadsheet, serta menyimpan dan membagikan view ke anggota project.
      </DocsParagraph>
      <DocsCallout variant="info">
        Butuh daftar serupa tetapi lintas project? Gunakan{" "}
        <DocsTopicLink slug="workspace-views">Workspace Views</DocsTopicLink>. Queue hanya menampilkan ticket dari
        project tempat ia dibuat.
      </DocsCallout>
    </>
  );
}
