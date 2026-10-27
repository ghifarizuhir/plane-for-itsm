/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { createContext, Fragment, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { observer } from "mobx-react";
import { useParams, useRouter } from "next/navigation";
import { Dialog, Transition } from "@headlessui/react";
import { EUserPermissions, EUserPermissionsLevel } from "@plane/constants";
import { useInstance } from "@/hooks/store/use-instance";
import { useUserPermissions } from "@/hooks/store/user";
import { canUseGalileo } from "@/lib/galileo-routing";
import { GalileoHistorySidebar } from "./history-sidebar";

type TGallileoMobileHistoryContext = {
  openHistory: () => void;
};

const GalileoMobileHistoryContext = createContext<TGallileoMobileHistoryContext | undefined>(undefined);

export const useGalileoMobileHistory = (): TGallileoMobileHistoryContext => {
  const context = useContext(GalileoMobileHistoryContext);
  return context ?? { openHistory: () => {} };
};

export const GalileoLayout = observer(function GalileoLayout({ children }: { children: ReactNode }) {
  // router
  const { workspaceSlug } = useParams();
  const slug = workspaceSlug?.toString() ?? "";
  const router = useRouter();
  // store hooks
  const { config, isLoading } = useInstance();
  const { allowPermissions, getWorkspaceRoleByWorkspaceSlug } = useUserPermissions();
  // local state
  const [historyOpen, setHistoryOpen] = useState(false);

  const role = getWorkspaceRoleByWorkspaceSlug(slug);
  const roleLoaded = role !== undefined;
  const canUseAi = allowPermissions(
    [EUserPermissions.ADMIN, EUserPermissions.MEMBER],
    EUserPermissionsLevel.WORKSPACE,
    slug
  );
  const allowed = canUseGalileo(!!config?.has_llm_configured, canUseAi);

  useEffect(() => {
    if (isLoading || !roleLoaded) return;
    if (!allowed) router.replace(`/${slug}`);
  }, [allowed, isLoading, roleLoaded, router, slug]);

  const contextValue = useMemo(() => ({ openHistory: () => setHistoryOpen(true) }), []);

  if (isLoading || !roleLoaded || !allowed) return null;

  return (
    <GalileoMobileHistoryContext.Provider value={contextValue}>
      <div className="relative flex size-full overflow-hidden rounded-lg border border-subtle bg-surface-1">
        <GalileoHistorySidebar className="hidden md:flex" />
        <div className="flex min-w-0 flex-1 flex-col overflow-hidden">{children}</div>
      </div>

      <Transition.Root show={historyOpen} as={Fragment}>
        <Dialog as="div" className="relative z-50 md:hidden" onClose={() => setHistoryOpen(false)}>
          <Transition.Child
            as={Fragment}
            enter="ease-out duration-200"
            enterFrom="opacity-0"
            enterTo="opacity-100"
            leave="ease-in duration-150"
            leaveFrom="opacity-100"
            leaveTo="opacity-0"
          >
            <div className="fixed inset-0 bg-backdrop" />
          </Transition.Child>
          <Transition.Child
            as={Fragment}
            enter="ease-out duration-200"
            enterFrom="-translate-x-full"
            enterTo="translate-x-0"
            leave="ease-in duration-150"
            leaveFrom="translate-x-0"
            leaveTo="-translate-x-full"
          >
            <Dialog.Panel className="fixed inset-y-0 left-0 flex w-[17.5rem] max-w-[85vw] flex-col bg-surface-1 shadow-raised-200">
              <GalileoHistorySidebar className="flex" onNavigate={() => setHistoryOpen(false)} />
            </Dialog.Panel>
          </Transition.Child>
        </Dialog>
      </Transition.Root>
    </GalileoMobileHistoryContext.Provider>
  );
});
