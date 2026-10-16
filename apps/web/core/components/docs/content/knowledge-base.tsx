/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function KnowledgeBaseContent() {
  return (
    <>
      <DocsParagraph>
        Knowledge base adalah tempat dokumentasi tim hidup: artikel solusi, panduan operasional, catatan insiden besar,
        dan apa pun yang perlu dibaca ulang.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Menulis halaman dengan editor kaya (heading, gambar, tabel, code block), membuat sub-halaman berjenjang,
        berkolaborasi realtime dengan rekan tim, mempublikasikan halaman ke web publik lewat share link, dan mengekspor
        halaman ke PDF atau DOCX.
      </DocsParagraph>
      <DocsParagraph>
        Tautkan artikel solusi ke <DocsTopicLink slug="tickets">tickets</DocsTopicLink> terkait agar penanganan
        berikutnya lebih cepat. Artikel baru biasanya lahir dari ticket yang sudah selesai — pertimbangkan menuliskannya
        setiap kali menyelesaikan insiden yang menarik.
      </DocsParagraph>
    </>
  );
}
