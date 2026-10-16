/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Outlet } from "react-router";
// components
import { DocsLayout } from "@/components/docs/docs-layout";
import { ProjectsAppPowerKProvider } from "@/components/power-k/projects-app-provider";

export default function DocsSectionLayout() {
  return (
    <>
      <ProjectsAppPowerKProvider />
      <div className="relative flex size-full overflow-hidden rounded-lg border border-subtle">
        <DocsLayout>
          <Outlet />
        </DocsLayout>
      </div>
    </>
  );
}
