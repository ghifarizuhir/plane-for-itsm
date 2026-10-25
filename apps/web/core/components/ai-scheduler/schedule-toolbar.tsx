/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { PreferencesOutline, RefreshOutline, SearchOutline } from "@makeplane/propel/icons";
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { cn, calculateTimeAgo, renderFormattedDate, renderFormattedTime } from "@plane/utils";
import { FilterOption, FiltersDropdown } from "@/components/issues/issue-layouts/filters";
import type {
  TAiScheduleDisplayProperties,
  TAiScheduleGroupBy,
  TAiScheduleListFilter,
  TAiScheduleOrderBy,
} from "@/lib/ai-schedule";
import { DEFAULT_SCHEDULE_DISPLAY_PROPERTIES } from "@/lib/ai-schedule";

type TStatus = TAiScheduleListFilter["status"];

const STATUS_FILTERS: { key: TStatus; label: string }[] = [
  { key: "all", label: "All" },
  { key: "active", label: "Active" },
  { key: "paused", label: "Paused" },
];

const GROUP_OPTIONS: { key: TAiScheduleGroupBy; label: string }[] = [
  { key: "status", label: "Status" },
  { key: "frequency", label: "Cadence" },
  { key: "none", label: "None" },
];

const ORDER_OPTIONS: { key: TAiScheduleOrderBy; label: string }[] = [
  { key: "next_run", label: "Next run" },
  { key: "name", label: "Name" },
  { key: "created", label: "Recently created" },
];

const DISPLAY_OPTIONS: { key: keyof TAiScheduleDisplayProperties; label: string }[] = [
  { key: "cadence", label: "Cadence" },
  { key: "nextRun", label: "Next run" },
  { key: "lastRun", label: "Last run" },
  { key: "creator", label: "Creator" },
];

type Props = {
  query: string;
  onQueryChange: (query: string) => void;
  status: TStatus;
  onStatusChange: (status: TStatus) => void;
  counts: Record<TStatus, number>;
  groupBy: TAiScheduleGroupBy;
  onGroupByChange: (groupBy: TAiScheduleGroupBy) => void;
  orderBy: TAiScheduleOrderBy;
  onOrderByChange: (orderBy: TAiScheduleOrderBy) => void;
  displayProperties: TAiScheduleDisplayProperties;
  onDisplayPropertiesChange: (properties: TAiScheduleDisplayProperties) => void;
  resultCount: number;
  totalCount: number;
  refreshing: boolean;
  lastUpdatedAt: Date | null;
  onRefresh: () => void;
};

