/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTopicLink } from "../blocks";

export default function InboxContent() {
  return (
    <>
      <DocsParagraph>
        Inbox mengumpulkan semua pemberitahuan penting di satu tempat: mention, assignment, komentar, perubahan status,
        dan aktivitas pada item yang kamu ikuti.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Membuka ticket langsung dari notifikasi, menandai notifikasi sebagai sudah dibaca (satu per satu atau semua),
        dan memfilter pemberitahuan. Preferensi jenis notifikasi diatur di{" "}
        <DocsTopicLink slug="personal-settings">Profil & preferensi</DocsTopicLink>.
      </DocsParagraph>
      <DocsCallout variant="info" title="Subscription">
        Kamu otomatis mendapat notifikasi untuk ticket yang kamu buat, yang di-assign kepadamu, atau yang kamu
        komentari. Kamu juga bisa follow/unfollow ticket secara manual dari detail ticket.
      </DocsCallout>
    </>
  );
}
