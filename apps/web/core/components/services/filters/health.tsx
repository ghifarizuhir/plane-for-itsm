/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useTranslation } from "@plane/i18n";
import type { TServiceHealth } from "@plane/types";
// components
import { FilterHeader, FilterOption } from "@/components/issues/issue-layouts/filters";

export const SERVICE_HEALTH_OPTIONS: { value: TServiceHealth; i18n_label: string }[] = [
  { value: "down", i18n_label: "service.health_values.down" },
  { value: "degraded", i18n_label: "service.health_values.degraded" },
  { value: "healthy", i18n_label: "service.health_values.healthy" },
  { value: "unknown", i18n_label: "service.health_values.unknown" },
];

type Props = {
  appliedFilters: TServiceHealth[] | null;
  handleUpdate: (val: string) => void;
  searchQuery: string;
};

export const FilterServiceHealth = observer(function FilterServiceHealth(props: Props) {
  const { appliedFilters, handleUpdate, searchQuery } = props;
  // states
  const [previewEnabled, setPreviewEnabled] = useState(true);
  const { t } = useTranslation();

  const filteredOptions = SERVICE_HEALTH_OPTIONS.filter((option) =>
    t(option.i18n_label).toLowerCase().includes(searchQuery.toLowerCase())
  );
  const appliedFiltersCount = appliedFilters?.length ?? 0;

  return (
    <>
      <FilterHeader
        title={`${t("service.fields.health")}${appliedFiltersCount > 0 ? ` (${appliedFiltersCount})` : ""}`}
        isPreviewEnabled={previewEnabled}
        handleIsPreviewEnabled={() => setPreviewEnabled(!previewEnabled)}
      />
      {previewEnabled && (
        <div>
          {filteredOptions.length > 0 ? (
            filteredOptions.map((option) => (
              <FilterOption
                key={option.value}
                isChecked={appliedFilters?.includes(option.value) ?? false}
                onClick={() => handleUpdate(option.value)}
                title={t(option.i18n_label)}
              />
            ))
          ) : (
            <p className="text-11 text-placeholder italic">{t("common.search.no_matches_found")}</p>
          )}
        </div>
      )}
    </>
  );
});
