/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Outlet } from "react-router";
import { GalileoLayout } from "@/components/galileo/layout";

export default function GalileoSectionLayout() {
  return (
    <GalileoLayout>
      <Outlet />
    </GalileoLayout>
  );
}
