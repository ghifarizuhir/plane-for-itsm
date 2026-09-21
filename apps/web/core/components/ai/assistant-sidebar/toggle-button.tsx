/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { AiStar1Outline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { cn } from "@plane/utils";
import { useAppTheme } from "@/hooks/store/use-app-theme";

export const AiAssistantSidebarToggle = observer(function AiAssistantSidebarToggle() {
  const { aiSidebarCollapsed, toggleAiSidebar } = useAppTheme();
  const isOpen = aiSidebarCollapsed === false;

  return (
    <Tooltip label="AI Assistant" side="bottom">
      <button
        type="button"
        aria-label="AI Assistant"
        aria-expanded={isOpen}
        onClick={() => toggleAiSidebar(isOpen)}
        className={cn("flex size-8 items-center justify-center rounded-md transition-colors hover:bg-layer-1-hover", {
          "bg-layer-1": isOpen,
        })}
      >
        <AiStar1Outline className="size-5 text-primary" />
      </button>
    </Tooltip>
  );
});
