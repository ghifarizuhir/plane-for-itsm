/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import * as React from "react";

// Terraline logo lockup. Not part of @makeplane/propel's icon set, so it is kept as a local asset.

type PlaneLockupProps = {
  width?: string | number;
  height?: string | number;
  className?: string;
  color?: string;
};

export function PlaneLockup({ width = "253", height = "53", className, color = "currentColor" }: PlaneLockupProps) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 253 53"
      fill={color}
      xmlns="http://www.w3.org/2000/svg"
      className={className}
    >
      <rect x="0" y="0.5" width="85" height="14" rx="7" fill={color} />
      <rect x="10.625" y="19.5" width="63.75" height="14" rx="7" fill={color} />
      <rect x="21.25" y="38.5" width="42.5" height="14" rx="7" fill={color} />
      <text
        x="96"
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
