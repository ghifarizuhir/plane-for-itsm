/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { WarningTriangleOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";

type Props = {
  onRetry: () => void;
};

export function ServiceLoadErrorState({ onRetry }: Props) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center">
      <WarningTriangleOutline className="size-8 text-tertiary" />
      <p className="text-sm font-medium text-primary">{t("service.detail.load_error_title")}</p>
      <p className="text-xs text-secondary">{t("service.detail.load_error_description")}</p>
      <Button variant="secondary" size="sm" onClick={onRetry}>
        {t("service.detail.retry")}
      </Button>
    </div>
  );
}
