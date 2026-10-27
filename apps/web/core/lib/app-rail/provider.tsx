/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import React, { useMemo } from "react";
import { observer } from "mobx-react";
import { AppRailVisibilityContext } from "./context";
import type { IAppRailVisibilityContext } from "./types";

interface AppRailVisibilityProviderProps {
  children: React.ReactNode;
  isEnabled?: boolean;
}

export const AppRailVisibilityProvider = observer(function AppRailVisibilityProvider({
  children,
  isEnabled = false,
}: AppRailVisibilityProviderProps) {
  const value: IAppRailVisibilityContext = useMemo(
    () => ({
      isEnabled,
      shouldRenderAppRail: isEnabled,
    }),
    [isEnabled]
  );

  return <AppRailVisibilityContext.Provider value={value}>{children}</AppRailVisibilityContext.Provider>;
});
