/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { ChevronDownOutline } from "@makeplane/propel/icons";
import { CustomMenu, Row } from "@plane/ui";
// components
import { SERVICE_VIEW_LAYOUTS, ServiceLayoutIcon } from "./service-layout-icon";
// hooks
import { useServiceFilter } from "@/hooks/store/use-service-filter";

export const ServiceMobileHeader = observer(function ServiceMobileHeader() {
  // router
  const { projectId } = useParams();
  // store hooks
  const { updateDisplayFilters } = useServiceFilter();

  return (
    <div className="flex justify-start md:hidden">
      <CustomMenu
        maxHeight="md"
        className="flex flex-grow justify-start border-b border-subtle bg-surface-1 py-2 text-13 text-secondary"
        customButton={
          <Row className="flex flex-grow justify-center gap-2 text-13 text-secondary">
            <span>Layout</span> <ChevronDownOutline className="my-auto h-4 w-4 text-secondary" />
          </Row>
        }
        customButtonClassName="flex flex-grow justify-center items-center text-secondary text-13"
        closeOnSelect
      >
        {SERVICE_VIEW_LAYOUTS.map((layout) => (
          <CustomMenu.MenuItem
            key={layout.key}
            onClick={() => {
              if (!projectId) return;
              updateDisplayFilters(projectId.toString(), { layout: layout.key });
            }}
            className="flex items-center gap-2"
          >
            <ServiceLayoutIcon layoutType={layout.key} />
            <div className="text-tertiary">{layout.label}</div>
          </CustomMenu.MenuItem>
        ))}
      </CustomMenu>
    </div>
  );
});
