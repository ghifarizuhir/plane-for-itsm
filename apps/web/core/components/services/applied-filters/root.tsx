/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { CloseOutline } from "@makeplane/propel/icons";
import { PillButton } from "@makeplane/propel/components/pill";
import { useTranslation } from "@plane/i18n";
import type { TServiceFilters } from "@plane/types";
// components
import { Header, EHeaderVariant } from "@plane/ui";

type Props = {
  appliedFilters: TServiceFilters;
  handleClearAllFilters: () => void;
  handleRemoveFilter: (key: keyof TServiceFilters, value: string | null) => void;
  alwaysAllowEditing?: boolean;
};

const SERVICE_FILTER_VALUE_I18N: Record<keyof TServiceFilters, (value: string) => string> = {
  status: (value) => `service.status_values.${value}`,
  criticality: (value) => `service.criticality_values.${value}`,
  type: (value) => `service.type_values.${value}`,
};

export const ServiceAppliedFiltersList = observer(function ServiceAppliedFiltersList(props: Props) {
  const { appliedFilters, handleClearAllFilters, handleRemoveFilter, alwaysAllowEditing } = props;
  const { t } = useTranslation();

  if (!appliedFilters) return null;
  if (Object.keys(appliedFilters).length === 0) return null;

  const isEditingAllowed = alwaysAllowEditing;

  return (
    <Header variant={EHeaderVariant.TERNARY}>
      <div className="flex flex-wrap gap-2">
        {(Object.entries(appliedFilters) as [keyof TServiceFilters, string[] | undefined][]).map(([key, value]) => {
          if (!value) return null;
          if (Array.isArray(value) && value.length === 0) return null;
          const values = Array.isArray(value) ? value : [value];

          return (
            <div
              key={key}
              className="my-auto flex min-h-9 cursor-pointer flex-wrap items-center gap-1.5 rounded-md border border-subtle p-1.5 text-11 text-tertiary capitalize hover:text-secondary"
            >
              <div className="flex flex-wrap items-center gap-1.5">
                <span className="text-11 text-tertiary">{t(`service.fields.${key}`)}</span>
                {values.map((val) => (
                  <div key={val} className="flex items-center gap-1 rounded-sm bg-layer-1 p-1 text-11">
                    {t(SERVICE_FILTER_VALUE_I18N[key](val))}
                    {isEditingAllowed && (
                      <button
                        type="button"
                        className="grid place-items-center text-tertiary hover:text-secondary"
                        onClick={() => handleRemoveFilter(key, val)}
                      >
                        <CloseOutline height={10} width={10} />
                      </button>
                    )}
                  </div>
                ))}
                {isEditingAllowed && (
                  <button
                    type="button"
                    className="grid place-items-center text-tertiary hover:text-secondary"
                    onClick={() => handleRemoveFilter(key, null)}
                  >
                    <CloseOutline height={12} width={12} />
                  </button>
                )}
              </div>
            </div>
          );
        })}
        {isEditingAllowed && (
          <PillButton
            type="button"
            size="md"
            variant="outline"
            label={t("common.clear_all")}
            endIcon={<CloseOutline height={12} width={12} />}
            onClick={handleClearAllFilters}
          />
        )}
      </div>
    </Header>
  );
});
