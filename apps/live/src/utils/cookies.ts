/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

/**
 * Merge `Set-Cookie` values from a refresh response into an existing
 * `Cookie` header value. Rotated names are replaced, all other pairs
 * (e.g. unrelated cookies) are preserved untouched.
 */
export function mergeRefreshedCookies(oldCookie: string, setCookies: string[]): string {
  const updated = new Map<string, string>();
  for (const header of setCookies) {
    const pair = header.split(";")[0] ?? "";
    const eq = pair.indexOf("=");
    if (eq === -1) continue;
    const name = pair.slice(0, eq).trim();
    const value = pair.slice(eq + 1).trim();
    if (name && value) updated.set(name, value);
  }
  if (updated.size === 0) return oldCookie;
  const kept = oldCookie
    .split(";")
    .map((p) => p.trim())
    .filter(Boolean)
    .filter((pair) => {
      const eq = pair.indexOf("=");
      const name = eq === -1 ? pair : pair.slice(0, eq).trim();
      return !updated.has(name);
    });
  for (const [name, value] of updated) kept.push(`${name}=${value}`);
  return kept.join("; ");
}
