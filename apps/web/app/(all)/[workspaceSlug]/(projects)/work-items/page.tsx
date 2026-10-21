/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WorkspaceWorkItemsLayoutRoot } from "@/components/issues/issue-layouts/roots/workspace-work-items-layout-root";

function WorkspaceWorkItemsPage() {
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("sidebar.work_items")} />
      <div className="h-full w-full">
        <WorkspaceWorkItemsLayoutRoot />
      </div>
    </>
  );
}

export default observer(WorkspaceWorkItemsPage);
