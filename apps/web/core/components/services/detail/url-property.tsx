/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
// icons
import { EditOutline } from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
// ui
import { Input } from "@plane/ui";
// helpers
import { normalizeServiceUrl } from "@/services/service.helpers";

type Props = {
  value: string | null;
  onSubmit: (value: string | null) => void;
};

export function ServiceUrlProperty(props: Props) {
  const { value, onSubmit } = props;
  // states
  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState(value ?? "");
  // plane hooks
  const { t } = useTranslation();

  useEffect(() => {
    setDraft(value ?? "");
  }, [value]);

  const commit = () => {
    const next = normalizeServiceUrl(draft);
    setIsEditing(false);
    if (next !== value) onSubmit(next);
  };

  const cancel = () => {
    setDraft(value ?? "");
    setIsEditing(false);
  };

  if (isEditing) {
    return (
      <Input
        // oxlint-disable-next-line eslint-plugin-jsx-a11y/no-autofocus
        autoFocus
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
          if (e.key === "Escape") cancel();
        }}
        placeholder="https://"
        className="h-7.5 w-full grow rounded-sm px-2 text-body-xs-regular"
      />
    );
  }

  if (!value) {
    return (
      <button
        type="button"
        onClick={() => setIsEditing(true)}
        className="h-7.5 rounded-sm px-2 text-body-xs-regular text-placeholder hover:bg-layer-1"
      >
        {t("add")}
      </button>
    );
  }

  return (
    <div className="group flex w-full items-center gap-1">
      <a
        href={value}
        target="_blank"
        rel="noopener noreferrer"
        className="flex h-7.5 min-w-0 grow items-center truncate rounded-sm px-2 text-body-xs-regular text-primary hover:bg-layer-1"
        title={value}
      >
        {value}
      </a>
      <button
        type="button"
        onClick={() => setIsEditing(true)}
        className="hidden shrink-0 rounded-sm p-1 text-tertiary group-hover:block hover:text-primary"
        aria-label={t("edit")}
      >
        <EditOutline className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
