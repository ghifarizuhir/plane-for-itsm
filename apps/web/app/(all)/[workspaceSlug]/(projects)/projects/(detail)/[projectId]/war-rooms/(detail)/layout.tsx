/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Outlet } from "react-router";

/**
 * War rooms are incident command surfaces: they render in their own browser tab
 * and take over the viewport. `fullscreen` tells the workspace and project
 * layouts above to skip the top navigation, app rail and project sidebars.
 */
export const handle = { fullscreen: true };

export default function ProjectWarRoomDetailLayout() {
  return <Outlet />;
}
