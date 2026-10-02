import { observer } from "mobx-react";
import Link from "next/link";
// plane imports
import { getReleaseLink } from "@plane/constants";
import { ArrowExpandOutline } from "@makeplane/propel/icons";
import { renderFormattedDate } from "@plane/utils";
// components
import { ReleaseStatusPill } from "./release-status-pill";
// hooks
import { useRelease } from "@/hooks/store/use-release";

type Props = {
  releaseId: string;
  workspaceSlug: string;
};

export const ReleaseBoardRow = observer(function ReleaseBoardRow({ releaseId, workspaceSlug }: Props) {
  const { getReleaseById } = useRelease();
  const release = getReleaseById(releaseId);

  if (!release) return null;
  const href = getReleaseLink(workspaceSlug, release.id);

  return (
    <Link
      href={href}
      className="group flex cursor-pointer items-center gap-3 border-b border-subtle px-4 py-3 hover:bg-layer-1"
    >
      <span className="w-16 shrink-0 font-code text-11 text-tertiary">REL-{release.sequence_id}</span>
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-13 font-medium text-primary">{release.name}</span>
        {release.version && <span className="text-11 text-tertiary">{release.version}</span>}
      </div>
      <span className="flex w-24 justify-center">
        <ReleaseStatusPill status={release.status} />
      </span>
      <span className="w-28 text-right text-12 text-secondary">
        {release.target_date ? renderFormattedDate(release.target_date) : "—"}
      </span>
      <ArrowExpandOutline className="h-3.5 w-3.5 text-tertiary opacity-0 group-hover:opacity-100" />
    </Link>
  );
});
