/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { CloseOutline, SearchOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import type { TServiceFilters } from "@plane/types";
// components
import { FilterServiceCriticality, FilterServiceStatus, FilterServiceType } from "@/components/services";

type Props = {
  filters: TServiceFilters;
  handleFiltersUpdate: (key: keyof TServiceFilters, value: string | string[]) => void;
};

export const ServiceFiltersSelection = observer(function ServiceFiltersSelection(props: Props) {
  const { filters, handleFiltersUpdate } = props;
  // states
  const [filtersSearchQuery, setFiltersSearchQuery] = useState("");
  // hooks
  const { t } = useTranslation();

  return (
    <div className="flex h-full w-full flex-col overflow-hidden">
      <div className="bg-surface-1 p-2.5 pb-0">
        <div className="flex items-center gap-1.5 rounded-sm border-[0.5px] border-subtle bg-surface-2 px-1.5 py-1 text-11">
          <SearchOutline className="text-placeholder" width={12} height={12} />
          <input
            type="text"
            className="w-full bg-surface-2 outline-none placeholder:text-placeholder"
            placeholder={t("common.search.label")}
            value={filtersSearchQuery}
            onChange={(e) => setFiltersSearchQuery(e.target.value)}
          />
          {filtersSearchQuery !== "" && (
            <button type="button" className="grid place-items-center" onClick={() => setFiltersSearchQuery("")}>
              <CloseOutline className="text-tertiary" height={12} width={12} />
            </button>
          )}
        </div>
      </div>
      <div className="vertical-scrollbar scrollbar-sm h-full w-full divide-y divide-subtle-1 overflow-y-auto px-2.5">
        {/* status */}
        <div className="py-2">
          <FilterServiceStatus
            appliedFilters={filters.status ?? null}
            handleUpdate={(val) => handleFiltersUpdate("status", val)}
            searchQuery={filtersSearchQuery}
          />
        </div>

        {/* criticality */}
        <div className="py-2">
          <FilterServiceCriticality
            appliedFilters={filters.criticality ?? null}
            handleUpdate={(val) => handleFiltersUpdate("criticality", val)}
            searchQuery={filtersSearchQuery}
          />
        </div>

        {/* type */}
        <div className="py-2">
          <FilterServiceType
            appliedFilters={filters.type ?? null}
            handleUpdate={(val) => handleFiltersUpdate("type", val)}
            searchQuery={filtersSearchQuery}
          />
        </div>
      </div>
    </div>
  );
});
