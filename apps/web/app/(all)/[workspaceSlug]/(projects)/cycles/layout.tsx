/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
// local imports
import { WorkspaceCyclesCreateModals } from "./create-modals";
import { WorkspaceCyclesHeader } from "./header";

function WorkspaceSprintsLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceCyclesHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
      <WorkspaceCyclesCreateModals />
    </>
  );
}

export default observer(WorkspaceSprintsLayout);
