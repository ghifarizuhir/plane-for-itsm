/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ClockOutline } from "@makeplane/propel/icons";
import { calculateTimeAgo } from "@plane/utils";

type Props = {
  lastDeployedAt: string | null;
};

export function ServiceDeployCell({ lastDeployedAt }: Props) {
  if (!lastDeployedAt) return <span className="text-12 text-tertiary">—</span>;
  return (
    <span className="flex items-center gap-1 text-12 text-secondary">
      <ClockOutline className="h-3.5 w-3.5 text-tertiary" />
      <span className="font-code tabular-nums">{calculateTimeAgo(lastDeployedAt)}</span>
    </span>
  );
}
