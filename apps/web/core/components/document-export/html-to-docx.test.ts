/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import { Paragraph, Table } from "docx";
import { htmlToDocxBlocks, type TImageFetcher } from "./html-to-docx";

const noopImageFetcher: TImageFetcher = async () => null;

describe("htmlToDocxBlocks", () => {
  it("maps headings and paragraphs", async () => {
    const blocks = await htmlToDocxBlocks("<h2>Hello</h2><p>Plain <strong>bold</strong></p>", {
      imageFetcher: noopImageFetcher,
    });
    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toBeInstanceOf(Paragraph);
  });

  it("maps tables with header cells", async () => {
    const blocks = await htmlToDocxBlocks("<table><tr><th>ID</th></tr><tr><td>1</td></tr></table>", {
      imageFetcher: noopImageFetcher,
    });
    expect(blocks[0]).toBeInstanceOf(Table);
  });

  it("unwraps unknown tags and drops script/style", async () => {
    const blocks = await htmlToDocxBlocks("<custom-tag>kept</custom-tag><script>alert(1)</script><style>.x{}</style>", {
      imageFetcher: noopImageFetcher,
    });
    const json = JSON.stringify(blocks);
    expect(json).toContain("kept");
    expect(json).not.toContain("alert");
    expect(json).not.toContain(".x");
  });

  it("renders mention spans as plain @label text", async () => {
    const blocks = await htmlToDocxBlocks('<p><span data-type="mention" data-label="Budi">@</span> hello</p>', {
      imageFetcher: noopImageFetcher,
    });
    expect(JSON.stringify(blocks)).toContain("@Budi");
  });

  it("falls back to a placeholder when image fetch fails", async () => {
    const blocks = await htmlToDocxBlocks('<img src="https://x/y.png" alt="diagram" />', {
      imageFetcher: async () => null,
    });
    expect(JSON.stringify(blocks)).toContain("diagram");
  });

  it("embeds images when the fetcher succeeds", async () => {
    const blocks = await htmlToDocxBlocks('<img src="https://x/y.png" />', {
      imageFetcher: async () => ({ data: new Uint8Array([1, 2, 3]), width: 800, height: 400, type: "png" }),
    });
    expect(JSON.stringify(blocks)).toContain("image");
  });

  it("produces list paragraphs", async () => {
    const blocks = await htmlToDocxBlocks("<ul><li>one</li><li>two</li></ul>", { imageFetcher: noopImageFetcher });
    expect(blocks.length).toBeGreaterThanOrEqual(2);
    expect(blocks.every((block) => block instanceof Paragraph)).toBe(true);
  });
});
