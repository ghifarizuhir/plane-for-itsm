/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { AiStar1Outline, CalendarOutline } from "@makeplane/propel/icons";
import { Button } from "@plane/propel/button";
import { Breadcrumbs, Header } from "@plane/ui";
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";
import { CountChip } from "@/components/common/count-chip";
import { useAiAssistant } from "@/hooks/store/use-ai-assistant";
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import { useAppTheme } from "@/hooks/store/use-app-theme";
import { useInstance } from "@/hooks/store/use-instance";
import { useAppRouter } from "@/hooks/use-app-router";

export const WorkspaceSchedulerHeader = observer(function WorkspaceSchedulerHeader() {
  const router = useAppRouter();
  const { workspaceSlug: routerWorkspaceSlug } = useParams();
  const workspaceSlug = routerWorkspaceSlug?.toString();
  const { schedules } = useAiSchedules();
  const { config } = useInstance();
  const { toggleAiSidebar } = useAppTheme();
  const { setMode } = useAiAssistant();

  const openGalileo = () => {
    setMode("agent");
    toggleAiSidebar(false);
  };

  return (
    <Header>
      <Header.LeftItem>
        <div className="flex items-center gap-2.5">
          <Breadcrumbs onBack={router.back}>
            <Breadcrumbs.Item
              component={
                <BreadcrumbLink
                  label="Scheduler"
                  href={`/${workspaceSlug}/scheduler/`}
                  icon={<CalendarOutline className="h-4 w-4 text-tertiary" />}
                  isLast
                />
              }
              isLast
            />
          </Breadcrumbs>
          {schedules.length > 0 && <CountChip count={schedules.length} />}
        </div>
      </Header.LeftItem>
      <Header.RightItem>
        {config?.has_llm_configured && (
          <Button variant="primary" size="lg" prependIcon={<AiStar1Outline className="size-4" />} onClick={openGalileo}>
            <span className="hidden sm:inline">Ask Galileo</span>
            <span className="sm:hidden">Galileo</span>
          </Button>
        )}
      </Header.RightItem>
    </Header>
  );
});
