import { afterEach, describe, expect, it, vi } from "vitest";
import { getIsMobileViewport, getIsTouchPointer } from "./use-mobile-viewport";

const stubMatchMedia = (matches: (query: string) => boolean) => {
  vi.stubGlobal("window", {
    matchMedia: vi.fn((query: string) => ({ matches: matches(query) })),
  });
};

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("getIsMobileViewport", () => {
  it("returns false when window is unavailable", () => {
    expect(getIsMobileViewport()).toBe(false);
  });

  it("returns true when the mobile media query matches", () => {
    stubMatchMedia((query) => query === "(max-width: 767px)");
    expect(getIsMobileViewport()).toBe(true);
  });

  it("returns false when the mobile media query does not match", () => {
    stubMatchMedia(() => false);
    expect(getIsMobileViewport()).toBe(false);
  });
});

describe("getIsTouchPointer", () => {
  it("returns true when the coarse pointer query matches", () => {
    stubMatchMedia((query) => query === "(pointer: coarse)");
    expect(getIsTouchPointer()).toBe(true);
  });

  it("returns false when matchMedia is missing", () => {
    vi.stubGlobal("window", {});
    expect(getIsTouchPointer()).toBe(false);
  });
});
