import { describe, expect, it } from "vitest";
import type { TAiConversation } from "./ai-conversations";
import { groupConversationsByDate } from "./galileo-history";

const conversation = (id: string, updatedAt: Date): TAiConversation => ({
  id,
  title: id,
  mode: "classic",
  created_at: updatedAt.toISOString(),
  updated_at: updatedAt.toISOString(),
});

const at = (day: number, hour = 9): Date => new Date(2026, 9, day, hour, 0, 0);

describe("groupConversationsByDate", () => {
  it("groups conversations into today, yesterday, previous 7 days, and older", () => {
    const groups = groupConversationsByDate(
      [
        conversation("today", at(27)),
        conversation("yesterday", at(26)),
        conversation("week", at(22)),
        conversation("older", at(1)),
      ],
      at(27, 12)
    );

    expect(groups.today.map((item) => item.id)).toEqual(["today"]);
    expect(groups.yesterday.map((item) => item.id)).toEqual(["yesterday"]);
    expect(groups.previous7Days.map((item) => item.id)).toEqual(["week"]);
    expect(groups.older.map((item) => item.id)).toEqual(["older"]);
  });

  it("sorts newest first inside a group", () => {
    const groups = groupConversationsByDate(
      [conversation("older-today", at(27, 8)), conversation("newer-today", at(27, 11))],
      at(27, 12)
    );
    expect(groups.today.map((item) => item.id)).toEqual(["newer-today", "older-today"]);
  });

  it("puts invalid timestamps in older", () => {
    const groups = groupConversationsByDate(
      [{ ...conversation("broken", at(27)), updated_at: "not-a-date" }],
      at(27, 12)
    );
    expect(groups.older.map((item) => item.id)).toEqual(["broken"]);
  });
});
