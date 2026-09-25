/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TNameDescriptionLoader } from "@plane/types";
import { TextArea } from "@plane/ui";
import { cn } from "@plane/utils";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  serviceId: string;
  isSubmitting: TNameDescriptionLoader;
  setIsSubmitting: (value: TNameDescriptionLoader) => void;
};

export const ServiceTitleInput = observer(function ServiceTitleInput(props: Props) {
  const { serviceId, setIsSubmitting } = props;
  // states
  const [title, setTitle] = useState("");
  const [isLengthVisible, setIsLengthVisible] = useState(false);
  // refs
  const lastSaved = useRef("");
  const hasUnsavedChanges = useRef(false);
  const latestTitle = useRef("");
  const lastServiceId = useRef(serviceId);
  const commitRef = useRef<() => Promise<void>>(async () => {});
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, updateService } = useService();
  const { getWorkspaceBySlug } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  const serviceName = service?.name;
  const slug = workspaceSlug?.toString() ?? "";
  const pid = projectId?.toString() ?? service?.project_id ?? "";
  const workspaceId = getWorkspaceBySlug(slug)?.id;

  useEffect(() => {
    if (serviceName === undefined) return;
    const isServiceSwitch = lastServiceId.current !== serviceId;
    if (!isServiceSwitch && hasUnsavedChanges.current) return;
    lastServiceId.current = serviceId;
    setTitle(serviceName);
    latestTitle.current = serviceName;
    lastSaved.current = serviceName;
    hasUnsavedChanges.current = false;
  }, [serviceId, serviceName]);

  useEffect(
    () => () => {
      if (hasUnsavedChanges.current) void commitRef.current();
    },
    []
  );

  if (!service) return null;

  const commit = async () => {
    const trimmed = title.trim();
    if (trimmed.length === 0) {
      setTitle(lastSaved.current);
      latestTitle.current = lastSaved.current;
      hasUnsavedChanges.current = false;
      setIsSubmitting("saved");
      return;
    }
    if (trimmed === lastSaved.current) {
      hasUnsavedChanges.current = false;
      setIsSubmitting("saved");
      return;
    }
    if (!slug || !workspaceId) return;
    setTitle(trimmed);
    latestTitle.current = trimmed;
    setIsSubmitting("submitting");
    try {
      await updateService(slug, workspaceId, pid, serviceId, { name: trimmed });
      lastSaved.current = trimmed;
      if (latestTitle.current.trim() === trimmed) hasUnsavedChanges.current = false;
      setIsSubmitting("submitted");
    } catch {
      if (latestTitle.current.trim() === trimmed) {
        setTitle(lastSaved.current);
        latestTitle.current = lastSaved.current;
        hasUnsavedChanges.current = false;
      }
      setIsSubmitting("saved");
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("common.error.label"),
        message: t("entity.update.failed", { entity: t("service.title") }),
      });
    }
  };

  commitRef.current = commit;

  return (
    <div className="flex flex-col gap-1.5">
      <div className="relative -ml-3">
        <TextArea
          id="service-title-input"
          className={cn(
            "block w-full resize-none overflow-hidden rounded-sm border-none bg-transparent px-3 py-0 text-20 font-medium ring-0 outline-none",
            { "mx-2.5 ring-1 ring-danger-strong": title.length === 0 }
          )}
          value={title}
          onChange={(e) => {
            setIsSubmitting("submitting");
            latestTitle.current = e.target.value;
            hasUnsavedChanges.current = true;
            setTitle(e.target.value);
          }}
          onBlur={() => {
            setIsLengthVisible(false);
            void commit();
          }}
          maxLength={255}
          placeholder={t("service.fields.name")}
          onFocus={() => setIsLengthVisible(true)}
        />
        <div
          className={cn(
            "pointer-events-none absolute right-1 bottom-1 z-[2] rounded-sm bg-surface-1 p-0.5 text-11 text-secondary opacity-0 transition-opacity",
            { "opacity-100": isLengthVisible }
          )}
        >
          <span className={`${title.length === 0 || title.length > 255 ? "text-danger-primary" : ""}`}>
            {title.length}
          </span>
          /255
        </div>
      </div>
      {title.length === 0 && (
        <span className="text-13 font-medium text-danger-primary">{t("form.title.required")}</span>
      )}
    </div>
  );
});
