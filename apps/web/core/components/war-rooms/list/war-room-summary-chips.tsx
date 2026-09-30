/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { IWarRoomSummary } from "@plane/types";
import { cn } from "@plane/utils";

type Props = {
  summary: IWarRoomSummary;
};

const SUMMARY_CHIPS: { key: keyof IWarRoomSummary; className: string; label_key: string }[] = [
  { key: "active", className: "bg-danger-subtle text-danger-primary", label_key: "war_room.summary.active" },
  { key: "sev1_2", className: "bg-warning-subtle text-warning-primary", label_key: "war_room.summary.sev1_2" },
  {
    key: "resolved_7d",
    className: "bg-success-subtle text-success-primary",
    label_key: "war_room.summary.resolved_7d",
  },
];

export function WarRoomSummaryChips({ summary }: Props) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-subtle px-3 py-2">
      {SUMMARY_CHIPS.map((chip) => (
        <span key={chip.key} className={cn("rounded-full px-2 py-0.5 text-11 font-medium", chip.className)}>
          {t(chip.label_key, { count: summary[chip.key] })}
        </span>
      ))}
    </div>
  );
}
