/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import type { TWorkItemType } from "@plane/types";
import { Switch } from "@makeplane/propel/components/switch";

type Props = {
  type: TWorkItemType;
  enabled: boolean;
  onToggle: (next: boolean) => Promise<void> | void;
};

export const TypeToggleItem = observer(function TypeToggleItem(props: Props) {
  const { type, enabled, onToggle } = props;
  // states
  const [isUpdating, setIsUpdating] = useState(false);

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
        {type.description && <p className="mt-0.5 truncate text-11 text-secondary">{type.description}</p>}
      </div>
      <Switch
        size="sm"
        checked={enabled}
        onCheckedChange={(next) => void handleToggle(next)}
        disabled={isUpdating}
        aria-label={type.name}
      />
    </div>
  );
});
