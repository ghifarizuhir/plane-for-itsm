/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  isSubmitting?: boolean;
  onConfirm: (note: string) => void;
};

export const WarRoomResolveModal = observer(function WarRoomResolveModal({
  isOpen,
  onClose,
  isSubmitting = false,
  onConfirm,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // states
  const [note, setNote] = useState("");

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.CENTER} width={EModalWidth.XL}>
      <div className="space-y-3 p-5">
        <h3 className="text-16 font-medium text-primary">{t("war_room.resolve_modal.title")}</h3>
        <p className="text-13 text-secondary">{t("war_room.resolve_modal.description")}</p>
        <div className="space-y-1">
          <label className="text-12 text-secondary">{t("war_room.resolve_modal.note_label")}</label>
          <textarea
            value={note}
            onChange={(event) => setNote(event.target.value)}
            rows={3}
            placeholder={t("war_room.resolve_modal.note_placeholder")}
            className="w-full resize-none rounded-sm border border-subtle bg-surface-1 px-2 py-1.5 text-13 text-primary outline-none focus:border-strong"
          />
        </div>
        <div className="flex items-center justify-end gap-2 pt-1">
          <Button variant="secondary" size="lg" onClick={onClose} disabled={isSubmitting}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            onClick={() => onConfirm(note)}
            loading={isSubmitting}
            disabled={isSubmitting}
          >
            {t("war_room.resolve_modal.confirm")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
