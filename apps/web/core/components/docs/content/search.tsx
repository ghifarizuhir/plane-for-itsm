/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function SearchContent() {
  return (
    <>
      <DocsParagraph>
        Tekan Cmd/Ctrl + K kapan saja untuk membuka command palette: satu pintu untuk mencari dan menjalankan aksi tanpa
        mouse.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Mencari ticket, project, artikel knowledge base, dan member; menjalankan aksi cepat seperti membuat ticket baru,
        membuka halaman, atau berpindah project; serta melihat riwayat pencarian terakhirmu.
      </DocsParagraph>
      <DocsCallout variant="tip">
        Hafalkan Cmd/Ctrl + K — ini jalan tercepat ke mana pun. Untuk mencari solusi insiden yang pernah terjadi, coba
        kata kunci masalahnya lalu buka hasil dari <DocsTopicLink slug="knowledge-base">Knowledge base</DocsTopicLink>.
      </DocsCallout>
    </>
  );
}
