import { describe, expect, it } from "vitest";
import {
  buildGalileoConversationHref,
  buildGalileoHref,
  canUseGalileo,
  shouldNavigateToNewConversation,
} from "./galileo-routing";

describe("galileo routing helpers", () => {
  it("builds Galileo URLs", () => {
    expect(buildGalileoHref("acme")).toBe("/acme/galileo");
    expect(buildGalileoConversationHref("acme", "conv-1")).toBe("/acme/galileo/conv-1");
  });

  it("navigates only when the route is new and a conversation now exists", () => {
    expect(shouldNavigateToNewConversation(undefined, "conv-1")).toBe(true);
    expect(shouldNavigateToNewConversation("conv-1", "conv-1")).toBe(false);
    expect(shouldNavigateToNewConversation(undefined, undefined)).toBe(false);
  });

  it("requires both LLM configuration and AI permission", () => {
    expect(canUseGalileo(true, true)).toBe(true);
    expect(canUseGalileo(false, true)).toBe(false);
    expect(canUseGalileo(true, false)).toBe(false);
  });
});
