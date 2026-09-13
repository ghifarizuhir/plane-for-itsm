/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useTranslation } from "@plane/i18n";
import type { TServiceType } from "@plane/types";
// components
import { FilterHeader, FilterOption } from "@/components/issues/issue-layouts/filters";

export const SERVICE_TYPE_OPTIONS: { value: TServiceType; i18n_label: string }[] = [
  { value: "internal", i18n_label: "service.type_values.internal" },
  { value: "external", i18n_label: "service.type_values.external" },
  { value: "infrastructure", i18n_label: "service.type_values.infrastructure" },
  { value: "third_party", i18n_label: "service.type_values.third_party" },
];

type Props = {
  appliedFilters: TServiceType[] | null;
  handleUpdate: (val: string) => void;
  searchQuery: string;
};

export const FilterServiceType = observer(function FilterServiceType(props: Props) {
  const { appliedFilters, handleUpdate, searchQuery } = props;
  // states
  const [previewEnabled, setPreviewEnabled] = useState(true);
  const { t } = useTranslation();

  const filteredOptions = SERVICE_TYPE_OPTIONS.filter((option) =>
    t(option.i18n_label).toLowerCase().includes(searchQuery.toLowerCase())
  );
  const appliedFiltersCount = appliedFilters?.length ?? 0;

  return (
    <>
      <FilterHeader
        title={`${t("service.fields.type")}${appliedFiltersCount > 0 ? ` (${appliedFiltersCount})` : ""}`}
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
