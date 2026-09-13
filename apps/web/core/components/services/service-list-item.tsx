/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useRef } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// icons
import { DocumentationOutline, LinkOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
// components
import { ListItem } from "@/components/core/list";
import { ButtonAvatars } from "@/components/dropdowns/member/avatar";
// hooks
import { useService } from "@/hooks/store/use-service";
import { usePlatformOS } from "@/hooks/use-platform-os";

type Props = {
  serviceId: string;
};

const STATUS_DOT_CLASS: Record<string, string> = {
  active: "bg-green-500",
  planned: "bg-gray-400",
  maintenance: "bg-yellow-500",
  deprecated: "bg-orange-500",
  retired: "bg-gray-500",
};

const formatLabel = (value: string) => value.replace(/_/g, " ");

export const ServiceListItem = observer(function ServiceListItem(props: Props) {
  const { serviceId } = props;
  // refs
  const parentRef = useRef(null);
  // router
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById } = useService();
  const { isMobile } = usePlatformOS();

  // derived values
  const serviceDetails = getServiceById(serviceId);

  if (!serviceDetails) return null;

  return (
    <ListItem
      title={serviceDetails?.name ?? ""}
      itemLink={`/${workspaceSlug?.toString()}/projects/${serviceDetails.project_id}/services/${serviceDetails.id}`}
      prependTitleElement={
        <span
          className={`h-2.5 w-2.5 flex-shrink-0 rounded-full ${STATUS_DOT_CLASS[serviceDetails.status] ?? "bg-gray-400"}`}
        />
      }
      appendTitleElement={
        <div className="flex flex-shrink-0 items-center gap-1.5">
          <span className="rounded-sm border border-subtle px-1.5 py-0.5 text-11 text-secondary capitalize">
            {formatLabel(serviceDetails.status)}
          </span>
          <span className="rounded-sm border border-subtle px-1.5 py-0.5 text-11 text-secondary capitalize">
            {formatLabel(serviceDetails.criticality)}
          </span>
          <span className="rounded-sm border border-subtle px-1.5 py-0.5 text-11 text-tertiary capitalize">
            {formatLabel(serviceDetails.type)}
          </span>
        </div>
      }
      actionableItems={
        <>
          {serviceDetails.owner_id && <ButtonAvatars showTooltip userIds={serviceDetails.owner_id} />}
          {serviceDetails.repository_url && (
            <a
              href={serviceDetails.repository_url}
              target="_blank"
              rel="noreferrer"
              onClick={(e) => e.stopPropagation()}
              className="flex items-center gap-1 text-11 text-tertiary hover:text-primary"
            >
              <LinkOutline className="h-3.5 w-3.5" />
              {t("service.fields.repository")}
            </a>
          )}
          {serviceDetails.documentation_url && (
            <a
              href={serviceDetails.documentation_url}
              target="_blank"
              rel="noreferrer"
              onClick={(e) => e.stopPropagation()}
              className="flex items-center gap-1 text-11 text-tertiary hover:text-primary"
            >
              <DocumentationOutline className="h-3.5 w-3.5" />
              {t("service.fields.documentation")}
            </a>
          )}
        </>
      }
      isMobile={isMobile}
      parentRef={parentRef}
    />
  );
});
