/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import * as React from "react";

import type { ISvgIcons } from "../type";

export type TPlaneLogoBoxedProps = ISvgIcons & {
  boxColor?: string;
  markColor?: string;
};

/**
 * App icon treatment: the core mark centered inside the brand rounded box.
 * Geometry mirrors the favicon/PWA icons so every surface stays pixel-consistent.
 */
export function PlaneLogoBoxed({
  width = "48",
  height = "48",
  className,
  boxColor = "#3F76FF",
  markColor = "#FFFFFF",
  ...rest
}: TPlaneLogoBoxedProps) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 512 512"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className={className}
      {...rest}
    >
      <rect width="512" height="512" rx="110" fill={boxColor} />
      <g className="terraline-logo-mark" fill={markColor} transform="translate(105.5 164.24) scale(3.5294)">
        <rect width="85" height="14" rx="7" />
        <rect x="10.625" y="19" width="63.75" height="14" rx="7" />
        <rect x="21.25" y="38" width="42.5" height="14" rx="7" />
      </g>
    </svg>
  );
}
