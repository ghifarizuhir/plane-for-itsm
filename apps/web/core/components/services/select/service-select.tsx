/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { usePopper } from "react-popper";
// plane imports
import { useTranslation } from "@plane/i18n";
import { ChevronDownOutline, CloseOutline, SearchOutline } from "@makeplane/propel/icons";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { ComboDropDown } from "@plane/ui";
import { cn } from "@plane/utils";
// hooks
import { useDropdown } from "@/hooks/use-dropdown";
import { useIssueDetail } from "@/hooks/store/use-issue-detail";
import { useProject } from "@/hooks/store/use-project";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

type TServiceSelect = {
  className?: string;
  workspaceSlug: string;
  projectId: string;
  issueId: string;
  disabled?: boolean;
};

export const ServiceSelect = observer(function ServiceSelect(props: TServiceSelect) {
  const { className = "", workspaceSlug, projectId, issueId, disabled = false } = props;
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
  const {
    issue: { getIssueById },
  } = useIssueDetail();
  const { getProjectById } = useProject();
  const {
    getServiceById,
    getProjectServiceIds,
    workItemLinkMap,
    fetchServices,
    linkWorkItem,
    unlinkWorkItem,
    fetchedMap,
    loader,
  } = useService();
  const { currentWorkspace, getWorkspaceBySlug } = useWorkspace();
  // derived values
  const issue = getIssueById(issueId);
  const workspaceId = getWorkspaceBySlug(workspaceSlug)?.id ?? currentWorkspace?.id;
  const serviceIds = getProjectServiceIds(projectId);
  const issueLinks = Object.values(workItemLinkMap).filter((l) => l.issue_id === issueId && l.project_id === projectId);
  const linkedServiceIds = issueLinks.map((l) => l.service_id);
  const linkedSet = new Set(linkedServiceIds);
  const linkedServices = linkedServiceIds
    .map((id) => getServiceById(id))
    .filter((s): s is NonNullable<typeof s> => Boolean(s));
  const projectIdentifier = issue ? getProjectById(issue.project_id)?.identifier : undefined;
  const issueIdentifier = projectIdentifier && issue ? `${projectIdentifier}-${issue.sequence_id}` : undefined;

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

  const { handleKeyDown, handleOnClick } = useDropdown({
    dropdownRef,
    inputRef,
    isOpen,
    onOpen: ensureServices,
    query,
    setIsOpen,
    setQuery,
  });

  // The issue detail never hydrates the service store, so fetch on mount for the chips.
  useEffect(() => {
    if (serviceIds !== null || fetchedMap[projectId] || loader || !workspaceId) return;
    fetchServices(workspaceSlug, workspaceId, projectId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId, projectId]);

  const handleToggle = async (serviceId: string) => {
    if (disabled || !issue || !workspaceId) return;
    const existing = Object.values(workItemLinkMap).find(
      (l) => l.service_id === serviceId && l.issue_id === issueId && l.project_id === projectId
    );
    try {
      if (existing) {
        await unlinkWorkItem(workspaceSlug, workspaceId, projectId, existing.id);
      } else {
        await linkWorkItem(workspaceSlug, workspaceId, projectId, serviceId, {
          id: issueId,
          identifier: issueIdentifier,
          name: issue.name,
        });
      }
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: t(existing ? "service.detail.unlink_work_item_error" : "service.detail.link_work_item_error"),
      });
    }
  };

  const filteredServiceIds = (serviceIds ?? []).filter((id) =>
    (getServiceById(id)?.name ?? "").toLowerCase().includes(query.toLowerCase())
  );
  const selectedFirst = filteredServiceIds.filter((id) => linkedSet.has(id));
  const unselected = filteredServiceIds.filter((id) => !linkedSet.has(id));
  const orderedServiceIds = [...selectedFirst, ...unselected];

  const comboButton = (
    <button
      ref={setReferenceElement}
      type="button"
      className={cn("clickable block h-full w-full max-w-full rounded-sm text-left outline-none hover:bg-layer-1", {
        "cursor-not-allowed text-secondary": disabled,
        "cursor-pointer": !disabled,
      })}
      onClick={handleOnClick}
      onKeyDown={handleKeyDown}
      disabled={disabled}
    >
      <div className="group flex h-full w-full items-center justify-between gap-1 px-2 py-0.5">
        {linkedServices.length > 0 ? (
          <div className="flex flex-wrap items-center gap-1 py-0.5">
            {linkedServices.map((service) => (
              <span
                key={service.id}
                title={service.name}
                className="group/chip flex max-w-30 items-center gap-1 rounded-sm bg-surface-2 px-1.5 py-0.5 text-caption-sm-medium text-primary"
              >
                <span className="truncate">{service.name}</span>
                {!disabled && (
                  // eslint-disable-next-line jsx_a11y/click-events-have-key-events oxlint-disable-next-line jsx_a11y/no-static-element-interactions
                  <span
                    onClick={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      handleToggle(service.id);
                    }}
                    className="flex shrink-0 items-center"
                  >
                    <CloseOutline className="h-2.5 w-2.5 text-tertiary hover:text-danger-primary" />
                  </span>
                )}
              </span>
            ))}
          </div>
        ) : (
          <span className="text-body-xs-regular text-placeholder">{t("service.detail.select_service")}</span>
        )}
        {!disabled && <ChevronDownOutline className="hidden h-3.5 w-3.5 shrink-0 group-hover:inline" />}
      </div>
    </button>
  );

  return (
    <div className={cn("flex h-full items-center gap-1", className)}>
      <ComboDropDown as="div" ref={dropdownRef} className="h-full w-full" button={comboButton} disabled={disabled}>
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
                ) : orderedServiceIds.length > 0 ? (
                  orderedServiceIds.map((serviceId) => {
                    const service = getServiceById(serviceId);
                    const checked = linkedSet.has(serviceId);
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
