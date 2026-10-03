import { useState } from "react";
import { observer } from "mobx-react";
import Link from "next/link";
// plane imports
import { getReleaseLink, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IReviewSessionDetail } from "@plane/types";
import { renderFormattedDate, renderFormattedTime } from "@plane/utils";
// helpers
import { briefingItemById, isBriefingStale, isBriefingTextMode } from "@/services/review.helpers";
// hooks
import { useReview } from "@/hooks/store/use-review";

type Props = {
  workspaceSlug: string;
  session: IReviewSessionDetail;
  canManage: boolean;
};

export const SessionBriefing = observer(function SessionBriefing({ workspaceSlug, session, canManage }: Props) {
  const { t, currentLocale } = useTranslation();
  const { generateBriefing } = useReview();
  const [isGenerating, setIsGenerating] = useState(false);

  const briefing = session.briefing;
  const canGenerate = canManage && session.status === "scheduled";
  // oxlint-disable-next-line unicorn/no-array-sort -- ES2022 target; sorting a fresh copied list
  const items = [...session.items].sort((a, b) => a.position - b.position);
  const briefByItem = briefingItemById(briefing);
  const isTextMode = isBriefingTextMode(briefing);
  const isStale = isBriefingStale(briefing, items.length);

  const handleGenerate = async () => {
    setIsGenerating(true);
    try {
      await generateBriefing(workspaceSlug, session.id, currentLocale);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Success!", message: t("review.briefing.generated") });
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("review.briefing.generate_failed"),
      });
    } finally {
      setIsGenerating(false);
    }
  };

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <h3 className="text-13 font-semibold text-primary">{t("review.briefing.title")}</h3>
          <span className="rounded-full bg-layer-2 px-2 py-0.5 text-10 font-medium text-tertiary">
            {t("review.briefing.ai_badge")}
          </span>
        </div>
        {canGenerate && (
          <Button variant="secondary" size="sm" loading={isGenerating} onClick={() => void handleGenerate()}>
            {briefing ? t("review.briefing.regenerate") : t("review.briefing.generate")}
          </Button>
        )}
      </div>
      {isGenerating && <p className="text-12 text-secondary">{t("review.briefing.generating")}</p>}
      {!briefing && !isGenerating && (
        <p className="text-12 text-secondary">
          {canManage ? t("review.briefing.empty") : t("review.briefing.empty_readonly")}
        </p>
      )}
      {briefing && (
        <div className="space-y-3">
          <p className="text-11 text-tertiary">
            {t("review.briefing.generated_at", {
              date: `${renderFormattedDate(briefing.generated_at)} ${renderFormattedTime(briefing.generated_at)}`,
              name: briefing.generated_by_name,
              model: briefing.model,
            })}
          </p>
          <p className="text-11 text-tertiary">{t("review.briefing.disclaimer")}</p>
          {isTextMode && <p className="text-11 text-warning-primary">{t("review.briefing.text_mode_notice")}</p>}
          {briefing.skipped_items > 0 && (
            <p className="text-11 text-warning-primary">
              {t("review.briefing.skipped_notice", { count: briefing.skipped_items })}
            </p>
          )}
          {isStale && canGenerate && (
            <p className="text-11 text-warning-primary">{t("review.briefing.stale_notice")}</p>
          )}
          {briefing.overall && (
            <div className="space-y-1">
              <h4 className="text-12 font-medium text-primary">{t("review.briefing.overall")}</h4>
              <p className="text-12 whitespace-pre-wrap text-secondary">{briefing.overall}</p>
            </div>
          )}
          {!isTextMode && items.length > 0 && (
            <ul className="space-y-2">
              {items.map((item) => {
                const brief = briefByItem.get(item.id);
                const label = [item.subject.identifier, item.subject.name].filter(Boolean).join(" ");
                const facts = item.facts;
                const subjectHref =
                  item.subject.kind === "change" && item.subject.identifier
                    ? `/${workspaceSlug}/browse/${item.subject.identifier}`
                    : item.subject.kind === "release" && facts.release_id
                      ? getReleaseLink(workspaceSlug, facts.release_id)
                      : null;
                const warRoomHref =
                  facts.war_room && facts.project_id
                    ? getWarRoomLink(workspaceSlug, facts.project_id, facts.war_room.id)
                    : null;
                return (
                  <li key={item.id} className="space-y-2 rounded-md border border-subtle px-3 py-2">
                    <div className="flex flex-wrap items-center gap-2 text-11 text-tertiary">
                      {subjectHref ? (
                        <Link href={subjectHref} className="text-13 text-accent-primary hover:underline">
                          {label}
                        </Link>
                      ) : (
                        <span className="text-13 text-primary">{label}</span>
                      )}
                      {item.subject.kind === "change" && (
                        <>
                          {facts.priority && <span>· {facts.priority}</span>}
                          {facts.state_name && <span>· {facts.state_name}</span>}
                          {(facts.assignees?.length ?? 0) > 0 && <span>· {facts.assignees?.join(", ")}</span>}
                          {facts.target_date && <span>· {facts.target_date}</span>}
                          {facts.war_room && warRoomHref && (
                            <Link href={warRoomHref} className="text-accent-primary hover:underline">
                              · {facts.war_room.name}
                            </Link>
                          )}
                        </>
                      )}
                      {item.subject.kind === "release" && (
                        <>
                          {facts.status && <span>· {facts.status}</span>}
                          {facts.target_date && <span>· {facts.target_date}</span>}
                        </>
                      )}
                    </div>
                    {brief ? (
                      <div className="space-y-1">
                        {brief.summary && <p className="text-12 text-secondary">{brief.summary}</p>}
                        {brief.discussion_points.length > 0 && (
                          <div>
                            <p className="text-11 font-medium text-primary">{t("review.briefing.discussion_points")}</p>
                            <ul className="list-inside list-disc text-11 text-secondary">
                              {brief.discussion_points.map((point) => (
                                <li key={point}>{point}</li>
                              ))}
                            </ul>
                          </div>
                        )}
                        {brief.risks.length > 0 && (
                          <div>
                            <p className="text-11 font-medium text-primary">{t("review.briefing.risks")}</p>
                            <ul className="list-inside list-disc text-11 text-secondary">
                              {brief.risks.map((risk) => (
                                <li key={risk}>{risk}</li>
                              ))}
                            </ul>
                          </div>
                        )}
                      </div>
                    ) : (
                      <p className="text-11 text-tertiary">{t("review.briefing.item_no_ai")}</p>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
    </section>
  );
});
