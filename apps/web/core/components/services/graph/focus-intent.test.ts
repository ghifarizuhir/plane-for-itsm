/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
// local imports
import {
  createFocusIntentController,
  FOCUS_CLOSE_DELAY,
  FOCUS_OPEN_DELAY,
  type TServiceFocusState,
} from "./focus-intent";

const setup = () => {
  const states: TServiceFocusState[] = [];
  const controller = createFocusIntentController({
    onChange: (state) => states.push(state),
  });
  const latest = () => states[states.length - 1] ?? null;

  return { controller, latest, states };
};

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("focus intent controller", () => {
  it("opens only after the intent delay", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY - 1);
    expect(latest()).toBeNull();

    vi.advanceTimersByTime(1);
    expect(latest()).toEqual({
      hoveredServiceId: "service-a",
      pinnedServiceId: null,
      suppressed: false,
    });
  });

  it("cancels a pending open when the pointer leaves early", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY / 2);
    controller.leaveNode();
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY + FOCUS_CLOSE_DELAY);

    expect(latest()).toBeNull();
  });

  it("keeps the card open through the node-card gap and closes after the grace period", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    controller.leaveNode();

    vi.advanceTimersByTime(FOCUS_CLOSE_DELAY - 1);
    expect(latest()?.hoveredServiceId).toBe("service-a");

    controller.enterCard();
    vi.advanceTimersByTime(FOCUS_CLOSE_DELAY * 5);
    expect(latest()?.hoveredServiceId).toBe("service-a");

    controller.leaveCard();
    vi.advanceTimersByTime(FOCUS_CLOSE_DELAY);
    expect(latest()?.hoveredServiceId).toBeNull();
  });

  it("keeps a pinned card when hovering another node", () => {
    const { controller, latest } = setup();

    controller.pin("service-a");
    controller.hover("service-b");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY + FOCUS_CLOSE_DELAY);

    expect(latest()).toEqual({
      hoveredServiceId: null,
      pinnedServiceId: "service-a",
      suppressed: false,
    });
  });

  it("moves the pin when another node is pinned", () => {
    const { controller, latest } = setup();

    controller.pin("service-a");
    controller.pin("service-b");

    expect(latest()).toEqual({
      hoveredServiceId: null,
      pinnedServiceId: "service-b",
      suppressed: false,
    });
  });

  it("clears hover and pin timers on clear", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    controller.clear();
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY + FOCUS_CLOSE_DELAY);

    expect(latest()).toEqual({
      hoveredServiceId: null,
      pinnedServiceId: null,
      suppressed: false,
    });
  });

  it("suppresses hover until resumed", () => {
    const { controller, latest } = setup();

    controller.hover("service-a");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    controller.suppress();

    expect(latest()).toEqual({
      hoveredServiceId: null,
      pinnedServiceId: null,
      suppressed: true,
    });

    controller.hover("service-b");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    expect(latest()?.hoveredServiceId).toBeNull();

    controller.resume();
    controller.hover("service-b");
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);
    expect(latest()).toEqual({
      hoveredServiceId: "service-b",
      pinnedServiceId: null,
      suppressed: false,
    });
  });

  it("cancels pending timers on dispose", () => {
    const { controller, latest, states } = setup();

    controller.hover("service-a");
    controller.dispose();
    vi.advanceTimersByTime(FOCUS_OPEN_DELAY);

    expect(states).toHaveLength(0);
    expect(latest()).toBeNull();
  });
});
