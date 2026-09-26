/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { TWorkItemType } from "@plane/types";
import { Switch } from "@makeplane/propel/components/switch";

type Props = {
  type: TWorkItemType;
  workflowName?: string;
  enabled: boolean;
  disabledReason?: string;
  onToggle: (next: boolean) => Promise<void> | void;
};

export const TypeToggleItem = observer(function TypeToggleItem(props: Props) {
  const { type, workflowName, enabled, disabledReason, onToggle } = props;
  // states
  const [isUpdating, setIsUpdating] = useState(false);
  // plane hooks
  const { t } = useTranslation();
  // derived values
  const isDisabled = Boolean(disabledReason) || isUpdating;

  const handleToggle = async (next: boolean) => {
    setIsUpdating(true);
    try {
      await onToggle(next);
    } finally {
      setIsUpdating(false);
    }
  };

  return (
    <div className="flex items-center justify-between gap-4 px-4 py-3">
      <div className="flex min-w-0 flex-col">
        <p className="truncate text-13 font-medium text-primary">{type.name}</p>
        <p className="mt-0.5 truncate text-11 text-secondary">
          {workflowName ?? t("workspace_settings.settings.work_item_types.no_workflow")}
        </p>
        {disabledReason && <p className="mt-0.5 text-11 text-tertiary">{disabledReason}</p>}
      </div>
      <Switch
        size="sm"
        checked={enabled}
        onCheckedChange={(next) => void handleToggle(next)}
        disabled={isDisabled}
        aria-label={type.name}
      />
    </div>
  );
});
