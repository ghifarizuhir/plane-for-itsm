/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function YourWorkContent() {
  return (
    <>
      <DocsParagraph>
        Your work adalah halaman pribadimu: semua ticket lintas project dalam satu tampilan, tanpa perlu membuka project
        satu per satu.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat ticket yang di-assign kepadamu, yang kamu buat, dan yang kamu ikuti (subscribed). Di tab activity, kamu
        bisa melihat riwayat aktivitasmu yang terbaru.
      </DocsParagraph>
      <DocsParagraph>
        Butuh tampilan serupa untuk seluruh workspace? Gunakan{" "}
        <DocsTopicLink slug="workspace-views">Workspace Views</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
