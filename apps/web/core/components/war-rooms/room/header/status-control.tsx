/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
// plane imports
import { WAR_ROOM_STATUS_CONFIG, WAR_ROOM_STATUS_TRANSITIONS } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import type { TWarRoomStatus } from "@plane/types";
import { CustomSelect } from "@plane/ui";
import { cn } from "@plane/utils";

type Props = {
  status: TWarRoomStatus;
  disabled?: boolean;
  onChange: (status: TWarRoomStatus) => void;
};

export const WarRoomStatusControl = observer(function WarRoomStatusControl({
  status,
  disabled = false,
  onChange,
}: Props) {
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const config = WAR_ROOM_STATUS_CONFIG[status];
  const transitions = WAR_ROOM_STATUS_TRANSITIONS[status];
  const pill = (
    <span className={cn("rounded-full px-2 py-0.5 text-11 font-medium", config.pill)}>{t(config.label_key)}</span>
  );

  if (transitions.length === 0 || disabled) return pill;

  return (
    <CustomSelect value={status} label={pill} onChange={(value: TWarRoomStatus) => onChange(value)} noChevron>
      {transitions.map((nextStatus) => (
        <CustomSelect.Option key={nextStatus} value={nextStatus}>
          {t(WAR_ROOM_STATUS_CONFIG[nextStatus].label_key)}
        </CustomSelect.Option>
      ))}
    </CustomSelect>
  );
});
