/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TNameDescriptionLoader } from "@plane/types";
import { EFileAssetType } from "@plane/types";
// components
import { DescriptionInput } from "@/components/editor/rich-text/description-input";
import { NameDescriptionUpdateStatus } from "@/components/issues/issue-update-status";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS, stripHtmlToText } from "@/services/service.helpers";

type Props = {
  serviceId: string;
  isSubmitting: TNameDescriptionLoader;
  setIsSubmitting: (value: TNameDescriptionLoader) => void;
};

export const ServiceDescription = observer(function ServiceDescription(props: Props) {
  const { serviceId, isSubmitting, setIsSubmitting } = props;
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, updateService } = useService();
  const { getWorkspaceBySlug } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString() ?? "";
  const pid = projectId?.toString() ?? service.project_id;
  const workspaceId = getWorkspaceBySlug(slug)?.id;

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-4">
        <h4 className="text-13 font-medium text-secondary">{t("service.fields.description")}</h4>
        <NameDescriptionUpdateStatus isSubmitting={isSubmitting} />
      </div>
      {slug && workspaceId && (
        <DescriptionInput
          containerClassName="-ml-6 border-none p-0! pl-6!"
          entityId={service.id}
          fileAssetType={EFileAssetType.PROJECT_DESCRIPTION}
          initialValue={service.description_html || "<p></p>"}
          key={service.id}
          disabledExtensions={SERVICE_DESCRIPTION_DISABLED_EXTENSIONS}
          onSubmit={async (value) => {
            const descriptionHtml =
              value.description_html && value.description_html.trim() !== "" ? value.description_html : "<p></p>";
            await updateService(slug, workspaceId, pid, service.id, {
              description: stripHtmlToText(descriptionHtml),
              description_html: descriptionHtml,
            });
          }}
          projectId={pid}
          setIsSubmitting={(value) => setIsSubmitting(value)}
          workspaceSlug={slug}
          placeholder={t("service.fields.description")}
        />
      )}
    </div>
  );
});
