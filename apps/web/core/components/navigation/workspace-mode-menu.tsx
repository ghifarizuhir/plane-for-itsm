/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { Fragment } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
import { Menu, Transition } from "@headlessui/react";
import { AiStar1Outline, ChevronDownOutline, DocumentationOutline, WorkItemsOutline } from "@makeplane/propel/icons";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { cn } from "@plane/utils";
import { useInstance } from "@/hooks/store/use-instance";
import { useUserPermissions } from "@/hooks/store/user";
import { useWorkspacePaths } from "@/hooks/use-workspace-paths";
import { canUseGalileo } from "@/lib/galileo-routing";

export const WorkspaceModeMenu = observer(function WorkspaceModeMenu() {
  // router
  const { workspaceSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const router = useRouter();
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { config } = useInstance();
  const { allowPermissions } = useUserPermissions();
  const { isDocsPath, isGalileoPath, isWorkPath } = useWorkspacePaths();
  // derived values
  const canUseAi = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.WORKSPACE,
    slug
  );
  const showGalileo = canUseGalileo(!!config?.has_llm_configured, canUseAi);

  if (!slug) return null;

  const items = [
    {
      key: "work",
      label: t("sidebar.work"),
      icon: WorkItemsOutline,
      href: `/${slug}`,
      isActive: isWorkPath,
    },
    {
      key: "procedure",
      label: t("sidebar.procedure"),
      icon: DocumentationOutline,
      href: `/${slug}/docs`,
      isActive: isDocsPath,
    },
    ...(showGalileo
      ? [
          {
            key: "galileo",
            label: t("sidebar.galileo"),
            icon: AiStar1Outline,
            href: `/${slug}/galileo`,
            isActive: isGalileoPath,
          },
        ]
      : []),
  ];

  const activeItem = items.find((item) => item.isActive) ?? items[0];
  if (!activeItem) return null;
  const ActiveIcon = activeItem.icon;

  return (
    <Menu as="div" className="relative md:hidden">
      <Menu.Button
        type="button"
        aria-label={`${t("assistant.galileo.switch_mode")}: ${activeItem.label}`}
        title={activeItem.label}
        className="flex size-8 items-center justify-center gap-0.5 rounded-md text-secondary transition-colors hover:bg-layer-1-hover focus:outline-none"
      >
        <ActiveIcon className="size-4.5" />
        <ChevronDownOutline className="size-3 text-tertiary" />
      </Menu.Button>
      <Transition
        as={Fragment}
        enter="transition ease-out duration-100"
        enterFrom="transform opacity-0 scale-95"
        enterTo="transform opacity-100 scale-100"
        leave="transition ease-in duration-75"
        leaveFrom="transform opacity-100 scale-100"
        leaveTo="transform opacity-0 scale-95"
      >
        <Menu.Items className="absolute right-0 z-30 mt-1 w-44 origin-top-right rounded-md border border-subtle bg-surface-1 p-1 shadow-raised-200 focus:outline-none">
          {items.map((item) => {
            const Icon = item.icon;
            return (
              <Menu.Item key={item.key}>
                {({ active }) => (
                  <button
                    type="button"
                    onClick={() => router.push(item.href)}
                    className={cn("flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-12", {
                      "bg-layer-1 text-primary": item.isActive,
                      "text-secondary hover:bg-layer-1-hover": !item.isActive,
                      "bg-layer-1-hover": active && !item.isActive,
                    })}
                  >
                    <Icon className="size-4 flex-shrink-0" />
                    <span className="truncate">{item.label}</span>
                  </button>
                )}
              </Menu.Item>
            );
          })}
        </Menu.Items>
      </Transition>
    </Menu>
  );
});
