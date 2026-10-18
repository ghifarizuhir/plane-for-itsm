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
import { WorkspaceTracksListView } from "@/components/modules/workspace/workspace-tracks-list-view";

function WorkspaceTracksPage() {
  const searchParams = useSearchParams();
  const projectFilter = searchParams.get("project") ?? undefined;
  const { t } = useTranslation();

  return (
    <>
      <PageHead title={t("sidebar.modules")} />
      <WorkspaceTracksListView projectFilter={projectFilter} />
    </>
  );
}

export default observer(WorkspaceTracksPage);
