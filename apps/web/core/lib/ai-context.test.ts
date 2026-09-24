import { describe, expect, it } from "vitest";
import { AI_ASSISTANT_TASK, buildAiContext, sanitizeAssistantHtml, stripHtml } from "./ai-context";
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

describe("buildAiContext", () => {
  const context: TAiIssueContext = {
    name: "Login fails with SSO",
    descriptionHtml: "<p>User cannot log in via SSO.</p>",
    state: "In Progress",
    priority: "high",
  };

  it("includes issue context and timezone but not history", () => {
    const result = buildAiContext(context, "Asia/Jakarta");
    expect(result).toContain("Work item context:");
    expect(result).toContain("Work item: Login fails with SSO");
    expect(result).toContain("State: In Progress");
    expect(result).toContain("User timezone: Asia/Jakarta");
    expect(result).not.toContain("Conversation so far:");
  });

  it("omits context block fields that are missing", () => {
    const result = buildAiContext({ name: "X", descriptionHtml: "" });
    expect(result).not.toContain("Description:");
    expect(result).not.toContain("State:");
    expect(result).not.toContain("User timezone:");
  });

  it("falls back to general knowledge without an active issue", () => {
    expect(buildAiContext(undefined)).toBe("No active work item context. Answer from general knowledge.");
  });
});
