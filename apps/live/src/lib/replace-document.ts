/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { getBinaryDataFromDocumentEditorHTMLString } from "@plane/editor";
import * as Y from "yjs";

/**
 * Replace the full content of a live collaborative document (body + title)
 * with new HTML. Deletes every existing child first, then applies the new
 * content as CRDT insert operations, so connected clients receive a clean
 * delete+insert instead of a union of old and new items.
 */
export const replaceDocumentContent = (doc: Y.Doc, descriptionHtml: string, title: string): void => {
  doc.transact(() => {
    const defaultFragment = doc.getXmlFragment("default");
    const titleFragment = doc.getXmlFragment("title");

    defaultFragment.delete(0, defaultFragment.length);
    titleFragment.delete(0, titleFragment.length);

    const source = new Y.Doc();
    Y.applyUpdate(source, getBinaryDataFromDocumentEditorHTMLString(descriptionHtml, title));
    Y.applyUpdate(doc, Y.encodeStateAsUpdate(source));
  });
};
