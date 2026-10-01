/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useMatches } from "react-router";

type TFullscreenHandle = { fullscreen?: boolean };

/**
 * True when any matched route module exports `handle.fullscreen = true`.
 * Fullscreen routes opt out of the app chrome (top navigation, app rail and
 * project sidebars) so dedicated pages such as the war room render edge to edge.
 */
export const useFullscreenRoute = (): boolean =>
  useMatches().some((match) => (match.handle as TFullscreenHandle | undefined)?.fullscreen === true);
