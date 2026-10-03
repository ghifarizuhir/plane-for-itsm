/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useRef, useState } from "react";
import { observer } from "mobx-react";
import { createPortal } from "react-dom";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TNameDescriptionLoader } from "@plane/types";
import { cn } from "@plane/utils";
// hooks
import { useService } from "@/hooks/store/use-service";
import useKeypress from "@/hooks/use-keypress";
import usePeekOverviewOutsideClickDetector from "@/hooks/use-peek-overview-outside-click";
// local imports
import { ServiceDescription } from "../detail/description";
import { ServiceDetailSidebar } from "../detail/sidebar";
import { ServiceTitleInput } from "../detail/title-input";
import { ServiceWorkItems } from "../detail/work-items";
import { ServiceLoadErrorState } from "../service-load-error-state";
import { ServicePeekHeader, type TServicePeekModes } from "./header";

type Props = {
  workspaceSlug: string;
  projectId: string;
  serviceId: string;
  status: "loading" | "error" | "ready";
  onRetry: () => void;
};

export const ServicePeekView = observer(function ServicePeekView(props: Props) {
  const { workspaceSlug, projectId, serviceId, status, onRetry } = props;
  // states
  const [peekMode, setPeekMode] = useState<TServicePeekModes>("side-peek");
  const [isSubmitting, setIsSubmitting] = useState<TNameDescriptionLoader>("saved");
  // refs
  const peekRef = useRef<HTMLDivElement>(null);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, setPeekService } = useService();
  // derived values
  const service = getServiceById(serviceId);
  const serviceLink = `/${workspaceSlug}/projects/${projectId}/services/${serviceId}`;

  const closePeek = () => {
    setPeekService(undefined);
    document.getElementById(`service-${serviceId}`)?.focus();
  };

  usePeekOverviewOutsideClickDetector(
    peekRef,
    () => {
      if (document.querySelector('[role="dialog"]')) return;
      closePeek();
    },
    serviceId,
    ["main-sidebar"],
    `service-${serviceId}`
  );

  useKeypress("Escape", () => {
    if (document.querySelector('[role="dialog"]')) return;
    const activeElement = document.activeElement;
    if (
      activeElement instanceof HTMLElement &&
      (activeElement.tagName === "INPUT" || activeElement.tagName === "TEXTAREA" || activeElement.isContentEditable)
    )
      return;
    closePeek();
  });

  const renderBody = () => {
    if (status === "error") return <ServiceLoadErrorState onRetry={onRetry} />;
    if (status === "loading") return <div className="text-sm p-6 text-secondary">{t("common.loading")}</div>;
    if (!service)
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
          <p className="text-sm font-medium text-primary">{t("service.detail.not_found_title")}</p>
          <p className="text-xs text-secondary">{t("service.detail.not_found_description")}</p>
        </div>
      );

    const mainContent = (
      <>
        <ServiceTitleInput serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
        <ServiceDescription serviceId={serviceId} isSubmitting={isSubmitting} setIsSubmitting={setIsSubmitting} />
        <ServiceWorkItems serviceId={serviceId} />
      </>
    );

    if (peekMode === "full-screen")
      return (
        <div className="flex h-full w-full flex-col overflow-hidden md:flex-row">
          <div className="w-full space-y-6 overflow-y-auto p-4 py-5 md:h-full md:min-w-0 md:flex-1 md:px-8">
            {mainContent}
          </div>
          <div className="w-full shrink-0 border-t border-subtle bg-surface-1 md:h-full md:!w-[400px] md:border-t-0 md:border-l">
            <ServiceDetailSidebar serviceId={serviceId} />
          </div>
        </div>
      );

    return (
      <div className="relative flex flex-col gap-6 px-4 py-5 md:px-8">
        {mainContent}
        <div className="border-t border-subtle pt-4">
          <ServiceDetailSidebar serviceId={serviceId} layout="stacked" />
        </div>
      </div>
    );
  };

  const content = (
    <div
      ref={peekRef}
      className={cn(
        "absolute z-[25] flex flex-col overflow-hidden rounded-sm border border-subtle bg-surface-1 transition-all duration-300",
        {
          "top-0 right-0 bottom-0 w-full border-0 border-l md:w-[50%]": peekMode === "side-peek",
          "top-[8.33%] left-[8.33%] size-5/6": peekMode === "modal",
          "absolute inset-0 m-4": peekMode === "full-screen",
        }
      )}
      style={{
        boxShadow:
          "0px 4px 8px 0px rgba(0, 0, 0, 0.12), 0px 6px 12px 0px rgba(16, 24, 40, 0.12), 0px 1px 16px 0px rgba(16, 24, 40, 0.12)",
      }}
    >
      <ServicePeekHeader
        peekMode={peekMode}
        setPeekMode={setPeekMode}
        closePeek={closePeek}
        serviceLink={serviceLink}
        service={service ?? undefined}
        isSubmitting={isSubmitting}
      />
      <div className="vertical-scrollbar relative h-full w-full overflow-hidden overflow-y-auto">{renderBody()}</div>
    </div>
  );

  const portalContainer = document.getElementById("full-screen-portal");

  return <>{portalContainer ? createPortal(content, portalContainer) : content}</>;
});
