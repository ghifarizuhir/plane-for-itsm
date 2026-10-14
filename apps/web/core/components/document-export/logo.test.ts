/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// @vitest-environment happy-dom
import { describe, expect, it } from "vitest";
import type { TDocxImage } from "./html-to-docx";
import { resolveDocumentLogo } from "./logo";

const image: TDocxImage = { data: new Uint8Array([1]), width: 10, height: 10, type: "png" };

describe("resolveDocumentLogo", () => {
  it("uses the workspace logo url in workspace mode", async () => {
    const requested: string[] = [];
    const logo = await resolveDocumentLogo(
      { header_logo_mode: "workspace", header_logo_asset_id: null, header_logo_url: null },
      { logo_url: "/apin/v2/static/ws-logo/" },
      async (src) => {
        requested.push(src);
        return image;
      }
    );
    expect(requested).toEqual(["/apin/v2/static/ws-logo/"]);
    expect(logo).toEqual(image);
  });

  it("prefers the custom asset id over the static url", async () => {
    const requested: string[] = [];
    await resolveDocumentLogo(
      {
        header_logo_mode: "custom",
        header_logo_asset_id: "asset-9",
        header_logo_url: "/api/assets/v2/static/asset-9/",
      },
      null,
      async (src) => {
        requested.push(src);
        return image;
      }
    );
    expect(requested).toEqual(["asset-9"]);
  });

  it("returns null when mode is none", async () => {
    const logo = await resolveDocumentLogo(
      { header_logo_mode: "none", header_logo_asset_id: null, header_logo_url: null },
      null,
      async () => image
    );
    expect(logo).toBeNull();
  });
});
