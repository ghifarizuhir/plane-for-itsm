/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { AiStar1Outline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";

const SUGGESTIONS = [
  "assistant.galileo.suggestion_summarize",
  "assistant.galileo.suggestion_procedure",
  "assistant.galileo.suggestion_draft",
  "assistant.galileo.suggestion_explain",
] as const;

export function GalileoEmptyState({ onSelect }: { onSelect: (prompt: string) => void }) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-10 text-center">
      <span className="flex size-12 items-center justify-center rounded-xl border border-subtle bg-layer-1">
        <AiStar1Outline className="size-5 text-accent-primary" />
      </span>
      <h2 className="mt-4 text-16 font-semibold text-primary">{t("assistant.galileo.empty_title")}</h2>
      <p className="mt-1 max-w-md text-13 leading-relaxed text-tertiary">{t("assistant.galileo.empty_subtitle")}</p>
      <div className="mt-6 grid w-full max-w-md grid-cols-1 gap-2 sm:grid-cols-2">
        {SUGGESTIONS.map((key) => (
          <button
            key={key}
            type="button"
            onClick={() => onSelect(t(key))}
            className="rounded-lg border border-subtle bg-layer-1 px-3 py-2 text-left text-12 text-secondary transition-colors hover:border-strong hover:text-primary"
          >
            {t(key)}
          </button>
        ))}
      </div>
    </div>
  );
}
