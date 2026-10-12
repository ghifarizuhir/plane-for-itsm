/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, expect, it } from "vitest";
import type { TIssueActivity } from "@plane/types";
import { milestoneRows, type TMilestoneLabels } from "./milestones";

const labels: TMilestoneLabels = {
  created: "Created",
  state: "State",
  priority: "Priority",
  assignees: "Assignee",
  target_date: "Target date",
  start_date: "Start date",
  archived_at: "Archived",
  noValue: "—",
};

const activity = (overrides: Partial<TIssueActivity>): TIssueActivity =>
  ({
    id: "a1",
    verb: "updated",
    field: "state",
    old_value: "state-1",
    new_value: "state-2",
    created_at: "2026-10-12T10:00:00Z",
    actor_detail: { display_name: "Budi" },
    ...overrides,
  }) as TIssueActivity;

const lookups = {
  stateName: (id: string) => ({ "state-1": "Todo", "state-2": "In Progress" })[id],
  memberName: (id: string) => ({ "user-1": "Budi", "user-2": "Siti" })[id],
};

describe("milestoneRows", () => {
  it("includes created events", () => {
    const rows = milestoneRows([activity({ verb: "created", field: undefined })], lookups, labels);
    expect(rows).toHaveLength(1);
    expect(rows[0].event).toBe("Created");
  });

  it("resolves state changes to names", () => {
    const rows = milestoneRows([activity({})], lookups, labels);
    expect(rows[0]).toMatchObject({ event: "State", from: "Todo", to: "In Progress", actor: "Budi" });
  });

  it("computes added and removed assignees", () => {
    const rows = milestoneRows(
      [activity({ field: "assignees", old_value: "user-1", new_value: "user-1,user-2" })],
      lookups,
      labels
    );
    expect(rows[0]).toMatchObject({ event: "Assignee", from: "—", to: "Siti" });
  });

  it("excludes non-milestone fields", () => {
    const rows = milestoneRows(
      [activity({ field: "labels" }), activity({ field: "description" }), activity({ field: "comment" })],
      lookups,
      labels
    );
    expect(rows).toHaveLength(0);
  });

  it("sorts ascending by time", () => {
    const rows = milestoneRows(
      [
        activity({ id: "b", created_at: "2026-10-12T12:00:00Z" }),
        activity({ id: "a", created_at: "2026-10-12T08:00:00Z" }),
      ],
      lookups,
      labels
    );
    expect(rows[0].createdAt).toBe("2026-10-12T08:00:00Z");
  });
});
