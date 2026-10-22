/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React, { useCallback, useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { CloseOutline, FilterOutline, SearchOutline } from "@makeplane/propel/icons";
// plane helpers
import { MODULE_VIEW_LAYOUTS } from "@plane/constants";
import { useOutsideClickDetector } from "@plane/hooks";
// types
import { useTranslation } from "@plane/i18n";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import type { TModuleFilters } from "@plane/types";
// ui
import { cn, calculateTotalFilters } from "@plane/utils";
// plane utils
// components
import { FiltersDropdown } from "@/components/issues/issue-layouts/filters";
import { ModuleFiltersSelection, ModuleOrderByDropdown } from "@/components/modules/dropdowns";
// constants
// helpers
// hooks
import { useMember } from "@/hooks/store/use-member";
import { useModuleFilter } from "@/hooks/store/use-module-filter";
import { usePlatformOS } from "@/hooks/use-platform-os";
import { ModuleLayoutIcon } from "./module-layout-icon";
import { IconButton } from "@plane/propel/icon-button";
// i18n

type Props = {
  filterKey?: string;
  isGanttDisabled?: boolean;
};

export const ModuleViewHeader = observer(function ModuleViewHeader({ filterKey, isGanttDisabled = false }: Props) {
  // refs
  const inputRef = useRef<HTMLInputElement>(null);
  // router
  const { projectId } = useParams();
  const resolvedFilterKey = filterKey ?? projectId?.toString();
  // hooks
  const { isMobile } = usePlatformOS();
  // store hooks
  const {
    workspace: { workspaceMemberIds },
  } = useMember();
  const {
    getDisplayFiltersByProjectId,
    getFiltersByProjectId,
    searchQuery,
    updateDisplayFilters,
    updateFilters,
    updateSearchQuery,
  } = useModuleFilter();
  const { t } = useTranslation();
  const filters = resolvedFilterKey ? getFiltersByProjectId(resolvedFilterKey) : undefined;
  const displayFilters = resolvedFilterKey ? getDisplayFiltersByProjectId(resolvedFilterKey) : undefined;

  // states
  const [isSearchOpen, setIsSearchOpen] = useState(searchQuery !== "");

  // handlers
  const handleFilters = useCallback(
    (key: keyof TModuleFilters, value: string | string[]) => {
      if (!resolvedFilterKey) return;
      const newValues = [...(filters?.[key] ?? [])];

      if (Array.isArray(value))
        value.forEach((val) => {
          if (!newValues.includes(val)) newValues.push(val);
          else newValues.splice(newValues.indexOf(val), 1);
        });
      else {
        if (filters?.[key]?.includes(value)) newValues.splice(newValues.indexOf(value), 1);
        else newValues.push(value);
      }

      updateFilters(resolvedFilterKey, { [key]: newValues });
    },
    [filters, resolvedFilterKey, updateFilters]
  );

  const handleInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      if (searchQuery && searchQuery.trim() !== "") updateSearchQuery("");
      else {
        setIsSearchOpen(false);
        inputRef.current?.blur();
      }
    }
  };

  // outside click detector hook
  useOutsideClickDetector(inputRef, () => {
    if (isSearchOpen && searchQuery.trim() === "") setIsSearchOpen(false);
  });

  useEffect(() => {
    if (searchQuery.trim() !== "") setIsSearchOpen(true);
  }, [searchQuery]);

  const isFiltersApplied = calculateTotalFilters(filters ?? {}) !== 0 || displayFilters?.favorites;

  return (
    <div className="hidden h-full items-center gap-2 self-end sm:flex">
      <div className="flex items-center">
        {!isSearchOpen && (
          <IconButton
            variant="ghost"
            size="lg"
            className="p- -mr-1"
            onClick={() => {
              setIsSearchOpen(true);
              inputRef.current?.focus();
            }}
            icon={SearchOutline}
          />
        )}
        <div
          className={cn(
            "ml-auto flex w-0 items-center justify-start gap-1 overflow-hidden rounded-md border border-transparent bg-surface-1 text-placeholder opacity-0 transition-[width] ease-linear",
            {
              "w-64 border-subtle px-2.5 py-1.5 opacity-100": isSearchOpen,
            }
          )}
        >
          <SearchOutline className="h-3.5 w-3.5" />
          <input
            ref={inputRef}
            className="w-full max-w-[234px] border-none bg-transparent text-13 text-primary placeholder:text-placeholder focus:outline-none"
            placeholder="Search"
            value={searchQuery}
            onChange={(e) => updateSearchQuery(e.target.value)}
            onKeyDown={handleInputKeyDown}
          />
          {isSearchOpen && (
            <button
              type="button"
              className="grid place-items-center"
              onClick={() => {
                // updateSearchQuery("");
                setIsSearchOpen(false);
              }}
            >
              <CloseOutline className="h-3 w-3" />
            </button>
          )}
        </div>
      </div>
      <ModuleOrderByDropdown
        value={displayFilters?.order_by}
        onChange={(val) => {
          if (!resolvedFilterKey || val === displayFilters?.order_by) return;
          updateDisplayFilters(resolvedFilterKey, {
            order_by: val,
          });
        }}
      />
      <FiltersDropdown
        icon={<FilterOutline className="h-3 w-3" />}
        title="Filters"
        placement="bottom-end"
        isFiltersApplied={isFiltersApplied}
      >
        <ModuleFiltersSelection
          displayFilters={displayFilters ?? {}}
          filters={filters ?? {}}
          handleDisplayFiltersUpdate={(val) => {
            if (!resolvedFilterKey) return;
            updateDisplayFilters(resolvedFilterKey, val);
          }}
          handleFiltersUpdate={handleFilters}
          memberIds={workspaceMemberIds ?? undefined}
        />
      </FiltersDropdown>
      <div className="hidden items-center gap-1 rounded-sm bg-layer-3 p-1 md:flex">
        {MODULE_VIEW_LAYOUTS.map((layout) => {
          const isLayoutDisabled = layout.key === "gantt" && isGanttDisabled;

          return (
            <Tooltip
              key={layout.key}
              label={isLayoutDisabled ? "Select an application to view timeline" : t(layout.i18n_title)}
              disabled={isMobile && !isLayoutDisabled}
            >
              <button
                type="button"
                aria-disabled={isLayoutDisabled}
                className={cn(
                  "group grid h-5.5 w-7 place-items-center overflow-hidden rounded-sm transition-all hover:bg-layer-transparent-hover",
                  {
                    "bg-layer-transparent-active hover:bg-layer-transparent-active":
                      displayFilters?.layout === layout.key,
                    "cursor-not-allowed opacity-40 hover:bg-transparent": isLayoutDisabled,
                  }
                )}
                onClick={() => {
                  if (isLayoutDisabled || !resolvedFilterKey) return;
                  updateDisplayFilters(resolvedFilterKey, { layout: layout.key });
                }}
              >
                <ModuleLayoutIcon layoutType={layout.key} />
              </button>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
});
