/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import { observer } from "mobx-react";
import { Combobox } from "@headlessui/react";
import { usePopper } from "@plane/hooks";
// plane imports
import { useTranslation } from "@plane/i18n";
import { ChevronDownOutline, TickOutline } from "@makeplane/propel/icons";
// ui
import { ComboDropDown } from "@plane/ui";
// helpers
import { cn } from "@plane/utils";
// components
import { DropdownButton } from "@/components/dropdowns/buttons";
import { BUTTON_VARIANTS_WITH_TEXT } from "@/components/dropdowns/constants";
import type { TDropdownProps } from "@/components/dropdowns/types";
// hooks
import { useWorkflow } from "@/hooks/store/use-workflow";
import { useDropdown } from "@/hooks/use-dropdown";

export type TWorkItemTypeDropdownProps = TDropdownProps & {
  dropdownArrow?: boolean;
  dropdownArrowClassName?: string;
  onChange: (typeId: string) => void;
  onClose?: () => void;
  projectId: string | undefined;
  value: string | null | undefined;
};

export const WorkItemTypeDropdown = observer(function WorkItemTypeDropdown(props: TWorkItemTypeDropdownProps) {
  const {
    buttonClassName,
    buttonContainerClassName,
    buttonVariant,
    className = "",
    disabled = false,
    dropdownArrow = false,
    dropdownArrowClassName = "",
    onChange,
    onClose,
    placement,
    projectId,
    showTooltip = false,
    tabIndex,
    value,
  } = props;
  // states
  const [isOpen, setIsOpen] = useState(false);
  // refs
  const dropdownRef = useRef<HTMLDivElement | null>(null);
  // popper-js refs
  const [referenceElement, setReferenceElement] = useState<HTMLButtonElement | null>(null);
  const [popperElement, setPopperElement] = useState<HTMLElement | null>(null);
  // store hooks
  const { t } = useTranslation();
  const { getWorkflowMap } = useWorkflow();
  // derived values: only enabled types with a workflow show up in the project workflow map
  const mapTypes = projectId ? (getWorkflowMap(projectId)?.types ?? []) : [];
  const selectedType = mapTypes.find((type) => type.type_id === value);
  // popper-js init
  const { styles, attributes } = usePopper(referenceElement, popperElement, {
    placement: placement ?? "bottom-start",
    strategy: "fixed",
    modifiers: [
      {
        name: "preventOverflow",
        options: {
          padding: 12,
        },
      },
    ],
  });
  // dropdown init
  const { handleClose, handleKeyDown, handleOnClick } = useDropdown({
    dropdownRef,
    isOpen,
    onClose,
    setIsOpen,
  });

  const dropdownOnChange = (typeId: string) => {
    onChange(typeId);
    handleClose();
  };

  // projects without typed workflows keep the legacy form, which has no type selector
  if (mapTypes.length === 0) return null;

  const comboButton = (
    <button
      ref={setReferenceElement}
      type="button"
      className={cn(
        "clickable block h-full max-w-full outline-none",
        {
          "cursor-not-allowed text-secondary": disabled,
          "cursor-pointer": !disabled,
        },
        buttonContainerClassName
      )}
      onClick={handleOnClick}
      disabled={disabled}
      tabIndex={tabIndex}
    >
      <DropdownButton
        className={buttonClassName}
        isActive={isOpen}
        tooltipHeading={t("type")}
        tooltipContent={selectedType?.type_name}
        showTooltip={showTooltip}
        variant={buttonVariant}
      >
        {BUTTON_VARIANTS_WITH_TEXT.includes(buttonVariant) && (
          <span className={cn("flex-grow truncate text-left", { "text-placeholder": !selectedType })}>
            {selectedType?.type_name ?? t("type")}
          </span>
        )}
        {dropdownArrow && (
          <ChevronDownOutline className={cn("h-2.5 w-2.5 flex-shrink-0", dropdownArrowClassName)} aria-hidden="true" />
        )}
      </DropdownButton>
    </button>
  );

  return (
    // oxlint-disable-next-line jsx_a11y/no-static-element-interactions
    <ComboDropDown
      as="div"
      ref={dropdownRef}
      className={cn("h-full", className)}
      value={value ?? null}
      onChange={dropdownOnChange}
      disabled={disabled}
      onKeyDown={handleKeyDown}
      button={comboButton}
    >
      {isOpen &&
        createPortal(
          <Combobox.Options
            as="ul"
            className="z-30"
            data-prevent-outside-click
            static
            ref={setPopperElement}
            style={styles.popper}
            {...attributes.popper}
            modal={false}
          >
            <div className="my-1 w-48 rounded-sm border-[0.5px] border-strong bg-surface-1 px-2 py-2.5 text-11 shadow-raised-200 focus:outline-none">
              <div className="max-h-48 space-y-1 overflow-y-scroll">
                {mapTypes.map((type) => (
                  <Combobox.Option
                    as="li"
                    key={type.type_id}
                    value={type.type_id}
                    className={({ active, selected }) =>
                      cn(
                        "flex w-full cursor-pointer items-center justify-between gap-2 truncate rounded-sm px-1 py-1.5 select-none",
                        {
                          "bg-layer-transparent-hover": active,
                          "text-primary": selected,
                          "text-secondary": !selected,
                        }
                      )
                    }
                  >
                    {({ selected }) => (
                      <>
                        <span className="flex-grow truncate">{type.type_name}</span>
                        {selected && <TickOutline className="h-3.5 w-3.5 flex-shrink-0" />}
                      </>
                    )}
                  </Combobox.Option>
                ))}
              </div>
            </div>
          </Combobox.Options>,
          document.body
        )}
    </ComboDropDown>
  );
});
