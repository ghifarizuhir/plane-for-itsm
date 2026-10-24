/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import useSWR from "swr";
import { API_BASE_URL } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { EmptyStateCompact } from "@plane/propel/empty-state";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IMetricSource } from "@plane/types";
import { copyTextToClipboard } from "@plane/utils";
import { AlertModalCore } from "@plane/ui";
import { SettingsHeading } from "@/components/settings/heading";
import { ServiceMetricsService } from "@/services/service-metrics.service";
import { ServiceService } from "@/services/service.service";
import { MetricSourceForm } from "./source-form";

const metricsService = new ServiceMetricsService();
const serviceService = new ServiceService();

type Props = {
  workspaceSlug: string;
};

export function MetricSourcesRoot({ workspaceSlug }: Props) {
  const { t } = useTranslation();
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<IMetricSource | null>(null);
  const [deleting, setDeleting] = useState<IMetricSource | null>(null);
  const [rotating, setRotating] = useState<IMetricSource | null>(null);
  const [revealed, setRevealed] = useState<Record<string, boolean>>({});
  const [submitting, setSubmitting] = useState(false);

  const {
    data: sources,
    isLoading,
    mutate,
  } = useSWR(workspaceSlug ? `METRIC_SOURCES_${workspaceSlug}` : null, () => metricsService.getSources(workspaceSlug));
  const { data: services } = useSWR(workspaceSlug ? `METRIC_SOURCE_SERVICES_${workspaceSlug}` : null, () =>
    serviceService.getWorkspaceServices(workspaceSlug)
  );

  const handleDelete = async () => {
    if (!deleting) return;
    setSubmitting(true);
    try {
      await metricsService.deleteSource(workspaceSlug, deleting.id);
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("workspace_settings.settings.metric_sources.deleted"),
      });
      setDeleting(null);
      await mutate();
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("common.error") });
    } finally {
      setSubmitting(false);
    }
  };

  const handleRotate = async () => {
    if (!rotating) return;
    setSubmitting(true);
    try {
      await metricsService.rotateSource(workspaceSlug, rotating.id);
      setToast({
        type: TOAST_TYPE.SUCCESS,
        title: t("workspace_settings.settings.metric_sources.rotated"),
      });
      setRotating(null);
      await mutate();
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: t("common.error") });
    } finally {
      setSubmitting(false);
    }
  };

  const copyToken = async (token: string) => {
    await copyTextToClipboard(token);
    setToast({
      type: TOAST_TYPE.SUCCESS,
      title: t("workspace_settings.settings.metric_sources.token_copied"),
    });
  };

  return (
    <div className="w-full">
      <MetricSourceForm
        isOpen={showForm}
        workspaceSlug={workspaceSlug}
        source={editing}
        services={services ?? []}
        onClose={() => {
          setShowForm(false);
          setEditing(null);
        }}
        onSaved={async () => {
          await mutate();
          setShowForm(false);
          setEditing(null);
        }}
      />
      <AlertModalCore
        isOpen={Boolean(deleting)}
        handleClose={() => setDeleting(null)}
        handleSubmit={handleDelete}
        isSubmitting={submitting}
        title={t("workspace_settings.settings.metric_sources.delete")}
        content={t("workspace_settings.settings.metric_sources.delete_confirm")}
        variant="danger"
        primaryButtonText={{
          loading: t("common.delete"),
          default: t("common.delete"),
        }}
      />
      <AlertModalCore
        isOpen={Boolean(rotating)}
        handleClose={() => setRotating(null)}
        handleSubmit={handleRotate}
        isSubmitting={submitting}
        title={t("workspace_settings.settings.metric_sources.rotate")}
        content={t("workspace_settings.settings.metric_sources.rotate_confirm")}
        primaryButtonText={{
          loading: t("workspace_settings.settings.metric_sources.rotate"),
          default: t("workspace_settings.settings.metric_sources.rotate"),
        }}
      />

      <SettingsHeading
        title={t("workspace_settings.settings.metric_sources.heading")}
        description={t("workspace_settings.settings.metric_sources.description")}
        control={
          <Button
            variant="primary"
            size="lg"
            onClick={() => {
              setEditing(null);
              setShowForm(true);
            }}
          >
            {t("workspace_settings.settings.metric_sources.add")}
          </Button>
        }
      />

      {isLoading ? (
        <p className="mt-4 text-13 text-tertiary">{t("common.loading")}</p>
      ) : !sources?.length ? (
        <div className="mt-4 flex justify-center py-20">
          <EmptyStateCompact
            assetKey="unknown"
            title={t("workspace_settings.settings.metric_sources.empty_title")}
            description={t("workspace_settings.settings.metric_sources.empty_description")}
          />
        </div>
      ) : (
        <div className="mt-4 divide-y divide-subtle rounded-md border border-subtle">
          {sources.map((source) => {
            const mapped = Object.keys(source.config.service_map ?? {}).length;
            return (
              <div key={source.id} className="flex flex-col gap-2 p-3 md:flex-row md:items-center">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-13 font-medium text-primary">{source.name}</p>
                  <p className="text-11 text-tertiary">
                    {mapped} {t("workspace_settings.settings.metric_sources.mapped_services")} ·{" "}
                    {source.is_active
                      ? t("workspace_settings.settings.metric_sources.active")
                      : t("workspace_settings.settings.metric_sources.inactive")}
                  </p>
                </div>
                <div className="flex min-w-0 items-center gap-2">
                  <code className="max-w-55 truncate rounded bg-surface-2 px-2 py-1 text-11 text-secondary">
                    {revealed[source.id] ? source.token : `${source.token.slice(0, 12)}…`}
                  </code>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => setRevealed({ ...revealed, [source.id]: !revealed[source.id] })}
                  >
                    {revealed[source.id]
                      ? t("workspace_settings.settings.metric_sources.hide")
                      : t("workspace_settings.settings.metric_sources.show")}
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => copyToken(source.token)}>
                    {t("workspace_settings.settings.metric_sources.copy_token")}
                  </Button>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => {
                      setEditing(source);
                      setShowForm(true);
                    }}
                  >
                    {t("common.edit")}
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => setRotating(source)}>
                    {t("workspace_settings.settings.metric_sources.rotate")}
                  </Button>
                  <Button variant="secondary" size="sm" onClick={() => setDeleting(source)}>
                    {t("common.delete")}
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <div className="mt-6 rounded-md border border-subtle p-3">
        <p className="text-12 font-medium text-primary">{t("workspace_settings.settings.metric_sources.snippet")}</p>
        <pre className="mt-2 overflow-x-auto rounded bg-surface-2 p-2 text-11 text-secondary">
          {`curl -X POST "${API_BASE_URL}/api/inbound/metrics/<token>/" \\
  -H 'Content-Type: application/json' \\
  -d '{"metrics":[{"service":"payment-api","key":"http.latency","unit":"ms","resolution":60,"bucket_start":"2026-10-24T10:00:00Z","count":60,"avg":120}]}'`}
        </pre>
      </div>
    </div>
  );
}
