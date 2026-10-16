/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function WorkspaceViewsContent() {
  return (
    <>
      <DocsParagraph>
        Workspace Views adalah kumpulan ticket lintas project yang tersimpan sebagai tampilan. Empat view bawaan yang
        tersedia: All issues (semua ticket yang bisa kamu akses), Assigned (milikmu), Created (kamu buat), dan
        Subscribed (kamu ikuti).
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuka view bawaan, memfilter dan mencari ticket lintas project, serta membuka ticket tetap dalam konteks
        workspace. Untuk daftar tersaring di dalam satu project, gunakan{" "}
        <DocsTopicLink slug="queues">Queues</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
