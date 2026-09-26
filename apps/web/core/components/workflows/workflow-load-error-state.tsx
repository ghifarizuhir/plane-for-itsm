/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { WarningTriangleOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";

type Props = {
  onRetry: () => void;
};

export function WorkflowLoadErrorState({ onRetry }: Props) {
  const { t } = useTranslation();

  return (
    <div className="flex h-40 w-full flex-col items-center justify-center gap-3 p-6 text-center">
      <WarningTriangleOutline className="size-8 text-tertiary" />
      <p className="text-sm font-medium text-primary">{t("common.error.label")}</p>
      <p className="text-xs text-secondary">{t("common.error.message")}</p>
      <Button variant="secondary" size="sm" onClick={onRetry}>
        {t("common.retry")}
      </Button>
    </div>
  );
}
