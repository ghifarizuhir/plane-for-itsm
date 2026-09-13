/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import Link from "next/link";
import { useParams } from "next/navigation";
// icons
import { DocumentationOutline, LinkOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Card } from "@plane/ui";
// components
import { ButtonAvatars } from "@/components/dropdowns/member/avatar";
// hooks
import { useService } from "@/hooks/store/use-service";

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

export const ServiceCardItem = observer(function ServiceCardItem(props: Props) {
  const { serviceId } = props;
  // router
  const { workspaceSlug } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById } = useService();

  // derived values
  const serviceDetails = getServiceById(serviceId);

  if (!serviceDetails) return null;

  const serviceLink = `/${workspaceSlug?.toString()}/projects/${serviceDetails.project_id}/services/${serviceDetails.id}`;

  return (
    <Card>
      <div className="flex items-center justify-between gap-2">
        <Link href={serviceLink} className="flex min-w-0 items-center gap-2">
          <span
            aria-hidden="true"
            className={`h-2.5 w-2.5 flex-shrink-0 rounded-full ${STATUS_DOT_CLASS[serviceDetails.status] ?? "bg-gray-400"}`}
          />
          <span className="truncate text-14 font-medium">{serviceDetails.name}</span>
        </Link>
        {serviceDetails.owner_id && (
          <span className="cursor-default">
            <ButtonAvatars showTooltip={false} userIds={serviceDetails.owner_id} />
          </span>
        )}
      </div>
      <Link href={serviceLink} className="flex min-w-0 flex-col gap-3">
        {serviceDetails.description && (
          <p className="line-clamp-2 text-12 text-secondary">{serviceDetails.description}</p>
        )}
        <div className="flex flex-wrap items-center gap-1.5">
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
      </Link>
      {(serviceDetails.repository_url || serviceDetails.documentation_url) && (
        <div className="flex items-center gap-3">
          {serviceDetails.repository_url && (
            <a
              href={serviceDetails.repository_url}
              target="_blank"
              rel="noopener noreferrer"
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
              rel="noopener noreferrer"
              onClick={(e) => e.stopPropagation()}
              className="flex items-center gap-1 text-11 text-tertiary hover:text-primary"
            >
              <DocumentationOutline className="h-3.5 w-3.5" />
              {t("service.fields.documentation")}
            </a>
          )}
        </div>
      )}
    </Card>
  );
});
