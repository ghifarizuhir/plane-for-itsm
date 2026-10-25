/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useId } from "react";
import { LayoutGroup, motion, useReducedMotion } from "framer-motion";
// plane imports
import { ISSUE_LAYOUTS, MOTION_TRANSITION } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import type { EIssueLayoutTypes } from "@plane/types";
import { cn } from "@plane/utils";
// components
import { IssueLayoutIcon } from "@/components/issues/issue-layouts/layout-icon";
// hooks
import { usePlatformOS } from "@/hooks/use-platform-os";

type Props = {
  layouts: EIssueLayoutTypes[];
  onChange: (layout: EIssueLayoutTypes) => void;
  selectedLayout: EIssueLayoutTypes | undefined;
};

export function LayoutSelection(props: Props) {
  const { layouts, onChange, selectedLayout } = props;
  const { isMobile } = usePlatformOS();
  const { t } = useTranslation();
  const layoutGroupId = useId();
  const reducedMotion = useReducedMotion();
  const handleOnChange = (layoutKey: EIssueLayoutTypes) => {
    if (selectedLayout !== layoutKey) {
      onChange(layoutKey);
    }
  };

  return (
    <LayoutGroup id={layoutGroupId}>
      <div className="flex items-center gap-1 rounded-md bg-layer-3 p-1">
        {ISSUE_LAYOUTS.map((layout) => {
          if (!layouts.includes(layout.key)) return null;
          const isSelected = selectedLayout === layout.key;
          return (
            <Tooltip key={layout.key} label={t(layout.i18n_title)} disabled={isMobile}>
              <button
                type="button"
                aria-label={t(layout.i18n_title)}
                aria-pressed={isSelected}
                className={cn(
                  "group relative grid h-5.5 w-7 place-items-center rounded-sm transition-colors",
                  !isSelected && "hover:bg-layer-transparent-hover"
                )}
                onClick={() => handleOnChange(layout.key)}
              >
                {isSelected && (
                  <motion.span
                    layoutId={`layout-indicator-${layoutGroupId}`}
                    className="absolute inset-0 rounded-sm bg-layer-transparent-active"
                    transition={reducedMotion ? { duration: 0 } : MOTION_TRANSITION.panel}
                  />
                )}
                <IssueLayoutIcon
                  layout={layout.key}
                  size={14}
                  strokeWidth={2}
                  className={cn(
                    "relative z-[1] size-3.5 transition-colors",
                    isSelected ? "text-primary" : "text-secondary"
                  )}
                />
              </button>
            </Tooltip>
          );
        })}
      </div>
    </LayoutGroup>
  );
}
