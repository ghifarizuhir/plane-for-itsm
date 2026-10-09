/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { getBinaryDataFromDocumentEditorHTMLString } from "@plane/editor";
import { describe, expect, it } from "vitest";
import * as Y from "yjs";
import { replaceDocumentContent } from "@/lib/replace-document";

const makeDoc = (html: string, title: string) => {
  const doc = new Y.Doc();
  Y.applyUpdate(doc, getBinaryDataFromDocumentEditorHTMLString(html, title));
  return doc;
};

describe("replaceDocumentContent", () => {
  it("replaces body and title without leaving old content behind", () => {
    const doc = makeDoc("<p>Old body</p>", "Old title");
    replaceDocumentContent(doc, "<p>New body</p>", "New title");
    const body = doc.getXmlFragment("default").toString();
    const title = doc.getXmlFragment("title").toString();
    expect(body).toContain("New body");
    expect(body).not.toContain("Old body");
    expect(title).toContain("New title");
    expect(title).not.toContain("Old title");
  });

  it("keeps exactly one body child (no union duplicates)", () => {
    const doc = makeDoc("<p>Old body</p>", "Title");
    replaceDocumentContent(doc, "<p>New body</p>", "Title");
    expect(doc.getXmlFragment("default").length).toBe(1);
    expect(doc.getXmlFragment("title").length).toBe(1);
  });

  it("is stable in shape when applied twice", () => {
    const doc = makeDoc("<p>Old body</p>", "Old title");
    replaceDocumentContent(doc, "<p>New body</p>", "New title");
    replaceDocumentContent(doc, "<p>New body</p>", "New title");
    expect(doc.getXmlFragment("default").toString()).toContain("New body");
    expect(doc.getXmlFragment("default").length).toBe(1);
    expect(doc.getXmlFragment("title").length).toBe(1);
    expect(doc.getXmlFragment("title").toString()).toContain("New title");
  });

  it("handles empty html and empty title without throwing", () => {
    const doc = makeDoc("<p>Old body</p>", "Old title");
    expect(() => replaceDocumentContent(doc, "<p></p>", "")).not.toThrow();
    expect(doc.getXmlFragment("default").toString()).not.toContain("Old body");
  });
});
