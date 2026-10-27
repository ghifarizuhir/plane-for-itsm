/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { AiStar1Outline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";
import type { TAiMessage } from "@/lib/ai-context";
import { sanitizeAssistantHtml } from "@/lib/ai-context";

type Props = {
  message: TAiMessage;
  showRetry: boolean;
  onRetry: () => void;
};

export function GalileoMessage({ message, showRetry, onRetry }: Props) {
  const { t } = useTranslation();

  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[85%] rounded-2xl rounded-br-md bg-accent-primary px-3.5 py-2 text-13 leading-relaxed whitespace-pre-wrap text-on-color">
          {message.content}
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-w-0 gap-2.5">
      <span className="mt-0.5 flex size-6 flex-shrink-0 items-center justify-center rounded-md border border-subtle bg-layer-1">
        <AiStar1Outline className="size-3.5 text-accent-primary" />
      </span>
      <div className="min-w-0 flex-1">
        {message.isError ? (
          <div className="rounded-lg border border-danger-strong/50 bg-danger-subtle px-3 py-2 text-13 leading-relaxed whitespace-pre-wrap text-danger-primary">
            {message.content}
            {showRetry && (
              <button
                type="button"
                onClick={onRetry}
                className="mt-2 block text-12 font-medium underline transition-opacity hover:opacity-80"
              >
                {t("common.retry")}
              </button>
            )}
          </div>
        ) : (
          <div
            className="ai-prose w-full text-13 text-primary"
            dangerouslySetInnerHTML={{ __html: sanitizeAssistantHtml(message.content) }}
          />
        )}
      </div>
    </div>
  );
}
