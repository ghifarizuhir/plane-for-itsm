/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useEffect, useState } from "react";
// plane imports
import { API_BASE_URL } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { TIntakeSource } from "@plane/types";
// hooks
import { useIntakeSource } from "@/hooks/store/use-intake-source";
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";
// helpers
import { buildWebhookUrl } from "@/store/intake-source.helpers";
// local imports
import { IntakeSourceFormModal } from "./form-modal";

type Props = {
  workspaceSlug: string;
  projectId: string;
};

export const IntakeSourcesRoot = observer(function IntakeSourcesRoot(props: Props) {
  const { workspaceSlug, projectId } = props;
  // translation
  const { t } = useTranslation();
  // store hooks
  const { loader, getSourcesByProject, fetchSources, updateSource, deleteSource, rotateSource } = useIntakeSource();
  const { fetchServices } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const sources = getSourcesByProject(projectId);
  // undefined = tertutup; null = modal create; source = modal edit
  const [formSource, setFormSource] = useState<TIntakeSource | null | undefined>(undefined);

  useEffect(() => {
    void fetchSources(workspaceSlug, projectId);
  }, [workspaceSlug, projectId, fetchSources]);

  useEffect(() => {
    if (currentWorkspace?.id) void fetchServices(workspaceSlug, currentWorkspace.id, projectId);
  }, [workspaceSlug, projectId, currentWorkspace?.id, fetchServices]);

  const copyUrl = (token: string) => {
    void navigator.clipboard.writeText(buildWebhookUrl(API_BASE_URL, token));
  };

  const handleRotate = async (source: TIntakeSource) => {
    if (!window.confirm(t("project_settings.intake_sources.rotate_confirm"))) return;
    const token = await rotateSource(workspaceSlug, projectId, source.id);
    copyUrl(token);
  };

  const handleDelete = async (source: TIntakeSource) => {
    if (!window.confirm(t("project_settings.intake_sources.delete_confirm", { name: source.name }))) return;
    await deleteSource(workspaceSlug, projectId, source.id);
  };

  if (sources === null) {
    return <div className="mt-4 text-13 text-tertiary">{loader ? t("common.loading") : null}</div>;
  }

  return (
    <div className="mt-4 flex flex-col gap-3">
      <div className="flex justify-end">
        <Button variant="primary" size="sm" onClick={() => setFormSource(null)}>
          {t("project_settings.intake_sources.add_source")}
        </Button>
      </div>

      {sources.length === 0 ? (
        <div className="rounded-md border border-subtle p-6 text-center text-13 text-tertiary">
          {t("project_settings.intake_sources.empty")}
        </div>
      ) : (
        sources.map((source) => (
          <div
            key={source.id}
            className="flex flex-col gap-3 rounded-md border border-subtle p-4 sm:flex-row sm:items-center sm:justify-between"
          >
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <p className="truncate text-body-sm-medium">{source.name}</p>
                <span className="rounded-sm bg-layer-2 px-1.5 py-0.5 text-caption-sm-regular text-tertiary">
                  {source.is_active
                    ? t("project_settings.intake_sources.active")
                    : t("project_settings.intake_sources.inactive")}
                </span>
                {source.auto_accept && (
                  <span className="rounded-sm bg-layer-2 px-1.5 py-0.5 text-caption-sm-regular text-tertiary">
                    {t("project_settings.intake_sources.auto_accept")}
                  </span>
                )}
              </div>
              <p className="mt-1 truncate text-caption-md-regular text-tertiary">
                {buildWebhookUrl(API_BASE_URL, source.token)}
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button variant="secondary" size="sm" onClick={() => copyUrl(source.token)}>
                {t("project_settings.intake_sources.copy_url")}
              </Button>
              <Button variant="secondary" size="sm" onClick={() => void handleRotate(source)}>
                {t("project_settings.intake_sources.rotate")}
              </Button>
              <Button variant="secondary" size="sm" onClick={() => setFormSource(source)}>
                {t("common.edit")}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void updateSource(workspaceSlug, projectId, source.id, { is_active: !source.is_active })}
              >
                {source.is_active
                  ? t("project_settings.intake_sources.deactivate")
                  : t("project_settings.intake_sources.activate")}
              </Button>
              <Button variant="error-outline" size="sm" onClick={() => void handleDelete(source)}>
                {t("common.delete")}
              </Button>
            </div>
          </div>
        ))
      )}

      {formSource !== undefined && (
        <IntakeSourceFormModal
          workspaceSlug={workspaceSlug}
          projectId={projectId}
          source={formSource}
          isOpen
          onClose={() => setFormSource(undefined)}
        />
      )}
    </div>
  );
});
