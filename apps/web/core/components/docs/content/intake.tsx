/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsStepList, DocsTopicLink } from "../blocks";

export default function IntakeContent() {
  return (
    <>
      <DocsParagraph>
        Intake adalah pintu masuk permintaan: laporan dari luar (email, form, webhook) masuk ke antrean triage sebelum
        menjadi ticket.
      </DocsParagraph>
      <DocsSectionHeading>Alur triage</DocsSectionHeading>
      <DocsStepList
        steps={[
          { title: "Permintaan masuk", description: "Laporan dari intake source muncul di antrean intake." },
          {
            title: "Triage",
            description: "Periksa kelayakan: ini insiden, permintaan layanan, atau bukan urusan tim?",
          },
          {
            title: "Accept atau Decline",
            description: "Accept mengubahnya menjadi ticket; Decline menutupnya dengan alasan.",
          },
        ]}
      />
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat daftar permintaan masuk beserta sumbernya, menerima atau menolak dengan alasan, dan mengatur sumber
        intake di pengaturan project (lihat <DocsTopicLink slug="project-settings">Pengaturan project</DocsTopicLink>
        ).
      </DocsParagraph>
      <DocsCallout variant="info">
        Intake menjaga halaman Tickets tetap bersih: hanya permintaan yang tervalidasi yang menjadi{" "}
        <DocsTopicLink slug="tickets">Tickets</DocsTopicLink>.
      </DocsCallout>
    </>
  );
}
