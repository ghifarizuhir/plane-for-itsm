/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { RightSidePaneOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { cn } from "@plane/utils";
import { useAppTheme } from "@/hooks/store/use-app-theme";

export const AiAssistantSidebarToggle = observer(function AiAssistantSidebarToggle({
  compact = false,
}: {
  compact?: boolean;
}) {
  const { aiSidebarCollapsed, toggleAiSidebar } = useAppTheme();
  const isOpen = aiSidebarCollapsed === false;

  return (
    <Tooltip label="AI Assistant" side="bottom">
      <button
        type="button"
        aria-label="AI Assistant"
        aria-expanded={isOpen}
        onClick={() => toggleAiSidebar(isOpen)}
        className={cn(
          "flex items-center justify-center rounded-md transition-colors focus-visible:ring-2 focus-visible:ring-accent-strong focus-visible:outline-none",
          compact ? "size-7" : "size-8",
          isOpen
            ? "text-icon-primary"
            : "text-icon-tertiary hover:bg-layer-transparent-hover hover:text-icon-secondary",
          isOpen && (compact ? "bg-layer-transparent-selected" : "bg-layer-1")
        )}
      >
        <RightSidePaneOutline className={cn(compact ? "size-4.5" : "size-5")} />
      </button>
    </Tooltip>
  );
});
