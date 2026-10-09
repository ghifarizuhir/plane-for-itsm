import { useParams } from "next/navigation";
import { observer } from "mobx-react";
// plane imports
import type { TIssue } from "@plane/types";
// components
import { WarRoomProperty } from "@/components/war-rooms/war-room-property";

type Props = {
  issue: TIssue;
  onClose: () => void;
  onChange: (issue: TIssue, data: Partial<TIssue>, updates: any) => void;
  disabled: boolean;
};

export const SpreadsheetWarRoomColumn = observer(function SpreadsheetWarRoomColumn(props: Props) {
  const { issue, disabled } = props;
  const { workspaceSlug } = useParams();
  const workspaceSlugString = workspaceSlug?.toString() ?? "";

  return (
    <div className="flex h-11 items-center border-b-[0.5px] border-subtle px-page-x">
      <WarRoomProperty
        workspaceSlug={workspaceSlugString}
        projectId={issue.project_id ?? ""}
        issue={issue}
        disabled={disabled}
      />
    </div>
  );
});
