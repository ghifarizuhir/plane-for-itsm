/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { DocsCallout, DocsParagraph, DocsSectionHeading, DocsTermTable, DocsTopicLink } from "../blocks";

export default function ServicesContent() {
  return (
    <>
      <DocsParagraph>
        Services adalah katalog layanan IT: apa saja yang disediakan tim, siapa pemiliknya, dan bagaimana kondisi
        kesehatannya saat ini.
      </DocsParagraph>
      <DocsSectionHeading>Yang bisa kamu lakukan</DocsSectionHeading>
      <DocsParagraph>
        Melihat daftar service beserta status kesehatan dan pemiliknya, membuka detail service (deskripsi, dependencies,
        ticket terkait), serta menelusuri dependency graph untuk memahami hubungan antar layanan.
      </DocsParagraph>
      <DocsSectionHeading>Istilah di halaman Services</DocsSectionHeading>
      <DocsTermTable
        terms={[
          { term: "Health", meaning: "Status kesehatan service: sehat, terganggu, atau down." },
          {
            term: "Dependencies",
            meaning: "Layanan lain yang bergantung pada, atau menjadi tempat bergantung, service ini.",
          },
          { term: "Dependency graph", meaning: "Peta hubungan antar service untuk menilai dampak suatu gangguan." },
        ]}
      />
      <DocsCallout variant="tip">
        Saat insiden besar terjadi, cek dependency graph untuk memperkirakan siapa saja yang terdampak, lalu buka{" "}
        <DocsTopicLink slug="war-rooms">War Rooms</DocsTopicLink> untuk koordinasi.
      </DocsCallout>
    </>
  );
}
