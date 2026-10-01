/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { Field } from "@makeplane/propel/components/field";
import { Input, InputGroup } from "@makeplane/propel/components/input";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import type { IWarRoom } from "@plane/types";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { RichTextEditor } from "@/components/editor/rich-text";
// helpers
import { SERVICE_DESCRIPTION_DISABLED_EXTENSIONS } from "@/services/service.helpers";
// hooks
import { useWorkspace } from "@/hooks/store/use-workspace";
// services
import { WorkspaceService } from "@/services/workspace.service";

const workspaceService = new WorkspaceService();

type Props = {
  isOpen: boolean;
  onClose: () => void;
  room: IWarRoom;
  workspaceSlug: string;
  projectId: string;
  isSubmitting?: boolean;
  onSave: (data: { name: string; description_html: string }) => void;
};

export const WarRoomEditDetailsModal = observer(function WarRoomEditDetailsModal({
  isOpen,
  onClose,
  room,
  workspaceSlug,
  projectId,
  isSubmitting = false,
  onSave,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getWorkspaceBySlug } = useWorkspace();
  // states
  const [name, setName] = useState(room.name);
  const [descriptionHtml, setDescriptionHtml] = useState(room.description_html);

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.TOP} width={EModalWidth.XXL}>
      <div className="space-y-4 p-5">
        <h3 className="text-18 font-medium text-secondary">{t("war_room.edit_details_modal.title")}</h3>
        <Field name="name">
          <InputGroup size="2xl">
            <Input
              size="2xl"
              id="war-room-edit-name"
              name="name"
              type="text"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={t("war_room.header.rename_placeholder")}
              maxLength={255}
            />
          </InputGroup>
        </Field>
        <div>
          <RichTextEditor
            editable
            key={room.id}
            id="war-room-edit-description-editor"
            initialValue={room.description_html}
            value={descriptionHtml}
            workspaceSlug={workspaceSlug}
            workspaceId={getWorkspaceBySlug(workspaceSlug)?.id ?? ""}
            projectId={projectId}
            disabledExtensions={SERVICE_DESCRIPTION_DISABLED_EXTENSIONS}
            onChange={(_json: object, html: string) => setDescriptionHtml(html)}
            placeholder={t("war_room.edit_details_modal.description")}
            containerClassName="min-h-24 rounded-md border border-subtle"
            searchMentionCallback={async (payload) =>
              await workspaceService.searchEntity(workspaceSlug, {
                ...payload,
                project_id: projectId,
              })
            }
            uploadFile={async () => {
              throw new Error("File upload is disabled for war room descriptions.");
            }}
            duplicateFile={async () => {
              throw new Error("File upload is disabled for war room descriptions.");
            }}
          />
        </div>
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={onClose} disabled={isSubmitting}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            onClick={() => onSave({ name: name.trim(), description_html: descriptionHtml })}
            loading={isSubmitting}
            disabled={isSubmitting || name.trim() === ""}
          >
            {t("war_room.edit_details_modal.save")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
