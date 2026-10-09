/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

const REV_KEY_PREFIX = "page-sync-rev:";

/**
 * Deletes the IndexedDB cache of a page document. Best-effort: resolves on
 * success, error, or blocked (another tab still holds the database open).
 */
export const clearPageCache = async (pageId: string): Promise<boolean> => {
  if (typeof indexedDB === "undefined") return false;
  return await new Promise<boolean>((resolve) => {
    try {
      const request = indexedDB.deleteDatabase(pageId);
      request.onsuccess = () => resolve(true);
      // oxlint-disable-next-line unicorn/prefer-add-event-listener
      request.onerror = request.onblocked = () => resolve(false);
    } catch {
      resolve(false);
    }
  });
};

/**
 * Ensures the local collaborative cache is not stale relative to the server
 * page revision. When the revision differs (page changed elsewhere, e.g. an
 * AI edit), the cache is deleted so the next provider sync loads the server
 * document fresh instead of union-merging stale content.
 */
export const ensureFreshPageCache = async (
  pageId: string | undefined,
  revision: string | Date | null | undefined
): Promise<void> => {
  if (!pageId || !revision) return;
  try {
    const revisionString = revision instanceof Date ? revision.toISOString() : revision;
    const key = `${REV_KEY_PREFIX}${pageId}`;
    if (localStorage.getItem(key) === revisionString) return;
    const purged = await clearPageCache(pageId);
    if (purged) localStorage.setItem(key, revisionString);
  } catch {
    // Best-effort: a failed cache purge must never block the editor.
  }
};
