import { afterEach, describe, expect, it, vi } from "vitest";
import { getIsMobileViewport, getIsTouchPointer, subscribeToMediaQuery } from "./use-mobile-viewport";

const stubMatchMedia = (matches: (query: string) => boolean) => {
  vi.stubGlobal("window", {
    matchMedia: vi.fn((query: string) => ({
      matches: matches(query),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
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

describe("subscribeToMediaQuery", () => {
  it("registers and removes the change listener on the media query list", () => {
    const addEventListener = vi.fn();
    const removeEventListener = vi.fn();
    const mediaQueryList = { matches: false, addEventListener, removeEventListener };
    vi.stubGlobal("window", { matchMedia: vi.fn(() => mediaQueryList) });
    const listener = vi.fn();

    const unsubscribe = subscribeToMediaQuery("(max-width: 767px)", listener);

    expect(addEventListener).toHaveBeenCalledWith("change", listener);
    unsubscribe();
    expect(removeEventListener).toHaveBeenCalledWith("change", listener);
  });

  it("returns a no-op unsubscribe when matchMedia is unavailable", () => {
    vi.stubGlobal("window", {});
    const unsubscribe = subscribeToMediaQuery("(max-width: 767px)", vi.fn());
    expect(() => unsubscribe()).not.toThrow();
  });
});
