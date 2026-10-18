/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useSearchParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { PageHead } from "@/components/core/page-title";
import { WorkspaceIntakeListView } from "@/components/inbox/workspace/workspace-intake-list-view";

function WorkspaceIntakePage() {
  const searchParams = useSearchParams();
  const typeFilter = searchParams.get("currentTab") ?? undefined;
  const projectFilter = searchParams.get("project") ?? undefined;
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("sidebar.intake")} />
      <WorkspaceIntakeListView typeFilter={typeFilter} projectFilter={projectFilter} />
    </>
  );
}

export default observer(WorkspaceIntakePage);
