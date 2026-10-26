/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { Controls } from "@xyflow/react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { RefreshOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { cn } from "@plane/utils";
// local imports
import { SERVICE_GRAPH_CONTROLS_SLOT_ID } from "./service-graph-controls-slot";
import { GRAPH_FIT_VIEW_PADDING } from "./use-graph-layout";

type TServiceGraphHeaderControlsProps = {
  onReLayout?: () => void;
  isReLayouting?: boolean;
};

/**
 * The graph has no floating chrome of its own. Re-layout and the React Flow
 * viewport controls live in the page header, next to the layout switcher.
 */
export function ServiceGraphHeaderControls({ onReLayout, isReLayouting }: TServiceGraphHeaderControlsProps) {
  // plane hooks
  const { t } = useTranslation();
  // states
  const [target, setTarget] = useState<HTMLElement | null>(null);

  useEffect(() => {
    const findTarget = () => document.getElementById(SERVICE_GRAPH_CONTROLS_SLOT_ID);
    const element = findTarget();
    if (element) {
      setTarget(element);
      return;
    }

    const observer = new MutationObserver(() => {
      const next = findTarget();
      if (!next) return;
      setTarget(next);
      observer.disconnect();
    });
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, []);

  if (!target) return null;

  return createPortal(
    <div className="service-graph-header-controls flex items-center gap-0.5 rounded-sm bg-layer-3 p-1">
      {onReLayout && (
        <>
          <Tooltip label={t("service.graph.re_layout")}>
            <button
              type="button"
              onClick={onReLayout}
              disabled={isReLayouting}
              aria-label={t("service.graph.re_layout")}
              className="grid h-5.5 w-7 place-items-center rounded-sm text-secondary transition-colors hover:bg-layer-transparent-hover hover:text-primary focus-visible:ring-2 focus-visible:ring-accent-strong focus-visible:outline-none disabled:opacity-50"
            >
              <RefreshOutline className={cn("h-3 w-3", isReLayouting && "animate-spin")} />
            </button>
          </Tooltip>
          <span aria-hidden className="bg-subtle mx-0.5 h-3.5 w-px shrink-0" />
        </>
      )}
      <Controls
        orientation="horizontal"
        showInteractive={false}
        fitViewOptions={{ padding: GRAPH_FIT_VIEW_PADDING }}
        style={{ position: "relative", margin: 0, boxShadow: "none", border: 0, background: "transparent" }}
      />
    </div>,
    target
  );
}
