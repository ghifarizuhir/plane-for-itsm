/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useSyncExternalStore } from "react";

export const MOBILE_VIEWPORT_QUERY = "(max-width: 767px)";
export const TOUCH_POINTER_QUERY = "(pointer: coarse)";

const getMatches = (query: string): boolean => {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return false;
  return window.matchMedia(query).matches;
};

export const getIsMobileViewport = (): boolean => getMatches(MOBILE_VIEWPORT_QUERY);
export const getIsTouchPointer = (): boolean => getMatches(TOUCH_POINTER_QUERY);

export const subscribeToMediaQuery = (query: string, onChange: () => void): (() => void) => {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return () => {};
  const mediaQueryList = window.matchMedia(query);
  mediaQueryList.addEventListener("change", onChange);
  return () => mediaQueryList.removeEventListener("change", onChange);
};

const getServerSnapshot = () => false;

export const useMediaQuery = (query: string): boolean => {
  const subscribe = useCallback((onStoreChange: () => void) => subscribeToMediaQuery(query, onStoreChange), [query]);
  const getSnapshot = useCallback(() => getMatches(query), [query]);
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
};

export const useMobileViewport = (): boolean => useMediaQuery(MOBILE_VIEWPORT_QUERY);
export const useTouchPointer = (): boolean => useMediaQuery(TOUCH_POINTER_QUERY);
