import { describe, expect, it } from "vitest";
import { EPageAccess } from "@plane/types";
import {
  AI_ASSISTANT_TASK,
  buildAiContext,
  CONTEXT_DESCRIPTION_LIMIT,
  sanitizeAssistantHtml,
  stripHtml,
} from "./ai-context";
import type { TAiAssistantContext } from "./ai-context";

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

  it("is entity-neutral", () => {
    expect(AI_ASSISTANT_TASK).toContain("ITSM assistant");
    expect(AI_ASSISTANT_TASK).not.toContain("work-item");
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
  const context: TAiAssistantContext = {
    kind: "work_item",
    name: "Login fails with SSO",
    descriptionHtml: "<p>User cannot log in via SSO.</p>",
    state: "In Progress",
    priority: "high",
  };

  it("includes work item context and timezone but not history", () => {
    const result = buildAiContext(context, "Asia/Jakarta");
    expect(result).toContain("Work item context:");
    expect(result).toContain("Work item: Login fails with SSO");
    expect(result).toContain("State: In Progress");
    expect(result).toContain("User timezone: Asia/Jakarta");
    expect(result).not.toContain("Conversation so far:");
  });

  it("omits work item fields that are missing", () => {
    const result = buildAiContext({ kind: "work_item", name: "X", descriptionHtml: "" });
    expect(result).not.toContain("Description:");
    expect(result).not.toContain("State:");
    expect(result).not.toContain("User timezone:");
  });

  it("builds a service context block with id and status", () => {
    const result = buildAiContext({
      kind: "service",
      id: "svc-1",
      name: "Payments API",
      descriptionHtml: "<p>Handles card payments.</p>",
      status: "active",
    });
    expect(result).toContain("Service context:");
    expect(result).toContain("Service: Payments API");
    expect(result).toContain("Service id: svc-1");
    expect(result).toContain("Status: active");
    expect(result).toContain("Description: Handles card payments.");
  });

  it("builds an article context block with id and access", () => {
    const publicArticle = buildAiContext({
      kind: "article",
      id: "art-1",
      name: "On-call runbook",
      descriptionHtml: "<p>Steps.</p>",
      access: EPageAccess.PUBLIC,
    });
    expect(publicArticle).toContain("Article context:");
    expect(publicArticle).toContain("Article: On-call runbook");
    expect(publicArticle).toContain("Article id: art-1");
    expect(publicArticle).toContain("Access: Public");
    expect(publicArticle).toContain("Description: Steps.");

    const privateArticle = buildAiContext({
      kind: "article",
      id: "art-2",
      name: "Secrets",
      descriptionHtml: "",
      access: EPageAccess.PRIVATE,
    });
    expect(privateArticle).toContain("Access: Private");
  });

  it("truncates long descriptions for every kind", () => {
    const longDescription = "x".repeat(CONTEXT_DESCRIPTION_LIMIT + 500);
    const service = buildAiContext({
      kind: "service",
      id: "svc-1",
      name: "Payments API",
      descriptionHtml: `<p>${longDescription}</p>`,
    });
    expect(service).toContain(`${"x".repeat(CONTEXT_DESCRIPTION_LIMIT)}…`);
    expect(service).not.toContain("x".repeat(CONTEXT_DESCRIPTION_LIMIT + 1));

    const workItem = buildAiContext({
      kind: "work_item",
      name: "X",
      descriptionHtml: `<p>${longDescription}</p>`,
    });
    expect(workItem).toContain(`${"x".repeat(CONTEXT_DESCRIPTION_LIMIT)}…`);
  });

  it("falls back to general knowledge without an active context", () => {
    expect(buildAiContext(undefined)).toBe("No active context. Answer from general knowledge.");
  });
});
