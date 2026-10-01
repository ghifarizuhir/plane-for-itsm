/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { Button } from "@plane/propel/button";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";

type Props = {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  description: string;
  cancelLabel: string;
  confirmLabel: string;
  isDestructive?: boolean;
  isSubmitting?: boolean;
  onConfirm: () => void;
};

export const WarRoomConfirmModal = observer(function WarRoomConfirmModal({
  isOpen,
  onClose,
  title,
  description,
  cancelLabel,
  confirmLabel,
  isDestructive = false,
  isSubmitting = false,
  onConfirm,
}: Props) {
  return (
    <ModalCore isOpen={isOpen} handleClose={onClose} position={EModalPosition.CENTER} width={EModalWidth.XL}>
      <div className="space-y-3 p-5">
        <h3 className="text-16 font-medium text-primary">{title}</h3>
        <p className="text-13 text-secondary">{description}</p>
        <div className="flex items-center justify-end gap-2 pt-1">
          <Button variant="secondary" size="lg" onClick={onClose} disabled={isSubmitting}>
            {cancelLabel}
          </Button>
          <Button
            variant={isDestructive ? "error-fill" : "primary"}
            size="lg"
            onClick={onConfirm}
            loading={isSubmitting}
            disabled={isSubmitting}
          >
            {confirmLabel}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
