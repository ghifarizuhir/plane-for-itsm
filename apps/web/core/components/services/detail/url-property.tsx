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
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// ui
import { Input } from "@plane/ui";
// helpers
import { isValidServiceUrl, normalizeServiceUrl } from "@/services/service.helpers";

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
    if (isEditing) return;
    setDraft(value ?? "");
  }, [value, isEditing]);

  const commit = () => {
    const next = normalizeServiceUrl(draft);
    if (next !== null && !isValidServiceUrl(next)) {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("common.error.label"),
        message: t("common.url_is_invalid"),
      });
      return;
    }
    setIsEditing(false);
    setDraft(next ?? "");
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
          if (e.key === "Enter" && !e.nativeEvent.isComposing) {
            e.preventDefault();
            commit();
          }
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
        <span className="truncate">{value}</span>
      </a>
      <button
        type="button"
        onClick={() => setIsEditing(true)}
        className="shrink-0 rounded-sm p-1 text-tertiary opacity-0 group-hover:opacity-100 hover:text-primary focus-visible:opacity-100"
        aria-label={t("edit")}
      >
        <EditOutline className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
