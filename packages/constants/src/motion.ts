/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const MOTION_DURATION = {
  micro: 0.12,
  panel: 0.18,
  overlay: 0.22,
} as const;

const EASE_ENTER: [number, number, number, number] = [0.2, 0, 0, 1];
const EASE_EXIT: [number, number, number, number] = [0.4, 0, 1, 1];

export const MOTION_EASE = {
  enter: EASE_ENTER,
  exit: EASE_EXIT,
} as const;

export const MOTION_TRANSITION = {
  micro: { duration: MOTION_DURATION.micro, ease: EASE_ENTER },
  panel: { duration: MOTION_DURATION.panel, ease: EASE_ENTER },
  overlay: { duration: MOTION_DURATION.overlay, ease: EASE_ENTER },
} as const;
