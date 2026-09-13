/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// icons
import { DocumentationOutline, LinkOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
// components
import { ButtonAvatars } from "@/components/dropdowns/member/avatar";
// hooks
import { useService } from "@/hooks/store/use-service";
// local imports
import { DeleteServiceModal } from "../delete-service-modal";
import { CreateUpdateServiceModal } from "../modal";

type Props = {
  serviceId: string;
};

export const ServiceDetailHeader = observer(function ServiceDetailHeader(props: Props) {
  const { serviceId } = props;
  // states
  const [isEditModalOpen, setIsEditModalOpen] = useState(false);
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false);
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById } = useService();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString() ?? service.project_id;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-2">
          <h2 className="text-xl font-semibold break-words text-primary">{service.name}</h2>
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="rounded-sm border border-subtle px-1.5 py-0.5 text-11 text-secondary capitalize">
              {t(`service.status_values.${service.status}`)}
            </span>
            <span className="rounded-sm border border-subtle px-1.5 py-0.5 text-11 text-secondary capitalize">
              {t(`service.criticality_values.${service.criticality}`)}
            </span>
            <span className="rounded-sm border border-subtle px-1.5 py-0.5 text-11 text-tertiary capitalize">
              {t(`service.type_values.${service.type}`)}
            </span>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button variant="secondary" size="sm" onClick={() => setIsEditModalOpen(true)}>
            {t("edit")}
          </Button>
          <Button variant="error-outline" size="sm" onClick={() => setIsDeleteModalOpen(true)}>
            {t("delete")}
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        {service.owner_id && (
          <span className="flex items-center gap-1.5 text-12 text-secondary">
            <span>{t("service.fields.owner")}</span>
            <ButtonAvatars showTooltip userIds={service.owner_id} />
          </span>
        )}
        {service.repository_url && (
          <a
            href={service.repository_url}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1 text-12 text-tertiary hover:text-primary"
          >
            <LinkOutline className="h-3.5 w-3.5" />
            {t("service.fields.repository")}
          </a>
        )}
        {service.documentation_url && (
          <a
            href={service.documentation_url}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1 text-12 text-tertiary hover:text-primary"
          >
            <DocumentationOutline className="h-3.5 w-3.5" />
            {t("service.fields.documentation")}
          </a>
        )}
      </div>
      {slug && (
        <CreateUpdateServiceModal
          isOpen={isEditModalOpen}
          onClose={() => setIsEditModalOpen(false)}
          data={service}
          workspaceSlug={slug}
          projectId={pid}
        />
      )}
      <DeleteServiceModal data={service} isOpen={isDeleteModalOpen} onClose={() => setIsDeleteModalOpen(false)} />
    </div>
  );
});
