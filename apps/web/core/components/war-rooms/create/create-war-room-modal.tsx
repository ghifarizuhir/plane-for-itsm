/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { observer } from "mobx-react";
import { Controller, useForm } from "react-hook-form";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { AlertOctagonOutline, CloseOutline, InfoOutline } from "@makeplane/propel/icons";
import { WAR_ROOM_SEVERITIES, WAR_ROOM_SEVERITY_CONFIG, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { ISearchIssueResponse, TWarRoomSeverity } from "@plane/types";
import { CustomSelect, EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { ExistingIssuesListModal } from "@/components/core/modals/existing-issues-list-modal";
import { RichTextEditor } from "@/components/editor/rich-text";
import { ServiceMultiSelect } from "@/components/services/select/service-multi-select";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS } from "@/services/service.helpers";
import { severityFromPriority } from "@/services/war-room.helpers";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useService } from "@/hooks/store/use-service";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWorkspace } from "@/hooks/store/use-workspace";
// services
import { WorkspaceService } from "@/services/workspace.service";

const workspaceService = new WorkspaceService();

export type TWarRoomIncidentOption = {
  id: string;
  name: string;
  priority: string | null;
};

type Props = {
  isOpen: boolean;
  onClose: () => void;
  workspaceSlug: string;
  projectId: string;
  initialIssue?: TWarRoomIncidentOption;
  initialIssueId?: string;
};

type TWarRoomCreateFormValues = {
  name: string;
  severity: TWarRoomSeverity | "";
  description_html: string;
  service_ids: string[];
};

