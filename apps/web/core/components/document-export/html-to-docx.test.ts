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

const okFetcher =
  (width: number, height: number): TImageFetcher =>
  async () => ({
    data: new Uint8Array([1]),
    width,
    height,
    type: "png",
  });

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

  it("embeds image-component blocks and passes the raw src to the fetcher", async () => {
    const requested: string[] = [];
    const blocks = await htmlToDocxBlocks(
      '<image-component src="asset-1" width="299px" height="535px"></image-component>',
      {
        imageFetcher: async (src) => {
          requested.push(src);
          return { data: new Uint8Array([1, 2, 3]), width: 800, height: 400, type: "png" };
        },
      }
    );
    expect(requested).toEqual(["asset-1"]);
    expect(JSON.stringify(blocks)).toContain("image");
  });

  it("scales image-component to the stored px width using the bitmap aspect ratio", async () => {
    const json = JSON.stringify(
      await htmlToDocxBlocks('<image-component src="a" width="299px" height="535px"></image-component>', {
        imageFetcher: okFetcher(800, 400),
      })
    );
    expect(json).toContain(String(299 * 9525));
    expect(json).toContain(String(150 * 9525));
  });

  it("caps the stored px width at the max image width", async () => {
    const json = JSON.stringify(
      await htmlToDocxBlocks('<image-component src="a" width="900px"></image-component>', {
        imageFetcher: okFetcher(1000, 500),
      })
    );
    expect(json).toContain(String(600 * 9525));
    expect(json).toContain(String(300 * 9525));
  });

  it("falls back to natural size for percent widths", async () => {
    const json = JSON.stringify(
      await htmlToDocxBlocks('<image-component src="a" width="35%" height="auto"></image-component>', {
        imageFetcher: okFetcher(800, 400),
      })
    );
    expect(json).toContain(String(600 * 9525));
    expect(json).toContain(String(300 * 9525));
  });

  it("embeds image-component inside paragraphs and list items", async () => {
    const fetcher = okFetcher(100, 100);
    const paragraphBlocks = await htmlToDocxBlocks('<p>before <image-component src="a"></image-component> after</p>', {
      imageFetcher: fetcher,
    });
    const paragraphListBlocks = await htmlToDocxBlocks(
      '<ul><li>item <image-component src="b"></image-component></li></ul>',
      {
        imageFetcher: fetcher,
      }
    );
    expect(paragraphBlocks).toHaveLength(2);
    expect(paragraphListBlocks.length).toBeGreaterThanOrEqual(2);
  });

  it("uses alt text for failed image-component fetches", async () => {
    const json = JSON.stringify(
      await htmlToDocxBlocks('<image-component src="a" alt="diagram"></image-component>', {
        imageFetcher: async () => null,
      })
    );
    expect(json).toContain("diagram");
  });

  it("uses the configured unavailable text when alt is missing", async () => {
    const json = JSON.stringify(
      await htmlToDocxBlocks('<image-component src="a"></image-component>', {
        imageFetcher: async () => null,
        imageUnavailableText: "Bild nicht verfügbar",
      })
    );
    expect(json).toContain("Bild nicht verfügbar");
  });

  it("does not leak the asset id as inline text in a mixed paragraph", async () => {
    const json = JSON.stringify(
      await htmlToDocxBlocks('<p>before <image-component src="asset-secret"></image-component> after</p>', {
        imageFetcher: async () => null,
      })
    );
    expect(json).toContain("before");
    expect(json).toContain("after");
    expect(json).not.toContain("asset-secret");
  });
});
