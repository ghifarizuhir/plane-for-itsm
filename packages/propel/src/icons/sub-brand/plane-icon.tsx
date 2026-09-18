/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import * as React from "react";

import { IconWrapper } from "../icon-wrapper";
import type { ISvgIcons } from "../type";

export function PlaneNewIcon({ color = "currentColor", ...rest }: ISvgIcons) {
  return (
    <IconWrapper color={color} {...rest}>
      <rect x="1.5" y="2" width="13" height="3" rx="1.5" fill={color} />
      <rect x="3.5" y="6.5" width="9" height="3" rx="1.5" fill={color} />
      <rect x="5.5" y="11" width="5" height="3" rx="1.5" fill={color} />
    </IconWrapper>
  );
}
