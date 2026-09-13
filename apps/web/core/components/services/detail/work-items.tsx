/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { Input } from "@makeplane/propel/components/input";
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
// hooks
import { useService } from "@/hooks/store/use-service";
import { useWorkspace } from "@/hooks/store/use-workspace";

type Props = {
  serviceId: string;
};

export const ServiceWorkItems = observer(function ServiceWorkItems(props: Props) {
  const { serviceId } = props;
  // states
  const [issueId, setIssueId] = useState("");
  const [isLinking, setIsLinking] = useState(false);
  // router
  const { workspaceSlug, projectId } = useParams();
  // plane hooks
  const { t } = useTranslation();
  // store hooks
  const { getServiceById, getWorkItemLinksByService, linkWorkItem, unlinkWorkItem } = useService();
  const { currentWorkspace } = useWorkspace();
  // derived values
  const service = getServiceById(serviceId);
  if (!service) return null;
  const slug = workspaceSlug?.toString();
  const pid = projectId?.toString() ?? service.project_id;
  const workspaceId = currentWorkspace?.id;
  const links = getWorkItemLinksByService(serviceId);

  // Minimal add flow (links by issue id); a full issue picker is out of scope.
  const handleLink = async () => {
    const id = issueId.trim();
    if (!slug || !workspaceId || !pid || !id) return;
    setIsLinking(true);
    try {
      await linkWorkItem(slug, workspaceId, pid, serviceId, { id });
      setIssueId("");
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: t("service.detail.link_work_item_error"),
      });
    } finally {
      setIsLinking(false);
    }
  };

  const handleUnlink = async (linkId: string) => {
    if (!slug || !workspaceId || !pid) return;
    try {
      await unlinkWorkItem(slug, workspaceId, pid, linkId);
    } catch {
      setToast({
        type: TOAST_TYPE.ERROR,
        title: t("error"),
        message: t("service.detail.unlink_work_item_error"),
      });
    }
  };

  return (
    <div className="flex max-w-3xl flex-col gap-3">
      <div className="flex items-center gap-2">
        <div className="w-full max-w-xs">
          <Input
            size="lg"
            id="service-work-item-id"
            name="service-work-item-id"
            type="text"
            value={issueId}
            onChange={(e) => setIssueId(e.target.value)}
            placeholder={t("service.detail.issue_id_placeholder")}
          />
        </div>
        <Button variant="primary" size="sm" onClick={handleLink} disabled={!issueId.trim()} loading={isLinking}>
          {t("add")}
        </Button>
      </div>
      {links.length === 0 ? (
        <p className="text-13 text-tertiary">{t("service.detail.no_work_items")}</p>
      ) : (
        <div className="flex flex-col gap-1.5">
          {links.map((link) => (
            <div
              key={link.id}
              className="flex items-center justify-between gap-2 rounded-md border border-subtle px-3 py-2"
            >
              <div className="flex min-w-0 flex-col">
                <span className="text-13 font-medium text-primary">{link.issue_identifier ?? link.issue_id}</span>
                <span
                  className="truncate text-12 text-secondary"
                  title={link.issue_name ?? t("service.detail.untitled")}
                >
                  {link.issue_name ?? t("service.detail.untitled")}
                </span>
              </div>
              <button
                type="button"
                onClick={() => handleUnlink(link.id)}
                className="shrink-0 text-12 text-tertiary hover:text-primary"
              >
                {t("remove")}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
});
