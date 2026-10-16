/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsParagraph, DocsSectionHeading, DocsTermTable, DocsTopicLink } from "../blocks";

export default function PersonalSettingsContent() {
  return (
    <>
      <DocsParagraph>
        Pengaturan pribadi dibuka dari menu profil di pojok kiri bawah (klik namamu lalu Settings). Semua yang ada di
        sini hanya memengaruhi akunmu, bukan tim.
      </DocsParagraph>
      <DocsSectionHeading>Tab yang tersedia</DocsSectionHeading>
      <DocsTermTable
        terms={[
          { term: "General", meaning: "Nama, foto profil, dan email." },
          { term: "Preferences", meaning: "Tema (terang/gelap), bahasa, dan preferensi tampilan." },
          { term: "Notifications", meaning: "Jenis pemberitahuan email dan in-app yang ingin kamu terima." },
          { term: "Security", meaning: "Ubah password akunmu." },
          { term: "API Tokens", meaning: "Token untuk integrasi pribadi dengan API." },
        ]}
      />
      <DocsParagraph>
        Jenis notifikasi yang kamu atur di sini menentukan isi <DocsTopicLink slug="inbox">Inbox</DocsTopicLink>.
      </DocsParagraph>
    </>
  );
}
