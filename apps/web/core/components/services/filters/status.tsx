/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useTranslation } from "@plane/i18n";
import type { TServiceStatus } from "@plane/types";
// components
import { FilterHeader, FilterOption } from "@/components/issues/issue-layouts/filters";

export const SERVICE_STATUS_OPTIONS: { value: TServiceStatus; i18n_label: string }[] = [
  { value: "active", i18n_label: "service.status_values.active" },
  { value: "planned", i18n_label: "service.status_values.planned" },
  { value: "maintenance", i18n_label: "service.status_values.maintenance" },
  { value: "deprecated", i18n_label: "service.status_values.deprecated" },
  { value: "retired", i18n_label: "service.status_values.retired" },
];

type Props = {
  appliedFilters: TServiceStatus[] | null;
  handleUpdate: (val: string) => void;
  searchQuery: string;
};

export const FilterServiceStatus = observer(function FilterServiceStatus(props: Props) {
  const { appliedFilters, handleUpdate, searchQuery } = props;
  // states
  const [previewEnabled, setPreviewEnabled] = useState(true);
  const { t } = useTranslation();

  const filteredOptions = SERVICE_STATUS_OPTIONS.filter((option) =>
    t(option.i18n_label).toLowerCase().includes(searchQuery.toLowerCase())
  );
  const appliedFiltersCount = appliedFilters?.length ?? 0;

  return (
    <>
      <FilterHeader
        title={`${t("service.fields.status")}${appliedFiltersCount > 0 ? ` (${appliedFiltersCount})` : ""}`}
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
