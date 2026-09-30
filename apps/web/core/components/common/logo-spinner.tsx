/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { PlaneLogoBoxed } from "@plane/propel/icons";

export function LogoSpinner() {
  return (
    <div className="flex items-center justify-center">
      <PlaneLogoBoxed role="status" aria-label="Loading" className="terraline-logo-spinner h-6 w-6 sm:h-11 sm:w-11" />
    </div>
  );
}
