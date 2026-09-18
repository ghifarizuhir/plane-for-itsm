/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import * as React from "react";

import type { ISvgIcons } from "../type";

export function PlaneWordmark({ width = "146", height = "44", className, color = "currentColor", ...rest }: ISvgIcons) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 146 44"
      fill={color}
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...rest}
    >
      <text
        x="0"
        y="32"
        fill={color}
        fontFamily="Inter Variable, Inter, ui-sans-serif, system-ui, sans-serif"
        fontSize="34"
        fontWeight="600"
        letterSpacing="-0.02em"
        textLength="146"
        lengthAdjust="spacingAndGlyphs"
      >
        Terraline
      </text>
    </svg>
  );
}
