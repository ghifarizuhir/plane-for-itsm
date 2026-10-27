/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useRef, useState } from "react";
import { SendOutline } from "@makeplane/propel/icons";
import { useTranslation } from "@plane/i18n";

type Props = {
  disabled: boolean;
  onSend: (question: string) => void;
};

export function GalileoComposer({ disabled, onSend }: Props) {
  const { t } = useTranslation();
  const [question, setQuestion] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const textarea = textareaRef.current;
    if (!textarea) return;
    textarea.style.height = "auto";
    textarea.style.height = `${Math.min(textarea.scrollHeight, 144)}px`;
  }, [question]);

  const handleSend = () => {
    const trimmed = question.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setQuestion("");
    textareaRef.current?.focus();
  };

  return (
    <div className="border-t border-subtle p-3">
      <div className="rounded-lg border border-subtle bg-layer-1 focus-within:border-strong">
        <textarea
          ref={textareaRef}
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              handleSend();
            }
          }}
          disabled={disabled}
          rows={1}
          aria-label={t("assistant.galileo.composer_placeholder")}
          placeholder={t("assistant.galileo.composer_placeholder")}
          className="max-h-36 w-full resize-none bg-transparent px-3.5 pt-2.5 pb-1 text-13 text-primary outline-none placeholder:text-tertiary disabled:opacity-60"
        />
        <div className="flex items-center gap-3 px-3.5 pb-2">
          <span className="hidden text-10 text-tertiary sm:inline">{t("assistant.galileo.composer_hint")}</span>
          <button
            type="button"
            onClick={handleSend}
            disabled={disabled || !question.trim()}
            aria-label={t("assistant.galileo.send")}
            className="ml-auto flex size-7 items-center justify-center rounded-full bg-accent-primary text-on-color transition-opacity hover:opacity-90 disabled:opacity-40"
          >
            <SendOutline className="size-3.5" />
          </button>
        </div>
      </div>
      <p className="mt-2 text-center text-10 leading-snug text-placeholder">
        {t("assistant.galileo.third_party_notice")}
      </p>
    </div>
  );
}
