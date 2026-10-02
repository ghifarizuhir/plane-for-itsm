import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { WarningTriangleOutline } from "@makeplane/propel/icons";

type Props = {
  onRetry: () => void;
};

export function ReleaseLoadErrorState({ onRetry }: Props) {
  const { t } = useTranslation();

  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center">
      <WarningTriangleOutline className="size-8 text-tertiary" />
      <p className="text-sm font-medium text-primary">{t("release.load_error.title")}</p>
      <p className="text-xs text-secondary">{t("release.load_error.description")}</p>
      <Button variant="secondary" size="sm" onClick={onRetry}>
        {t("release.load_error.retry")}
      </Button>
    </div>
  );
}
