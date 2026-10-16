/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading } from "../blocks";

export default function StickiesContent() {
  return (
    <>
      <DocsParagraph>
        Stickies adalah catatan tempel digital untuk hal-hal cepat yang tidak perlu menjadi ticket: daftar periksa
        manual, nomor kontak darurat, atau pengingat singkat.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Menulis catatan dengan warna pilihan, mengganti warna atau menghapusnya, memilih tampilan board atau list, serta
        mengarsipkan catatan lama.
      </DocsParagraph>
      <DocsCallout variant="info">
        Stickies bersifat pribadi — rekan satu workspace tidak melihat catatanmu.
      </DocsCallout>
    </>
  );
}
