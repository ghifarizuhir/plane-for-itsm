import { describe, expect, it } from "vitest";
import { toAiMessage } from "./ai-conversations";
import type { TAiStoredMessage } from "./ai-conversations";

const stored = (overrides: Partial<TAiStoredMessage>): TAiStoredMessage => ({
  id: "m1",
  role: "assistant",
  content: "plain",
  content_html: null,
  metadata: {},
  created_at: "2026-09-24T10:00:00Z",
  ...overrides,
});

describe("toAiMessage", () => {
  it("prefers response html for assistant messages", () => {
    const message = toAiMessage(stored({ content: "plain", content_html: "<p>rich</p>" }));
    expect(message.content).toBe("<p>rich</p>");
    expect(message.isError).toBe(false);
    expect(message.createdAt).toBe("2026-09-24T10:00:00Z");
  });

  it("falls back to plain content when html is missing", () => {
    expect(toAiMessage(stored({})).content).toBe("plain");
  });

  it("maps schedule metadata onto the message", () => {
    const message = toAiMessage(
      stored({
        metadata: {
          schedule_proposal: {
            name: "Laporan",
            prompt: "ringkas",
            frequency: "weekly",
            time: "09:00",
            timezone: "Asia/Jakarta",
          },
          schedule_proposal_key: "key-1",
          schedule_decision: "created",
          created_schedule_id: "sched-1",
          is_error: false,
        },
      })
    );
    expect(message.scheduleProposal?.name).toBe("Laporan");
    expect(message.scheduleProposalKey).toBe("key-1");
    expect(message.scheduleDecision).toBe("created");
    expect(message.createdScheduleId).toBe("sched-1");
  });

  it("maps is_error to the error flag", () => {
    const message = toAiMessage(stored({ metadata: { is_error: true } }));
    expect(message.isError).toBe(true);
  });

  it("maps work item proposals and decisions from metadata", () => {
    const message = toAiMessage(
      stored({
        metadata: {
          work_item_proposals: [{ key: "k1", proposal: { project: "LTS", name: "Fix pump" } }],
          work_item_decisions: {
            k1: { decision: "created", created_work_item_id: "i1", created_project_id: "p1" },
          },
        },
      })
    );
    expect(message.workItemProposals).toHaveLength(1);
    expect(message.workItemProposals?.[0].key).toBe("k1");
    expect(message.workItemProposals?.[0].proposal.name).toBe("Fix pump");
    expect(message.workItemDecisions?.k1.decision).toBe("created");
    expect(message.workItemDecisions?.k1.created_work_item_id).toBe("i1");
    expect(message.workItemDecisions?.k1.created_project_id).toBe("p1");
  });

  it("leaves work item fields undefined when metadata is absent", () => {
    const message = toAiMessage(stored({}));
    expect(message.workItemProposals).toBeUndefined();
    expect(message.workItemDecisions).toBeUndefined();
  });
});
