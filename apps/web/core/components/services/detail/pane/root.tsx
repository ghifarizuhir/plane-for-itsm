/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { ArrowRightCircle } from "lucide-react";
// plane imports
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { useTranslation } from "@plane/i18n";
import { Tabs } from "@plane/propel/tabs";
import { cn } from "@plane/utils";
// hooks
import { useServiceDetailPane } from "@/hooks/services/use-service-detail-pane";
// local imports
import { SERVICE_DETAIL_PANE_WIDTH } from "./helpers";
import { ServiceDetailPaneTabPanelsRoot } from "./tab-panels/root";
import { ServiceDetailPaneTabsList } from "./tabs-list";

type Props = {
  serviceId: string;
};

export function ServiceDetailPane(props: Props) {
  const { serviceId } = props;
  // plane hooks
  const { t } = useTranslation();
  // hooks
  const { activeTab, isOpen, closePane, setActiveTab } = useServiceDetailPane();

  return (
    <aside
      className={cn(
        "flex w-full shrink-0 flex-col border-t border-subtle bg-surface-1 pt-3.5 transition-all duration-300 ease-out md:h-full md:w-80 md:border-t-0 md:border-l",
        !isOpen && "hidden md:flex"
      )}
      style={{ marginRight: isOpen ? "0px" : `-${SERVICE_DETAIL_PANE_WIDTH}px` }}
    >
      <div className="mb-3.5 px-3.5">
        <Tooltip label={t("page_navigation_pane.close_button")}>
          <button
            type="button"
            className="grid size-3.5 place-items-center text-secondary transition-colors hover:text-primary"
            onClick={closePane}
            aria-label={t("page_navigation_pane.close_button")}
          >
            <ArrowRightCircle className="size-3.5" />
          </button>
        </Tooltip>
      </div>

      <div className="animate-slide-in-right flex min-h-0 flex-1 flex-col overflow-hidden">
        {isOpen ? (
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <ServiceDetailPaneTabsList />
            <ServiceDetailPaneTabPanelsRoot serviceId={serviceId} />
          </Tabs>
        ) : null}
      </div>
    </aside>
  );
}
