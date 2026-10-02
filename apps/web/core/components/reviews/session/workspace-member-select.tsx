import { useEffect } from "react";
import { observer } from "mobx-react";
import { Avatar } from "@makeplane/propel/components/avatar";
// plane imports
import { CustomSearchSelect } from "@plane/ui";
import { getFileURL } from "@plane/utils";
// hooks
import { useMember } from "@/hooks/store/use-member";

type Props = {
  workspaceSlug: string;
  value: string;
  onChange: (userId: string) => void;
  excludeUserIds: string[];
  isDisabled?: boolean;
  placeholder: string;
};

export const WorkspaceMemberSelect = observer(function WorkspaceMemberSelect({
  workspaceSlug,
  value,
  onChange,
  excludeUserIds,
  isDisabled = false,
  placeholder,
}: Props) {
  const {
    workspace: { workspaceMemberIds, getWorkspaceMemberDetails, fetchWorkspaceMembers },
  } = useMember();

  useEffect(() => {
    if (workspaceMemberIds) return;
    void fetchWorkspaceMembers(workspaceSlug).catch(() => undefined);
  }, [workspaceMemberIds, fetchWorkspaceMembers, workspaceSlug]);

  const options =
    workspaceMemberIds
      ?.map((userId) => {
        const details = getWorkspaceMemberDetails(userId);
        if (!details?.member) return null;
        return {
          value: userId,
          query: details.member.display_name,
          content: (
            <div className="flex items-center gap-2">
              <Avatar
                alt={details.member.display_name}
                fallback={details.member.display_name?.[0]?.toUpperCase()}
                src={getFileURL(details.member.avatar_url)}
                size="xs"
              />
              {details.member.display_name}
            </div>
          ),
        };
      })
      .filter((option): option is NonNullable<typeof option> => Boolean(option))
      .filter((option) => !excludeUserIds.includes(option.value)) ?? [];

  return (
    <CustomSearchSelect
      value={value}
      onChange={onChange}
      options={options}
      label={value || placeholder}
      disabled={isDisabled}
      buttonClassName="!px-3 !py-2 bg-surface-1"
    />
  );
});
