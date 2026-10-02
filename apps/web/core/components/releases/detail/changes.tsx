import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { ISearchIssueResponse } from "@plane/types";
// components
import { ExistingIssuesListModal } from "@/components/core/modals/existing-issues-list-modal";
import { TcbStatusBadge } from "@/components/reviews";
// hooks
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  workspaceSlug: string;
  releaseId: string;
  canWrite: boolean;
  isUnderReview: boolean;
};

export const ReleaseDetailChanges = observer(function ReleaseDetailChanges({
  workspaceSlug,
  releaseId,
  canWrite,
  isUnderReview,
}: Props) {
  const { t } = useTranslation();
  const { getReleaseChanges, linkChanges, unlinkChange } = useRelease();
  const [isPickerOpen, setIsPickerOpen] = useState(false);

  const changes = getReleaseChanges(releaseId) ?? [];
  const linkedIssueIds = changes.map((change) => change.issue_id);

  const handleLink = async (issues: ISearchIssueResponse[]) => {
    try {
      await linkChanges(
        workspaceSlug,
        releaseId,
        issues.map((issue) => issue.id)
      );
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Error!", message: t("release.changes.link_failed") });
    }
  };

  const handleUnlink = async (issueId: string) => {
    try {
      await unlinkChange(workspaceSlug, releaseId, issueId);
    } catch {
      setToast({ type: TOAST_TYPE.ERROR, title: "Error!", message: t("release.changes.unlink_failed") });
    }
  };

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-13 font-semibold text-primary">{t("release.fields.changes")}</h3>
        {canWrite && (
          <Button variant="secondary" size="sm" onClick={() => setIsPickerOpen(true)}>
            {t("release.changes.add")}
          </Button>
        )}
      </div>
      {isUnderReview && (
        <p className="rounded-md bg-warning-subtle px-3 py-2 text-12 text-warning-primary">
          {t("release.changes.scope_warning")}
        </p>
      )}
      {changes.length === 0 ? (
        <p className="text-12 text-secondary">{t("release.changes.empty")}</p>
      ) : (
        <ul className="divide-y divide-subtle rounded-md border border-subtle">
          {changes.map((change) => (
            <li key={change.id} className="flex items-center gap-2 px-3 py-2">
              <a
                href={`/${workspaceSlug}/browse/${change.issue_identifier}/`}
                className="flex min-w-0 flex-1 items-center gap-2 text-13 text-primary hover:underline"
              >
                <span className="font-code text-11 text-tertiary">{change.issue_identifier}</span>
                <span className="truncate">{change.issue_name}</span>
              </a>
              <TcbStatusBadge
                changeIssueId={change.issue_id}
                projectId={change.project_id}
                workspaceSlug={workspaceSlug}
              />
              {canWrite && (
                <button
                  type="button"
                  className="text-11 text-tertiary hover:text-danger-primary"
                  onClick={() => void handleUnlink(change.issue_id)}
                >
                  {t("release.changes.unlink")}
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      <ExistingIssuesListModal
        isOpen={isPickerOpen}
        handleClose={() => setIsPickerOpen(false)}
        workspaceSlug={workspaceSlug}
        searchParams={{}}
        workspaceLevelToggle
        selectedWorkItemIds={linkedIssueIds}
        shouldHideIssue={(issue) => linkedIssueIds.includes(issue.id)}
        handleOnSubmit={handleLink}
      />
    </section>
  );
});
