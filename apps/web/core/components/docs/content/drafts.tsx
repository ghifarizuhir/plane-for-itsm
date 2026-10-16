/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function DraftsContent() {
  return (
    <>
      <DocsParagraph>
        Drafts menyimpan ticket yang belum siap dibuat — tulisan setengah jadi tetap aman dan hanya terlihat olehmu.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Menyimpan draft ticket (judul, deskripsi, properti) tanpa langsung membuatnya, melanjutkan draft kapan saja, dan
        mempublikasikannya menjadi ticket penuh saat sudah siap. Draft bisa dimulai dari tombol New ticket lalu pilih
        Save as draft.
      </DocsParagraph>
      <DocsParagraph>
        Setelah dipublikasikan, ticket mengikuti alur normal di <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
