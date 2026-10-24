/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

const SERVICE_HEALTH_REFRESH_MS = 60_000;

export const useServiceHealthPolling = (workspaceSlug: string | undefined, projectId: string | null) => {
  const { refreshHealth } = useService();
  const { currentWorkspace } = useWorkspace();

  useEffect(() => {
    if (!workspaceSlug || !currentWorkspace?.id) return;

    const refresh = () => {
      void refreshHealth(workspaceSlug, projectId);
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") refresh();
    };

    const interval = window.setInterval(refresh, SERVICE_HEALTH_REFRESH_MS);
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", refresh);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [currentWorkspace?.id, projectId, refreshHealth, workspaceSlug]);
};
