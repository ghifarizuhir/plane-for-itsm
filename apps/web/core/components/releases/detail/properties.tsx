import { useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { RELEASE_STATUS_CONFIG, RELEASE_STATUSES } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { IRelease, TReleaseStatus } from "@plane/types";
import { CustomSelect, Input } from "@plane/ui";
// hooks
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  workspaceSlug: string;
  release: IRelease;
};

export const ReleaseDetailProperties = observer(function ReleaseDetailProperties({ workspaceSlug, release }: Props) {
  const { t } = useTranslation();
  const { updateRelease } = useRelease();
  const [version, setVersion] = useState(release.version ?? "");
  const [targetDate, setTargetDate] = useState(release.target_date ?? "");

  const patchRelease = async (data: Partial<IRelease>) => {
    try {
      await updateRelease(workspaceSlug, release.id, data);
    } catch (error) {
      const apiError = error as { detail?: string; error?: string };
      setToast({
        type: TOAST_TYPE.ERROR,
        title: "Error!",
        message: apiError?.detail ?? apiError?.error ?? t("release.errors.update_failed"),
      });
    }
  };

  return (
    <div className="space-y-4 p-4">
      <div>
        <p className="mb-1 text-11 font-medium text-tertiary">{t("release.fields.status")}</p>
        <CustomSelect
          value={release.status}
          onChange={(value: TReleaseStatus) => void patchRelease({ status: value })}
          label={t(RELEASE_STATUS_CONFIG[release.status].label_key)}
        >
          {RELEASE_STATUSES.map((status) => (
            <CustomSelect.Option key={status} value={status}>
              {t(RELEASE_STATUS_CONFIG[status].label_key)}
            </CustomSelect.Option>
          ))}
        </CustomSelect>
      </div>
      <div>
        <p className="mb-1 text-11 font-medium text-tertiary">{t("release.fields.version")}</p>
        <Input
          value={version}
          onChange={(event) => setVersion(event.target.value)}
          onBlur={() => void patchRelease({ version: version.trim() ? version.trim() : null })}
          placeholder="v1.0.0"
        />
      </div>
      <div>
        <p className="mb-1 text-11 font-medium text-tertiary">{t("release.fields.target_date")}</p>
        <Input
          type="date"
          value={targetDate}
          onChange={(event) => setTargetDate(event.target.value)}
          onBlur={() => void patchRelease({ target_date: targetDate || null })}
        />
      </div>
      <div className="border-t border-subtle pt-3 text-11 text-tertiary">
        <p>REL-{release.sequence_id}</p>
      </div>
    </div>
  );
});
