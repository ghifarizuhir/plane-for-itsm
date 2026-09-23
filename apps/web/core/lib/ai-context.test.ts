import { describe, expect, it } from "vitest";
import { AI_ASSISTANT_TASK, buildAiPrompt, sanitizeAssistantHtml, stripHtml } from "./ai-context";
import type { TAiIssueContext } from "./ai-context";

describe("stripHtml", () => {
  it("removes tags and collapses whitespace", () => {
    expect(stripHtml("<p>Some <b>bold</b> text</p>")).toBe("Some bold text");
  });

  it("decodes common entities", () => {
    expect(stripHtml("<p>A &amp; B</p>")).toBe("A & B");
    expect(stripHtml("<p>5 &lt; 6 &gt; 4</p>")).toBe("5 < 6 > 4");
  });

  it("returns empty string for empty html", () => {
    expect(stripHtml("")).toBe("");
  });
});

describe("AI_ASSISTANT_TASK", () => {
  it("is a non-empty instruction", () => {
    expect(AI_ASSISTANT_TASK.length).toBeGreaterThan(20);
  });
});

describe("sanitizeAssistantHtml", () => {
  it("keeps formatting tags", () => {
    expect(sanitizeAssistantHtml("<b>bold</b><br/>line")).toBe("<b>bold</b><br />line");
    expect(sanitizeAssistantHtml("<ul><li>one</li></ul>")).toBe("<ul><li>one</li></ul>");
  });

  it("strips scripts, event handlers, and unsafe URLs", () => {
    expect(sanitizeAssistantHtml("<script>alert(1)</script>")).toBe("");
    expect(sanitizeAssistantHtml('<img src=x onerror="alert(1)">')).toBe("");
    expect(sanitizeAssistantHtml('<a href="javascript:alert(1)">x</a>')).toBe(
      '<a target="_blank" rel="noopener noreferrer">x</a>'
    );
  });

  it("forces safe link attributes and preserves tables", () => {
    expect(sanitizeAssistantHtml('<a href="https://example.com" target="_self" rel="opener">x</a>')).toBe(
      '<a href="https://example.com" target="_blank" rel="noopener noreferrer">x</a>'
    );
    expect(sanitizeAssistantHtml('<a href="//evil.example">x</a>')).toBe(
      '<a target="_blank" rel="noopener noreferrer">x</a>'
    );
    expect(sanitizeAssistantHtml('<p onclick="x()" style="color:red">hi</p>')).toBe("<p>hi</p>");
    expect(sanitizeAssistantHtml("<table><tr><td>a</td><td>b</td></tr></table>")).toBe(
      "<table><tr><td>a</td><td>b</td></tr></table>"
    );
  });
});

describe("buildAiPrompt", () => {
  const context: TAiIssueContext = {
    name: "Login fails with SSO",
    descriptionHtml: "<p>User cannot log in via SSO.</p>",
    state: "In Progress",
    priority: "high",
  };

  it("includes issue context, history, and the question", () => {
    const history = [
      { id: "1", role: "user" as const, content: "What causes this?" },
      { id: "2", role: "assistant" as const, content: "Probably token refresh." },
    ];
    const result = buildAiPrompt(context, history, "Summarize the root cause");
    expect(result).toContain("Work item context:");
    expect(result).toContain("Work item: Login fails with SSO");
    expect(result).toContain("Description: User cannot log in via SSO.");
    expect(result).toContain("State: In Progress");
    expect(result).toContain("Priority: high");
    expect(result).toContain("User: What causes this?");
    expect(result).toContain("Assistant: Probably token refresh.");
    expect(result).toContain("User's new question: Summarize the root cause");
  });

  it("omits context block fields that are missing", () => {
    const result = buildAiPrompt({ name: "X", descriptionHtml: "" }, [], "hello");
    expect(result).not.toContain("Description:");
    expect(result).not.toContain("State:");
    expect(result).not.toContain("Priority:");
    expect(result).toContain("(empty)");
  });

  it("uses no-context block when context is undefined", () => {
    const result = buildAiPrompt(undefined, [], "hello");
    expect(result).toContain("No active work item context");
  });

  it("truncates history to the last 8 messages", () => {
    const history = Array.from({ length: 12 }, (_, i) => ({
      id: String(i),
      role: "user" as const,
      content: `msg-${i}`,
    }));
    const result = buildAiPrompt(undefined, history, "final question");
    expect(result).not.toContain("msg-0\n");
    expect(result).not.toContain("User: msg-3");
    expect(result).toContain("User: msg-4");
    expect(result).toContain("User: msg-11");
  });
});
