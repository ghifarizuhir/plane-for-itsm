/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { ChevronDownOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
// hooks
import { useServiceFilter } from "@/hooks/store/use-service-filter";
// helpers
import { groupServices } from "@/services/service.helpers";
// types
import type {
  IService,
  IServiceHealthSnapshot,
  TServiceCriticality,
  TServiceHealth,
  TServiceStatus,
} from "@plane/types";
// local imports
import { ServicesListRow } from "./services-list-row";

type Props = {
  services: IService[];
  filterKey: string;
  healthMap: Record<string, IServiceHealthSnapshot>;
};

export const ServicesList = observer(function ServicesList({ services, filterKey, healthMap }: Props) {
  const { t } = useTranslation();
  const { getDisplayFiltersByProjectId } = useServiceFilter();
  const groupBy = getDisplayFiltersByProjectId(filterKey)?.group_by ?? "status";
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const groups = groupServices(services, groupBy, healthMap);

  const toggleGroup = (key: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const groupLabel = (key: string): string => {
    if (groupBy === "status") return t(`service.status_values.${key as TServiceStatus}`);
    if (groupBy === "criticality") return t(`service.criticality_values.${key as TServiceCriticality}`);
    if (groupBy === "health") return t(`service.health_values.${key as TServiceHealth}`);
    return key;
  };

  return (
    <div className="flex h-full w-full flex-col">
      <div className="flex items-center gap-3 border-b border-subtle px-3 py-1 text-9 font-medium tracking-wide text-tertiary uppercase">
        <span className="flex-1">{t("service.list.service")}</span>
        <span className="hidden w-[100px] shrink-0 sm:block">{t("service.list.status")}</span>
        <span className="hidden w-[90px] shrink-0 md:block">{t("service.list.criticality")}</span>
        <span className="hidden w-[90px] shrink-0 md:block">{t("service.list.health")}</span>
        <span className="hidden w-[100px] shrink-0 lg:block">{t("service.list.incidents")}</span>
        <span className="hidden w-[90px] shrink-0 xl:block">{t("service.list.deploy")}</span>
        <span className="hidden w-[40px] shrink-0 text-right xl:block">{t("service.list.owner")}</span>
      </div>
      <div className="vertical-scrollbar min-h-0 flex-1">
        {groups.map((group) => {
          const isCollapsed = collapsed.has(group.key);
          return (
            <div key={group.key}>
              {groupBy && (
                <button
                  type="button"
                  onClick={() => toggleGroup(group.key)}
                  aria-expanded={!isCollapsed}
                  className="flex w-full items-center gap-1.5 border-b border-subtle bg-layer-1 px-3 py-1.5 text-11 font-medium text-secondary hover:text-primary"
                >
                  <ChevronDownOutline
                    className={cn("h-3.5 w-3.5 text-tertiary transition-transform", isCollapsed && "-rotate-90")}
                  />
                  <span className="capitalize">{groupLabel(group.key)}</span>
                  <span className="font-code text-10 text-tertiary tabular-nums">{group.services.length}</span>
                </button>
              )}
              {!isCollapsed &&
                group.services.map((service) => <ServicesListRow key={service.id} serviceId={service.id} />)}
            </div>
          );
        })}
      </div>
    </div>
  );
});
