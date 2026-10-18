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
import { WorkspaceKbListView } from "@/components/pages/workspace/workspace-kb-list-view";

function WorkspaceKbPage() {
  const searchParams = useSearchParams();
  const typeFilter = searchParams.get("type") ?? undefined;
  const projectFilter = searchParams.get("project") ?? undefined;
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("sidebar.pages")} />
      <WorkspaceKbListView typeFilter={typeFilter} projectFilter={projectFilter} />
    </>
  );
}

export default observer(WorkspaceKbPage);
