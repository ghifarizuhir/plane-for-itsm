/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function HomeContent() {
  return (
    <>
      <DocsParagraph>
        Home adalah halaman pertama yang kamu lihat setelah login. Isinya ringkasan hal-hal yang paling sering kamu
        butuhkan: pekerjaan yang menunggumu dan aktivitas terbaru workspace.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat widget ringkasan — pekerjaan yang di-assign kepadamu, aktivitas terbaru rekan tim, dan statistik
        workspace. Mengubah tata letak widget sesuai kebutuhan, serta melihat siapa saja yang baru bergabung.
      </DocsParagraph>
      <DocsCallout variant="tip" title="Kebiasaan pagi yang baik">
        Buka Home, cek daftar assigned work, lalu lanjut ke <DocsTopicLink slug="inbox">Inbox</DocsTopicLink> untuk
        memastikan tidak ada mention atau komentar yang terlewat.
      </DocsCallout>
    </>
  );
}