export function ScheduleToolbar(props: Props) {
  const {
    query,
    onQueryChange,
    status,
    onStatusChange,
    counts,
    groupBy,
    onGroupByChange,
    orderBy,
    onOrderByChange,
    displayProperties,
    onDisplayPropertiesChange,
    resultCount,
    totalCount,
    refreshing,
    lastUpdatedAt,
    onRefresh,
  } = props;

  const toggleDisplayProperty = (key: keyof TAiScheduleDisplayProperties) =>
    onDisplayPropertiesChange({ ...displayProperties, [key]: !displayProperties[key] });
  const displayFiltersApplied =
    groupBy !== "status" ||
    orderBy !== "next_run" ||
    DISPLAY_OPTIONS.some((option) => displayProperties[option.key] !== DEFAULT_SCHEDULE_DISPLAY_PROPERTIES[option.key]);

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-subtle bg-surface-1 px-4 py-2">
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
        <div className="flex h-8 w-full max-w-sm min-w-[12rem] items-center gap-2 rounded-md border border-subtle bg-layer-1 px-2.5">
          <SearchOutline className="size-3.5 flex-shrink-0 text-placeholder" />
          <input
            className="w-full border-none bg-transparent text-12 text-primary outline-none placeholder:text-placeholder"
            placeholder="Search schedules..."
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            aria-label="Search schedules"
          />
        </div>
        <div
          role="group"
          aria-label="Filter schedules by status"
          className="flex items-center gap-0.5 rounded-md border border-subtle bg-layer-1 p-0.5"
        >
          {STATUS_FILTERS.map((filter) => (
            <button
              key={filter.key}
              type="button"
              aria-pressed={status === filter.key}
              onClick={() => onStatusChange(filter.key)}
              className={cn(
                "flex items-center gap-1.5 rounded px-2 py-1 text-11 font-medium transition-colors",
                status === filter.key
                  ? "bg-layer-2 text-primary shadow-raised-100"
                  : "text-secondary hover:text-primary"
              )}
            >
              {filter.label}
              <span className={cn("text-10", status === filter.key ? "text-tertiary" : "text-placeholder")}>
                {counts[filter.key]}
              </span>
            </button>
          ))}
        </div>
        {query.trim().length > 0 && (
          <span className="text-11 text-tertiary">
            {resultCount} of {totalCount}
          </span>
        )}
      </div>
      <div className="flex items-center gap-1.5">
        <FiltersDropdown
          title="Display"
          placement="bottom-end"
          icon={<PreferencesOutline className="size-4" />}
          miniIcon={<PreferencesOutline className="size-4" />}
          isFiltersApplied={displayFiltersApplied}
          menuButton={
            <div className="relative flex size-8 items-center justify-center rounded-md border border-subtle bg-layer-1 text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary">
              <PreferencesOutline className="size-4" />
              <span className="sr-only">Display options</span>
              {displayFiltersApplied && (
                <span className="absolute -top-0.5 -right-0.5 size-1.5 rounded-full bg-accent-primary" />
              )}
            </div>
          }
        >
          <div className="flex-1 overflow-y-auto px-2 py-1.5">
            <p className="px-1.5 pt-1 pb-1 text-caption-sm-medium text-placeholder">Group by</p>
            {GROUP_OPTIONS.map((option) => (
              <FilterOption
                key={option.key}
                title={option.label}
                isChecked={groupBy === option.key}
                multiple={false}
                onClick={() => onGroupByChange(option.key)}
              />
            ))}
            <p className="px-1.5 pt-3 pb-1 text-caption-sm-medium text-placeholder">Order by</p>
            {ORDER_OPTIONS.map((option) => (
              <FilterOption
                key={option.key}
                title={option.label}
                isChecked={orderBy === option.key}
                multiple={false}
                onClick={() => onOrderByChange(option.key)}
              />
            ))}
            <p className="px-1.5 pt-3 pb-1 text-caption-sm-medium text-placeholder">Properties</p>
            {DISPLAY_OPTIONS.map((option) => (
              <FilterOption
                key={option.key}
                title={option.label}
                isChecked={displayProperties[option.key]}
                onClick={() => toggleDisplayProperty(option.key)}
              />
            ))}
          </div>
        </FiltersDropdown>
        <Tooltip
          label={lastUpdatedAt ? `Updated ${calculateTimeAgo(lastUpdatedAt)}` : "Refresh schedules"}
          side="bottom"
        >
          <button
            type="button"
            aria-label="Refresh schedules"
            onClick={onRefresh}
            className="grid size-8 place-items-center rounded-md border border-subtle bg-layer-1 text-secondary transition-colors hover:bg-layer-1-hover hover:text-primary"
          >
            <RefreshOutline className={cn("size-4", refreshing && "animate-spin")} />
          </button>
        </Tooltip>
        {lastUpdatedAt && (
          <Tooltip
            label={`${renderFormattedDate(lastUpdatedAt)} at ${renderFormattedTime(lastUpdatedAt)}`}
            side="bottom"
          >
            <span className="hidden text-11 text-placeholder xl:inline">Updated {calculateTimeAgo(lastUpdatedAt)}</span>
          </Tooltip>
        )}
      </div>
    </div>
  );
}
