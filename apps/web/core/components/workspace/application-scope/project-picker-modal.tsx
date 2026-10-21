/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useState } from "react";
import { observer } from "mobx-react";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Button } from "@plane/propel/button";
import { EModalPosition, EModalWidth, ModalCore } from "@plane/ui";
// components
import { ProjectDropdown } from "@/components/dropdowns/project/dropdown";
// hooks
import { useProject } from "@/hooks/store/use-project";

type Props = {
  isOpen: boolean;
  handleClose: () => void;
  onSelect: (projectId: string) => void;
  projectIds: string[];
};

export const WorkspaceApplicationPickerModal = observer(function WorkspaceApplicationPickerModal(props: Props) {
  const { isOpen, handleClose, onSelect, projectIds } = props;
  // i18n
  const { t } = useTranslation();
  // store hooks
  const { getProjectById } = useProject();
  // states
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) setSelectedProjectId(null);
  }, [isOpen]);

  const selectedProject = selectedProjectId ? getProjectById(selectedProjectId) : undefined;

  return (
    <ModalCore isOpen={isOpen} handleClose={handleClose} position={EModalPosition.CENTER} width={EModalWidth.SM}>
      <div className="flex flex-col gap-4 p-5">
        <h3 className="text-16 font-medium text-primary">{t("common.project")}</h3>
        <ProjectDropdown
          multiple={false}
          value={selectedProjectId}
          onChange={setSelectedProjectId}
          buttonVariant="border-with-text"
          buttonClassName="w-full"
          renderCondition={(projectId) => projectIds.includes(projectId)}
        />
        <div className="flex items-center justify-end gap-2">
          <Button variant="secondary" size="lg" onClick={handleClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            disabled={!selectedProject}
            onClick={() => selectedProjectId && onSelect(selectedProjectId)}
          >
            {t("common.continue")}
          </Button>
        </div>
      </div>
    </ModalCore>
  );
});
