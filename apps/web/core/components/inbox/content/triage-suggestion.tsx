/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TInboxIssueTriageField } from "@plane/types";
// stores
import type { IInboxIssueStore } from "@/store/inbox/inbox-issue.store";

type Props = {
  inboxIssue: IInboxIssueStore;
};

const percent = (value: number) => `${Math.round(value * 100)}%`;

export const InboxIssueTriageSuggestion = observer(function InboxIssueTriageSuggestion(props: Props) {
  const { inboxIssue } = props;
  // store hooks
  const { t } = useTranslation();
  const suggestion = inboxIssue.triageSuggestion;

  useEffect(() => {
    if (!inboxIssue.triageSuggestionFetched) void inboxIssue.fetchTriageSuggestion();
  }, [inboxIssue]);

  const runAction = async (action: "apply" | "dismiss", fields: TInboxIssueTriageField[]) => {
    try {
      if (action === "apply") await inboxIssue.applyTriageSuggestion(fields);
      else await inboxIssue.dismissTriageSuggestion(fields);
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("inbox_issue.triage.error_title"),
        message: t("inbox_issue.triage.error_message"),
      });
    }
  };

  if (!suggestion) return null;

  if (suggestion.status === "pending")
    return (
      <div className="mb-4 rounded-md border border-subtle bg-layer-1 px-3 py-2 text-13 text-tertiary">
        {t("inbox_issue.triage.pending")}
      </div>
    );

  const applied = suggestion.applied_fields;
  const dismissed = suggestion.dismissed_fields;

  return (
    <div className="mb-4 rounded-md border border-subtle bg-layer-1 px-3 py-2">
      <div className="mb-2 flex items-center justify-between gap-2">
        <h5 className="text-body-sm-medium">{t("inbox_issue.triage.title")}</h5>
        {suggestion.model && (
          <span className="text-11 text-tertiary">
            {t("inbox_issue.triage.suggested_by", { model: suggestion.model })}
          </span>
        )}
      </div>

      <div className="divide-y-2 divide-subtle-1">
        {suggestion.category && (
          <div className="flex items-center justify-between gap-2 py-2">
            <div className="flex flex-col">
              <span className="text-13 text-tertiary">{t("inbox_issue.triage.category")}</span>
              <span className="text-13 text-primary">
                {suggestion.category.label}{" "}
                <span className="text-tertiary">({percent(suggestion.category.confidence)})</span>
              </span>
            </div>
            {dismissed.includes("category") ? (
              <span className="text-11 text-tertiary">{t("inbox_issue.triage.dismissed")}</span>
            ) : (
              <button
                type="button"
                className="text-11 text-tertiary hover:text-primary"
                onClick={() => void runAction("dismiss", ["category"])}
              >
                {t("inbox_issue.triage.dismiss")}
              </button>
            )}
          </div>
        )}

        {suggestion.severity && (
          <div className="flex items-center justify-between gap-2 py-2">
            <div className="flex flex-col">
              <span className="text-13 text-tertiary">{t("inbox_issue.triage.severity")}</span>
              <span className="text-13 text-primary">
                {suggestion.severity.priority}{" "}
                <span className="text-tertiary">
                  ({suggestion.severity.score.toFixed(1)} / 4, {percent(suggestion.severity.confidence)})
                </span>
              </span>
            </div>
            {applied.includes("severity") ? (
              <span className="text-11 text-tertiary">{t("inbox_issue.triage.applied")}</span>
            ) : dismissed.includes("severity") ? (
              <span className="text-11 text-tertiary">{t("inbox_issue.triage.dismissed")}</span>
            ) : (
              <div className="flex items-center gap-3">
                <button
                  type="button"
                  className="text-11 text-accent-primary hover:underline"
                  onClick={() => void runAction("apply", ["severity"])}
                >
                  {t("inbox_issue.triage.apply")}
                </button>
                <button
                  type="button"
                  className="text-11 text-tertiary hover:text-primary"
                  onClick={() => void runAction("dismiss", ["severity"])}
                >
                  {t("inbox_issue.triage.dismiss")}
                </button>
              </div>
            )}
          </div>
        )}

        {suggestion.needs_human && (
          <div className="flex items-center justify-between gap-2 py-2">
            <div className="flex flex-col">
              <span className="text-13 text-tertiary">{t("inbox_issue.triage.needs_human")}</span>
              <span className="text-13 text-primary">
                {suggestion.needs_human.probability >= 0.5 ? t("inbox_issue.triage.yes") : t("inbox_issue.triage.no")}{" "}
                <span className="text-tertiary">({percent(suggestion.needs_human.probability)})</span>
              </span>
            </div>
            {dismissed.includes("needs_human") ? (
              <span className="text-11 text-tertiary">{t("inbox_issue.triage.dismissed")}</span>
            ) : (
              <button
                type="button"
                className="text-11 text-tertiary hover:text-primary"
                onClick={() => void runAction("dismiss", ["needs_human"])}
              >
                {t("inbox_issue.triage.dismiss")}
              </button>
            )}
          </div>
        )}
      </div>

      <p className="mt-2 text-11 text-tertiary">{t("inbox_issue.triage.disclaimer")}</p>
    </div>
  );
});
