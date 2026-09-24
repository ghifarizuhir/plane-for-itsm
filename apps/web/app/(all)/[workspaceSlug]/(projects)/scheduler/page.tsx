/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { PageHead } from "@/components/core/page-title";
import { SchedulerView } from "@/components/ai-scheduler/scheduler-view";

export default function WorkspaceSchedulerPage() {
  return (
    <>
      <PageHead title="Scheduler" />
      <div className="relative h-full w-full overflow-hidden overflow-y-auto">
        <SchedulerView />
      </div>
    </>
  );
}
