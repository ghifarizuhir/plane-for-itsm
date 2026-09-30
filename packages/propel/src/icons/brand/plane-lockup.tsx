/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import * as React from "react";

import type { ISvgIcons } from "../type";

export type TPlaneLockupProps = ISvgIcons & {
  boxColor?: string;
  markColor?: string;
};

export function PlaneLockup({
  width = "215",
  height = "53",
  className,
  color = "currentColor",
  boxColor = "#3F76FF",
  markColor = "#FFFFFF",
  ...rest
}: TPlaneLockupProps) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 215 53"
      fill={color}
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...rest}
    >
      <rect x="0" y="0.5" width="52" height="52" rx="11.172" fill={boxColor} />
      <g className="terraline-logo-mark" fill={markColor} transform="translate(10.766 17.18) scale(0.35846)">
        <rect width="85" height="14" rx="7" />
        <rect x="10.625" y="19" width="63.75" height="14" rx="7" />
        <rect x="21.25" y="38" width="42.5" height="14" rx="7" />
      </g>
      <text
        x="63"
        y="38"
        fill={color}
        fontFamily="Inter Variable, Inter, ui-sans-serif, system-ui, sans-serif"
        fontSize="36"
        fontWeight="600"
        letterSpacing="-0.02em"
        textLength="152"
        lengthAdjust="spacingAndGlyphs"
      >
        Terraline
      </text>
    </svg>
  );
}
