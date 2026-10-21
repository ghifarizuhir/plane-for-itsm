/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// components
import { CyclesListItem } from "./cycles-list-item";

type Props = {
  cycleIds: string[];
  projectId?: string;
  workspaceSlug: string;
  showProjectBadge?: boolean;
};

export function CyclesListMap(props: Props) {
  const { cycleIds, projectId, workspaceSlug, showProjectBadge = false } = props;

  return (
    <>
      {cycleIds.map((cycleId) => (
        <CyclesListItem
          key={cycleId}
          cycleId={cycleId}
          workspaceSlug={workspaceSlug}
          projectId={projectId}
          showProjectBadge={showProjectBadge}
        />
      ))}
    </>
  );
}
