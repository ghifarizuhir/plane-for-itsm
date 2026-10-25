/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const FOCUS_OPEN_DELAY = 180;
export const FOCUS_CLOSE_DELAY = 140;

export type TServiceFocusState = {
  hoveredServiceId: string | null;
  pinnedServiceId: string | null;
  suppressed: boolean;
};

type TFocusIntentControllerOptions = {
  openDelay?: number;
  closeDelay?: number;
  onChange: (state: TServiceFocusState) => void;
};

export type TFocusIntentController = {
  hover: (serviceId: string) => void;
  leaveNode: () => void;
  enterCard: () => void;
  leaveCard: () => void;
  pin: (serviceId: string) => void;
  clear: () => void;
  suppress: () => void;
  resume: () => void;
  dispose: () => void;
  getState: () => TServiceFocusState;
};

export const createFocusIntentController = ({
  openDelay = FOCUS_OPEN_DELAY,
  closeDelay = FOCUS_CLOSE_DELAY,
  onChange,
}: TFocusIntentControllerOptions): TFocusIntentController => {
  let state: TServiceFocusState = {
    hoveredServiceId: null,
    pinnedServiceId: null,
    suppressed: false,
  };
  let openTimer: ReturnType<typeof setTimeout> | null = null;
  let closeTimer: ReturnType<typeof setTimeout> | null = null;

  const emit = () => onChange({ ...state });

  const clearOpenTimer = () => {
    if (openTimer !== null) {
      clearTimeout(openTimer);
      openTimer = null;
    }
  };

  const clearCloseTimer = () => {
    if (closeTimer !== null) {
      clearTimeout(closeTimer);
      closeTimer = null;
    }
  };

  const clearTimers = () => {
    clearOpenTimer();
    clearCloseTimer();
  };

  const scheduleClose = () => {
    clearCloseTimer();
    closeTimer = setTimeout(() => {
      closeTimer = null;
      if (state.pinnedServiceId) return;
      state = { ...state, hoveredServiceId: null };
      emit();
    }, closeDelay);
  };

  const hover = (serviceId: string) => {
    if (state.pinnedServiceId || state.suppressed) return;

    clearCloseTimer();

    if (state.hoveredServiceId === serviceId) return;

    clearOpenTimer();
    openTimer = setTimeout(() => {
      openTimer = null;
      if (state.pinnedServiceId || state.suppressed) return;
      state = { ...state, hoveredServiceId: serviceId };
      emit();
    }, openDelay);
  };

  const leaveNode = () => {
    if (state.pinnedServiceId) return;

    clearOpenTimer();

    if (!state.hoveredServiceId) {
      clearCloseTimer();
      return;
    }

    scheduleClose();
  };

  const enterCard = () => {
    clearCloseTimer();
  };

  const leaveCard = () => {
    if (state.pinnedServiceId) return;
    if (state.hoveredServiceId) scheduleClose();
  };

  const pin = (serviceId: string) => {
    clearTimers();
    if (state.pinnedServiceId === serviceId && state.hoveredServiceId === null) return;

    state = {
      hoveredServiceId: null,
      pinnedServiceId: serviceId,
      suppressed: state.suppressed,
    };
    emit();
  };

  const clear = () => {
    clearTimers();
    state = { ...state, hoveredServiceId: null, pinnedServiceId: null };
    emit();
  };

  const suppress = () => {
    clearTimers();
    state = { ...state, hoveredServiceId: null, suppressed: true };
    emit();
  };

  const resume = () => {
    if (!state.suppressed) return;
    state = { ...state, suppressed: false };
    emit();
  };

  const dispose = () => {
    clearTimers();
  };

  return {
    hover,
    leaveNode,
    enterCard,
    leaveCard,
    pin,
    clear,
    suppress,
    resume,
    dispose,
    getState: () => ({ ...state }),
  };
};
