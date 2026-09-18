/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import * as React from "react";

import type { ISvgIcons } from "../type";

export function PlaneLogo({ width = "85", height = "52", className, color = "currentColor", ...rest }: ISvgIcons) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 85 52"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...rest}
    >
      <rect x="0" y="0" width="85" height="14" rx="7" fill={color} />
      <rect x="10.625" y="19" width="63.75" height="14" rx="7" fill={color} />
      <rect x="21.25" y="38" width="42.5" height="14" rx="7" fill={color} />
    </svg>
  );
}
