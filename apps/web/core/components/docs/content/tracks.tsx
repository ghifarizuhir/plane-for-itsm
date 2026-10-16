/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading } from "../blocks";

export default function TracksContent() {
  return (
    <>
      <DocsParagraph>
        Track mengelompokkan kumpulan ticket yang menuju satu sasaran atau fase — misalnya satu perbaikan besar, atau
        satu tahap migrasi sistem.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuat track dengan lead dan target date, mengelompokkan ticket ke dalamnya, dan memantau progress dari
        ringkasan ticket yang sudah selesai.
      </DocsParagraph>
      <DocsCallout variant="tip">
        Sprint menjawab "kapan", track menjawab "bagian mana dari sasaran besar". Pekerjaan bisa berada di keduanya
        sekaligus.
      </DocsCallout>
    </>
  );
}
