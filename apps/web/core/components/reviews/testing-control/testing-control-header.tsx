import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// plane imports
import { useTranslation } from "@plane/i18n";
import { CheckDoneOutline } from "@makeplane/propel/icons";
import { Breadcrumbs, Header } from "@plane/ui";
// components
import { BreadcrumbLink } from "@/components/common/breadcrumb-link";

export const TestingControlHeader = observer(function TestingControlHeader() {
  const { workspaceSlug, projectId } = useParams();
  const { t } = useTranslation();

  return (
    <Header>
      <Header.LeftItem>
        <Breadcrumbs>
          <Breadcrumbs.Item
            component={
              <BreadcrumbLink
                label={t("sidebar.testing_control")}
                href={`/${workspaceSlug}/projects/${projectId}/testing-control`}
                icon={<CheckDoneOutline className="h-4 w-4 text-tertiary" />}
                isLast
              />
            }
            isLast
          />
        </Breadcrumbs>
      </Header.LeftItem>
    </Header>
  );
});
