/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import { appendRichFilterNode } from "./issue-filter-scope";

describe("appendRichFilterNode", () => {
  it("creates the node when there is no existing expression", () => {
    expect(appendRichFilterNode(undefined, { module_id: ["m1"] })).toBe('{"module_id":["m1"]}');
    expect(appendRichFilterNode("{}", { module_id: ["m1"] })).toBe('{"module_id":["m1"]}');
  });

  it("wraps existing expressions in an AND", () => {
    expect(appendRichFilterNode('{"priority":"high"}', { module_id: ["m1"] })).toBe(
      '{"and":[{"priority":"high"},{"module_id":["m1"]}]}'
    );
  });

  it("replaces malformed or empty expressions", () => {
    expect(appendRichFilterNode("not-json", { module_id: ["m1"] })).toBe('{"module_id":["m1"]}');
    expect(appendRichFilterNode("[]", { module_id: ["m1"] })).toBe('{"module_id":["m1"]}');
  });

  it("scopes an expression to one application without dropping existing filters", () => {
    expect(appendRichFilterNode('{"priority":"high"}', { project_id: ["project-1"] })).toBe(
      '{"and":[{"priority":"high"},{"project_id":["project-1"]}]}'
    );
  });
});
