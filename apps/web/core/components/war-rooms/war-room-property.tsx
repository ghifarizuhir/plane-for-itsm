import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { Tooltip } from "@makeplane/propel/components/tooltip";
import { WAR_ROOM_STATUS_CONFIG, getWarRoomLink } from "@plane/constants";
import { useTranslation } from "@plane/i18n";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import type { TIssue } from "@plane/types";
import { cn } from "@plane/utils";
// helpers
import { isActiveWarRoomStatus } from "@/services/war-room.helpers";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWarRoom } from "@/hooks/store/use-war-room";
import { useWorkItemType } from "@/hooks/store/use-work-item-type";
import { useAppRouter } from "@/hooks/use-app-router";

type Props = {
  workspaceSlug: string;
  projectId: string;
  issue: TIssue;
  disabled?: boolean;
  readOnly?: boolean;
};

export const WarRoomProperty = observer(function WarRoomProperty(props: Props) {
  const { workspaceSlug, projectId, issue, disabled = false, readOnly = false } = props;
  // router
  const router = useAppRouter();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { toggleWarRoom } = useWarRoom();
  const { workItemTypes, fetchWorkItemTypes } = useWorkItemType();
  const { fetchedMap, workItemLinkMap } = useService();
  // states
  const [isToggling, setIsToggling] = useState(false);

  useEffect(() => {
    if (workItemTypes) return;
    void fetchWorkItemTypes(workspaceSlug).catch(() => undefined);
  }, [workItemTypes, workspaceSlug, fetchWorkItemTypes]);

  const issueType = workItemTypes?.find((type) => type.id === issue.type_id);
  const isIncident = issueType?.name.toLowerCase() === "incident";
  if (!isIncident || issue.archived_at) return null;

  const roomStatus = issue.war_room_status ?? null;
  const isOn = roomStatus ? isActiveWarRoomStatus(roomStatus) : false;
  const isEditable = !disabled && !readOnly && !issue.archived_at;
  const statusConfig = roomStatus ? WAR_ROOM_STATUS_CONFIG[roomStatus] : null;

  const serviceIds = fetchedMap[projectId]
    ? Object.values(workItemLinkMap)
        .filter((link) => link.issue_id === issue.id && link.project_id === projectId)
        .map((link) => link.service_id)
    : [];

  const handleToggle = async () => {
    if (!isEditable || isToggling) return;
    setIsToggling(true);
    try {
      await toggleWarRoom(workspaceSlug, projectId, issue, { serviceIds });
    } catch (error) {
      const apiError = error as { error?: string; war_room_id?: string };
      if (apiError?.error !== "active_war_room_exists") {
        setToast({
          type: TOAST_TYPE.ERROR,
          title: t("toast.error"),
          message: t("war_room.errors.generic"),
        });
      }
    } finally {
      setIsToggling(false);
    }
  };

  const handleOpenRoom = () => {
    if (!issue.war_room_id) return;
    router.push(getWarRoomLink(workspaceSlug, projectId, issue.war_room_id));
  };

  return (
    <div className="flex items-center gap-1.5">
      <Tooltip label={isOn ? t("war_room.toggle.turn_off") : t("war_room.toggle.turn_on")} disabled={!isEditable}>
        <button
          type="button"
          role="switch"
          aria-checked={isOn}
          aria-label={t("war_room.property_label")}
          disabled={!isEditable || isToggling}
          onClick={handleToggle}
          className={cn(
            "relative inline-flex h-4 w-7 flex-shrink-0 items-center rounded-full transition-colors",
            isOn ? "bg-danger-primary" : "bg-layer-3",
            !isEditable && "cursor-not-allowed opacity-60"
          )}
        >
          <span
            className={cn(
              "inline-block size-3 rounded-full bg-white transition-transform",
              isOn ? "translate-x-3.5" : "translate-x-0.5"
            )}
          />
        </button>
      </Tooltip>
      {isOn && roomStatus && statusConfig && issue.war_room_severity && (
        <span className={cn("rounded-sm px-1.5 py-0.5 text-11 font-medium", statusConfig.pill)}>
          {t(`war_room.severity_values.${issue.war_room_severity}`)} · {t(statusConfig.label_key)}
        </span>
      )}
      {isOn && issue.war_room_id && !readOnly && (
        <button
          type="button"
          onClick={handleOpenRoom}
          className="text-11 font-medium text-accent-primary hover:underline"
        >
          {t("war_room.open")}
        </button>
      )}
      {readOnly && !isOn && <span className="text-11 text-tertiary">—</span>}
    </div>
  );
});
