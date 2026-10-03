/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useCallback, useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TWorkItemType } from "@plane/types";
// ui
import { Spinner } from "@plane/ui";
// components
import { SettingsHeading } from "@/components/settings/heading";
import { LoadErrorState } from "@/components/common/load-error-state";
// hooks
import { useWorkItemType } from "@/hooks/store/use-work-item-type";
import { useAppRouter } from "@/hooks/use-app-router";
// local imports
import { DeleteTypeModal } from "./delete-type-modal";
import { TypeFormModal } from "./type-form-modal";
import { TypeListItem } from "./type-list-item";

type Props = {
  workspaceSlug: string;
};

export const WorkItemTypesRoot = observer(function WorkItemTypesRoot(props: Props) {
  const { workspaceSlug } = props;
  // router
  const router = useAppRouter();
  // states
  const [isFormOpen, setIsFormOpen] = useState(false);
  const [editingTypeId, setEditingTypeId] = useState<string | null>(null);
  const [deletingTypeId, setDeletingTypeId] = useState<string | null>(null);
  const [hasFetchError, setHasFetchError] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { workItemTypes, fetchWorkItemTypes } = useWorkItemType();
  // derived values
  // `t` is rebuilt on every render by `useTranslation`, so depending on it directly would re-run the effect endlessly.
  // Capturing the resolved strings keeps the dependency values stable across renders.
  const fetchErrorTitle = t("common.error.label");
  const fetchErrorMessage = t("common.error.message");

  const loadData = useCallback(() => {
    setHasFetchError(false);
    void fetchWorkItemTypes(workspaceSlug).catch((error: any) => {
      setHasFetchError(true);
      setToast({
        type: TOAST_TYPE.ERROR,
        title: fetchErrorTitle,
        message: error?.error ?? fetchErrorMessage,
      });
    });
  }, [workspaceSlug, fetchWorkItemTypes, fetchErrorTitle, fetchErrorMessage]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleCreated = (type: TWorkItemType) => {
    router.push(`/${workspaceSlug}/settings/work-item-types/${type.id}`);
  };

  return (
    <>
      <TypeFormModal
        workspaceSlug={workspaceSlug}
        isOpen={isFormOpen}
        typeId={editingTypeId}
        onClose={() => {
          setIsFormOpen(false);
          setEditingTypeId(null);
        }}
        onSuccess={handleCreated}
      />
      <DeleteTypeModal
        workspaceSlug={workspaceSlug}
        isOpen={Boolean(deletingTypeId)}
        typeId={deletingTypeId}
        onClose={() => setDeletingTypeId(null)}
      />
      <SettingsHeading
        title={t("workspace_settings.settings.work_item_types.heading")}
        description={t("workspace_settings.settings.work_item_types.description")}
        control={
          <Button
            variant="primary"
            size="lg"
            onClick={() => {
              setEditingTypeId(null);
              setIsFormOpen(true);
            }}
          >
            {t("workspace_settings.settings.work_item_types.add_type")}
          </Button>
        }
      />
      {hasFetchError && workItemTypes === undefined ? (
        <div className="mt-6">
          <LoadErrorState onRetry={loadData} />
        </div>
      ) : workItemTypes === undefined ? (
        <div className="mt-6 flex h-40 items-center justify-center">
          <Spinner />
        </div>
      ) : workItemTypes.length === 0 ? (
        <div className="mt-6 flex flex-col items-center justify-center gap-1 rounded-lg border border-subtle py-16 text-center">
          <p className="text-13 font-medium text-primary">
            {t("workspace_settings.settings.work_item_types.empty_state.title")}
          </p>
          <p className="text-11 text-tertiary">
            {t("workspace_settings.settings.work_item_types.empty_state.description")}
          </p>
        </div>
      ) : (
        <div className="mt-6 flex flex-col divide-y divide-subtle rounded-lg border border-subtle">
          {workItemTypes.map((type) => (
            <TypeListItem
              key={type.id}
              workspaceSlug={workspaceSlug}
              type={type}
              onEdit={() => {
                setEditingTypeId(type.id);
                setIsFormOpen(true);
              }}
              onDelete={() => setDeletingTypeId(type.id)}
            />
          ))}
        </div>
      )}
    </>
  );
});
