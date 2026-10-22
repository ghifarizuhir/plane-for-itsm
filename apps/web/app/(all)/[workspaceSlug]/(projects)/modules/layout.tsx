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
import { WorkspaceModulesCreateModals } from "./create-modals";
import { WorkspaceModulesHeader, WorkspaceModulesMobileHeader } from "./header";

function WorkspaceModulesLayout() {
  return (
    <>
      <AppHeader header={<WorkspaceModulesHeader />} mobileHeader={<WorkspaceModulesMobileHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
      <WorkspaceModulesCreateModals />
    </>
  );
}

export default observer(WorkspaceModulesLayout);
