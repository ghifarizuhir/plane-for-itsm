/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { usePopper } from "@plane/hooks";
// plane imports
import { useTranslation } from "@plane/i18n";
import { ChevronDownOutline, SearchOutline } from "@makeplane/propel/icons";
import { ComboDropDown } from "@plane/ui";
import { cn } from "@plane/utils";
// hooks
import { useDropdown } from "@/hooks/use-dropdown";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

type TServiceMultiSelect = {
  className?: string;
  workspaceSlug: string;
  projectId: string;
  value: string[];
  onChange: (serviceIds: string[]) => void;
  disabled?: boolean;
  tabIndex?: number;
};

export const ServiceMultiSelect = observer(function ServiceMultiSelect(props: TServiceMultiSelect) {
  const { className = "", workspaceSlug, projectId, value, onChange, disabled = false, tabIndex } = props;
  const { t } = useTranslation();
  // states
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState("");
  // refs
  const dropdownRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  // popper-js refs
  const [referenceElement, setReferenceElement] = useState<HTMLButtonElement | null>(null);
  const [popperElement, setPopperElement] = useState<HTMLDivElement | null>(null);
  // store hooks
  const { getServiceById, getProjectServiceIds, fetchServices, fetchedMap, loader } = useService();
  const { currentWorkspace, getWorkspaceBySlug } = useWorkspace();
  // derived values
  const workspaceId = getWorkspaceBySlug(workspaceSlug)?.id ?? currentWorkspace?.id;
  const serviceIds = getProjectServiceIds(projectId);
  const selectedSet = new Set(value);

  // popper-js init
  const { styles, attributes } = usePopper(referenceElement, popperElement, {
    placement: "bottom-start",
    modifiers: [
      {
        name: "preventOverflow",
        options: {
          padding: 12,
        },
      },
    ],
  });

  const ensureServices = () => {
    if (serviceIds === null && !fetchedMap[projectId] && !loader && workspaceId)
      fetchServices(workspaceSlug, workspaceId, projectId);
  };

  // the create modal never hydrates the service store, so fetch on mount
  useEffect(() => {
    if (serviceIds !== null || fetchedMap[projectId] || loader || !workspaceId) return;
    fetchServices(workspaceSlug, workspaceId, projectId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId, projectId]);

  const { handleKeyDown, handleOnClick } = useDropdown({
    dropdownRef,
    inputRef,
    isOpen,
    onOpen: ensureServices,
    query,
    setIsOpen,
    setQuery,
  });

  const handleToggle = (serviceId: string) => {
    if (disabled) return;
    const next = new Set(value);
    if (next.has(serviceId)) next.delete(serviceId);
    else next.add(serviceId);
    onChange([...next]);
  };

  const filteredServiceIds = (serviceIds ?? []).filter((id) =>
    (getServiceById(id)?.name ?? "").toLowerCase().includes(query.toLowerCase())
  );

  return (
    <div className={cn("h-full", className)}>
      <ComboDropDown
        as="div"
        ref={dropdownRef}
        className="h-full w-full"
        disabled={disabled}
        button={
          <button
            ref={setReferenceElement}
            type="button"
            tabIndex={tabIndex}
            className={cn(
              "flex h-full w-full cursor-pointer items-center justify-between gap-1 rounded-sm border-[0.5px] border-strong px-2 py-0.5 text-caption-sm-regular hover:bg-layer-1",
              {
                "cursor-not-allowed text-secondary": disabled,
              }
            )}
            onClick={handleOnClick}
            onKeyDown={handleKeyDown}
            disabled={disabled}
          >
            <span className={cn("truncate", { "text-placeholder": value.length === 0 })}>
              {value.length > 0 ? `${value.length} ${t("service.title")}` : t("service.detail.select_service")}
            </span>
            {!disabled && <ChevronDownOutline className="h-3 w-3 shrink-0" />}
          </button>
        }
      >
        {isOpen && (
          <ul className="fixed z-10">
            <div
              className="my-1 w-48 rounded-sm border-[0.5px] border-strong bg-surface-1 px-2 py-2.5 text-11 shadow-raised-200 focus:outline-none"
              ref={setPopperElement}
              style={styles.popper}
              {...attributes.popper}
            >
              <div className="flex items-center gap-1.5 rounded-sm border border-subtle bg-surface-2 px-2">
                <SearchOutline className="h-3.5 w-3.5 text-placeholder" />
                <input
                  ref={inputRef}
                  className="w-full bg-transparent py-1 text-11 text-secondary placeholder:text-placeholder focus:outline-none"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={t("common.search.label")}
                />
              </div>
              <div className="mt-2 max-h-48 space-y-1 overflow-y-scroll">
                {serviceIds === null ? (
                  <p className="px-1.5 py-1 text-placeholder italic">{t("common.loading")}</p>
                ) : filteredServiceIds.length > 0 ? (
                  filteredServiceIds.map((serviceId) => {
                    const service = getServiceById(serviceId);
                    const checked = selectedSet.has(serviceId);
                    return (
                      <li key={serviceId}>
                        <button
                          type="button"
                          onClick={() => handleToggle(serviceId)}
                          disabled={disabled}
                          className={cn(
                            "flex w-full cursor-pointer items-center gap-2 truncate rounded-sm px-1 py-1.5 select-none hover:bg-layer-transparent-hover",
                            {
                              "text-primary": checked,
                              "text-secondary": !checked,
                            }
                          )}
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            readOnly
                            tabIndex={-1}
                            className="accent-primary h-3.5 w-3.5 shrink-0"
                          />
                          <span className="flex-grow truncate" title={service?.name ?? serviceId}>
                            {service?.name ?? serviceId}
                          </span>
                        </button>
                      </li>
                    );
                  })
                ) : (
                  <p className="px-1.5 py-1 text-placeholder italic">{t("common.search.no_matching_results")}</p>
                )}
              </div>
            </div>
          </ul>
        )}
      </ComboDropDown>
    </div>
  );
});
