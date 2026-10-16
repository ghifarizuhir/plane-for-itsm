/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ReactNode } from "react";
import { Disclosure } from "@headlessui/react";
import { useParams } from "next/navigation";
import { ChevronDownOutline } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";
// local imports
import { DocsSidebar } from "./docs-sidebar";
import { DEFAULT_DOC_TOPIC_SLUG, getTopicBySlug } from "./registry";

function DocsMobileNav() {
  const { topicSlug } = useParams();
  const activeTopic = getTopicBySlug(topicSlug?.toString() ?? DEFAULT_DOC_TOPIC_SLUG);
  const ActiveIcon = activeTopic?.icon;

  return (
    <Disclosure as="div" className="relative z-40 border-b border-subtle bg-surface-1 md:hidden">
      {({ open, close }) => (
        <>
          <Disclosure.Button as="button" type="button" className="flex w-full items-center justify-between px-4 py-3">
            <span className="flex items-center gap-2 text-13 font-medium text-secondary">
              {ActiveIcon && <ActiveIcon className="size-4 flex-shrink-0 text-tertiary" />}
              {activeTopic?.title ?? "Docs"}
            </span>
            <ChevronDownOutline
              className={cn("size-4 flex-shrink-0 text-tertiary transition-transform", open && "rotate-180")}
            />
          </Disclosure.Button>
          {open && (
            <Disclosure.Panel
              static
              className="shadow-lg absolute inset-x-0 top-full max-h-[70vh] overflow-y-auto border-b border-subtle bg-surface-1"
            >
              <DocsSidebar isMobile onNavigate={close} />
            </Disclosure.Panel>
          )}
        </>
      )}
    </Disclosure>
  );
}

export function DocsLayout({ children }: { children: ReactNode }) {
  return (
    <div className="flex size-full flex-col overflow-hidden bg-surface-1">
      <DocsMobileNav />
      <div className="relative flex size-full overflow-hidden">
        <DocsSidebar />
        <div className="size-full overflow-hidden">{children}</div>
      </div>
    </div>
  );
}