export const CreateWarRoomModal = observer(function CreateWarRoomModal(props: Props) {
  const { isOpen, onClose, workspaceSlug, projectId, initialIssue, initialIssueId } = props;
  // plane hooks
  const { t } = useTranslation();
  // router
  const router = useAppRouter();
  // store hooks
  const { createWarRoom, getWarRoomById } = useWarRoom();
  const { getWorkspaceBySlug } = useWorkspace();
  const { fetchedMap, workItemLinkMap } = useService();
  // states
  const [selectedIssue, setSelectedIssue] = useState<TWarRoomIncidentOption | null>(null);
  const [isIncidentPickerOpen, setIsIncidentPickerOpen] = useState(false);
  const [duplicateRoomId, setDuplicateRoomId] = useState<string | null>(null);
  // refs
  const seededIssueId = useRef<string | null>(null);
  // form
  const {
    control,
    formState: { errors, isSubmitting },
    handleSubmit,
    reset,
    setValue,
  } = useForm<TWarRoomCreateFormValues>({
    defaultValues: {
      name: "",
      severity: "",
      description_html: "<p></p>",
      service_ids: [],
    },
  });

  useEffect(() => {
    if (!isOpen) return;
    seededIssueId.current = null;
    setDuplicateRoomId(null);
    setIsIncidentPickerOpen(false);
    const issue = initialIssue ?? (initialIssueId ? { id: initialIssueId, name: "", priority: null } : null);
    setSelectedIssue(issue);
    reset({
      name: initialIssue?.name ?? "",
      severity: severityFromPriority(initialIssue?.priority ?? null) ?? "",
      description_html: "<p></p>",
      service_ids: [],
    });
  }, [isOpen, initialIssue, initialIssueId, reset]);

  // Default affected services from the incident's service-issues links once the
  // service store is hydrated. Runs once per selected incident.
  useEffect(() => {
    if (!isOpen || !selectedIssue || seededIssueId.current === selectedIssue.id) return;
    if (!fetchedMap[projectId]) return;
    seededIssueId.current = selectedIssue.id;
    const linkedServiceIds = Object.values(workItemLinkMap)
      .filter((link) => link.issue_id === selectedIssue.id && link.project_id === projectId)
      .map((link) => link.service_id);
    if (linkedServiceIds.length > 0) setValue("service_ids", linkedServiceIds);
  }, [isOpen, selectedIssue, fetchedMap, workItemLinkMap, projectId, setValue]);

  const handleIncidentSelect = async (issues: ISearchIssueResponse[]) => {
    const issue = issues[0];
    if (!issue) return;
    seededIssueId.current = null;
    setSelectedIssue({ id: issue.id, name: issue.name, priority: null });
    setValue("name", issue.name);
    setIsIncidentPickerOpen(false);
  };

  const handleCreateWarRoom = async (formData: TWarRoomCreateFormValues) => {
    if (!selectedIssue) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("toast.error"),
        message: t("war_room.create.incident_required"),
      });
      return;
    }
    const descriptionHtml = formData.description_html.trim() !== "" ? formData.description_html : "<p></p>";
    try {
      const room = await createWarRoom(workspaceSlug, projectId, {
        name: formData.name.trim() !== "" ? formData.name.trim() : undefined,
        primary_issue_id: selectedIssue.id,
        severity: formData.severity === "" ? undefined : formData.severity,
        description_html: descriptionHtml,
        service_ids: formData.service_ids,
      });
      onClose();
      router.push(getWarRoomLink(workspaceSlug, projectId, room.id));
    } catch (error) {
      const apiError = error as { error?: string; war_room_id?: string; message?: string };
      if (apiError?.error === "active_war_room_exists" && apiError.war_room_id) {
        setDuplicateRoomId(apiError.war_room_id);
        return;
      }
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("toast.error"),
        message: apiError?.message ?? t("war_room.load_error.description"),
      });
    }
  };

  const duplicateRoom = duplicateRoomId ? getWarRoomById(duplicateRoomId) : null;
  const duplicateLabel = duplicateRoom ? `WR-${duplicateRoom.sequence_id}` : t("war_room.create.open_existing");
  const stableDescriptionHtml = "<p></p>";

  return (
    <>
      <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.XXL}>
        <form onSubmit={handleSubmit(handleCreateWarRoom)}>
          <div className="space-y-5 p-5">
            <div className="flex items-center gap-x-3">
              <h3 className="text-18 font-medium text-secondary">{t("war_room.create.title")}</h3>
            </div>

            {duplicateRoomId && (
              <div className="relative flex items-center gap-2 rounded-md border border-danger-strong/50 bg-danger-subtle p-2">
                <InfoOutline width={16} height={16} className="text-danger-primary" />
                <div className="w-full text-13 font-medium text-danger-primary">
                  {t("war_room.create.duplicate_title")}
                  <p className="font-normal text-12">{t("war_room.create.duplicate_description")}</p>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => {
                    onClose();
                    router.push(getWarRoomLink(workspaceSlug, projectId, duplicateRoomId));
                  }}
                >
                  {duplicateLabel}
                </Button>
                <button type="button" onClick={() => setDuplicateRoomId(null)}>
                  <CloseOutline className="h-3.5 w-3.5 text-secondary hover:text-primary" />
                </button>
              </div>
            )}

            <div className="space-y-1">
              <label className="text-12 text-secondary">{t("war_room.create.incident")}</label>
              <button
                type="button"
                onClick={() => setIsIncidentPickerOpen(true)}
                className="flex w-full items-center gap-2 rounded-sm border-[0.5px] border-strong px-2 py-2 text-left hover:bg-layer-1"
              >
                <AlertOctagonOutline className="h-4 w-4 shrink-0 text-tertiary" />
                <span className="truncate text-13 text-primary">
                  {selectedIssue
                    ? selectedIssue.name || t("war_room.create.incident_selected")
                    : t("war_room.create.select_incident")}
                </span>
              </button>
            </div>

            <div className="space-y-1">
              <Controller
                control={control}
                name="name"
                rules={{
                  required: t("title_is_required"),
                  maxLength: {
                    value: 255,
                    message: t("title_should_be_less_than_255_characters"),
                  },
                }}
                render={({ field: { value, onChange } }) => (
                  <Field name="name" invalid={Boolean(errors?.name)}>
                    <InputGroup size="2xl">
                      <Input
                        size="2xl"
                        id="name"
                        name="name"
                        type="text"
                        value={value}
                        onChange={onChange}
                        placeholder={t("war_room.create.name_placeholder")}
                      />
                    </InputGroup>
                  </Field>
                )}
              />
              <span className="text-11 text-danger-primary">{errors?.name?.message}</span>
            </div>

            <div>
              <Controller
                name="description_html"
                control={control}
                render={({ field: { onChange } }) => (
                  <RichTextEditor
                    editable
                    key={selectedIssue?.id ?? "war-room-create"}
                    id="war-room-description-editor"
                    initialValue={stableDescriptionHtml}
                    value={stableDescriptionHtml}
                    workspaceSlug={workspaceSlug}
                    workspaceId={getWorkspaceBySlug(workspaceSlug)?.id ?? ""}
                    projectId={projectId}
                    disabledExtensions={SERVICE_DESCRIPTION_DISABLED_EXTENSIONS}
                    onChange={(_descriptionJson: object, descriptionHtml: string) => {
                      onChange(descriptionHtml);
                    }}
                    placeholder={t("war_room.create.description")}
                    searchMentionCallback={async (payload) =>
                      await workspaceService.searchEntity(workspaceSlug, {
                        ...payload,
                        project_id: projectId,
                      })
                    }
                    containerClassName="min-h-24 rounded-md border border-subtle"
                    uploadFile={async () => {
                      throw new Error("File upload is disabled for war room descriptions.");
                    }}
                    duplicateFile={async () => {
                      throw new Error("File upload is disabled for war room descriptions.");
                    }}
                  />
                )}
              />
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <div className="h-7">
                <Controller
                  control={control}
                  name="severity"
                  render={({ field: { value, onChange } }) => (
                    <CustomSelect
                      value={value}
                      label={
                        <span className="flex items-center gap-2 py-0.5 text-11">
                          {value ? (
                            t(WAR_ROOM_SEVERITY_CONFIG[value].label_key)
                          ) : (
                            <span className="text-secondary">{t("war_room.create.severity")}</span>
                          )}
                        </span>
                      }
                      onChange={onChange}
                      noChevron
                    >
                      <CustomSelect.Option value="">
                        <span>{t("war_room.create.severity_auto")}</span>
                      </CustomSelect.Option>
                      {WAR_ROOM_SEVERITIES.map((severity) => (
                        <CustomSelect.Option key={severity} value={severity}>
                          <span>{t(WAR_ROOM_SEVERITY_CONFIG[severity].label_key)}</span>
                        </CustomSelect.Option>
                      ))}
                    </CustomSelect>
                  )}
                />
              </div>
              <div className="h-7">
                <Controller
                  control={control}
                  name="service_ids"
                  render={({ field: { value, onChange } }) => (
                    <ServiceMultiSelect
                      workspaceSlug={workspaceSlug}
                      projectId={projectId}
                      value={value}
                      onChange={onChange}
                    />
                  )}
                />
              </div>
            </div>
          </div>
          <div className="flex items-center justify-end gap-2 border-t-[0.5px] border-subtle px-5 py-4">
            <Button variant="secondary" size="lg" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              size="lg"
              type="submit"
              loading={isSubmitting}
              disabled={isSubmitting || !selectedIssue}
            >
              {t("war_room.create.submit")}
            </Button>
          </div>
        </form>
      </ModalCore>

      <ExistingIssuesListModal
        isOpen={isIncidentPickerOpen}
        handleClose={() => setIsIncidentPickerOpen(false)}
        workspaceSlug={workspaceSlug}
        projectId={projectId}
        searchParams={{ workspace_search: false }}
        selectionMode="single"
        handleOnSubmit={handleIncidentSelect}
      />
    </>
  );
});
