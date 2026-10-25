/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React, { Fragment, useState } from "react";
import type { Placement } from "@popperjs/core";
import { usePopper } from "@plane/hooks";
// headless ui
import { Popover, Transition } from "@headlessui/react";
// ui
import { Button } from "@plane/propel/button";
import { cn } from "@plane/utils";

const PANEL_ORIGIN: Record<Placement, string> = {
  auto: "origin-top",
  "auto-start": "origin-top-left",
  "auto-end": "origin-top-right",
  top: "origin-bottom",
  "top-start": "origin-bottom-left",
  "top-end": "origin-bottom-right",
  bottom: "origin-top",
  "bottom-start": "origin-top-left",
  "bottom-end": "origin-top-right",
  left: "origin-right",
  "left-start": "origin-top-right",
  "left-end": "origin-bottom-right",
  right: "origin-left",
  "right-start": "origin-top-left",
  "right-end": "origin-bottom-left",
};

type Props = {
  children: React.ReactNode;
  icon?: React.ReactElement;
  miniIcon?: React.ReactNode;
  title?: string;
  placement?: Placement;
  disabled?: boolean;
  tabIndex?: number;
  menuButton?: React.ReactNode;
  isFiltersApplied?: boolean;
};

export function FiltersDropdown(props: Props) {
  const {
    children,
    miniIcon,
    icon,
    title = "Dropdown",
    placement,
    disabled = false,
    tabIndex,
    menuButton,
    isFiltersApplied = false,
  } = props;

  const [referenceElement, setReferenceElement] = useState<HTMLButtonElement | HTMLDivElement | null>(null);
  const [popperElement, setPopperElement] = useState<HTMLDivElement | null>(null);

  const { styles, attributes } = usePopper(referenceElement, popperElement, {
    placement: placement ?? "auto",
    strategy: "fixed",
  });

  return (
    <Popover as="div">
      {({ open }) => (
        <>
          <Popover.Button as={React.Fragment}>
            {menuButton ? (
              <button type="button" ref={setReferenceElement}>
                {menuButton}
              </button>
            ) : (
              <div ref={setReferenceElement}>
                <div className="hidden @4xl:flex">
                  <Button
                    disabled={disabled}
                    variant="secondary"
                    prependIcon={icon}
                    tabIndex={tabIndex}
                    className="relative"
                    size="lg"
                  >
                    <>
                      <div className={`${open ? "text-primary" : "text-secondary"}`}>
                        <span>{title}</span>
                      </div>
                      {isFiltersApplied && (
                        <span className="absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full bg-accent-primary" />
                      )}
                    </>
                  </Button>
                </div>
                <div className="flex @4xl:hidden">
                  <Button
                    disabled={disabled}
                    ref={setReferenceElement}
                    variant="secondary"
                    tabIndex={tabIndex}
                    size="lg"
                  >
                    {miniIcon || title}
                  </Button>
                </div>
              </div>
            )}
          </Popover.Button>
          <Transition as={Fragment}>
            <Popover.Panel portal className="z-30">
              <div ref={setPopperElement} style={styles.popper} {...attributes.popper}>
                <Transition.Child
                  as={Fragment}
                  enter="transition motion-micro"
                  enterFrom="opacity-0 translate-y-1 scale-[0.98]"
                  enterTo="opacity-100 translate-y-0 scale-100"
                  leave="transition motion-micro"
                  leaveFrom="opacity-100 translate-y-0 scale-100"
                  leaveTo="opacity-0 translate-y-1 scale-[0.98]"
                >
                  <div
                    className={cn(
                      "z-30 my-1 overflow-hidden rounded-sm border border-subtle bg-surface-1 shadow-raised-100",
                      PANEL_ORIGIN[placement ?? "auto"]
                    )}
                  >
                    <div className="flex max-h-[30rem] w-[calc(100vw-2rem)] flex-col overflow-hidden md:w-[18.75rem] lg:max-h-[37.5rem]">
                      {children}
                    </div>
                  </div>
                </Transition.Child>
              </div>
            </Popover.Panel>
          </Transition>
        </>
      )}
    </Popover>
  );
}
