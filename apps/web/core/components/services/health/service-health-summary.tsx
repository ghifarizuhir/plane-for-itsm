/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useTranslation } from "@plane/i18n";
import type { TServiceHealthSummary } from "@plane/types";
import { cn } from "@plane/utils";

type Props = {
  summary: TServiceHealthSummary;
};

const SUMMARY_CHIPS: { key: keyof TServiceHealthSummary; className: string; label_key: string }[] = [
  { key: "down", className: "bg-danger-subtle text-danger-primary", label_key: "service.summary.down" },
  { key: "degraded", className: "bg-warning-subtle text-warning-primary", label_key: "service.summary.degraded" },
  { key: "healthy", className: "bg-success-subtle text-success-primary", label_key: "service.summary.healthy" },
];

export function ServiceHealthSummary({ summary }: Props) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-subtle px-3 py-2">
      {SUMMARY_CHIPS.map((chip) => (
        <span key={chip.key} className={cn("rounded-full px-2 py-0.5 text-11 font-medium", chip.className)}>
          {t(chip.label_key, { count: summary[chip.key] })}
        </span>
      ))}
      {summary.criticalImpacted > 0 && (
        <span className="rounded-full border border-subtle px-2 py-0.5 text-11 text-secondary">
          {t("service.summary.critical_impacted", { count: summary.criticalImpacted })}
        </span>
      )}
    </div>
  );
}
