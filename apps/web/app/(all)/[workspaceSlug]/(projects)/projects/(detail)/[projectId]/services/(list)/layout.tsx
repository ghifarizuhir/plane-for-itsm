/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Outlet } from "react-router";
// components
import { AppHeader } from "@/components/core/app-header";
import { ContentWrapper } from "@/components/core/content-wrapper";
import { ServiceMobileHeader, ServiceViewHeader } from "@/components/services";

export default function ProjectServicesListLayout() {
  return (
    <>
      <AppHeader header={<ServiceViewHeader />} mobileHeader={<ServiceMobileHeader />} />
      <ContentWrapper>
        <Outlet />
      </ContentWrapper>
    </>
  );
}
