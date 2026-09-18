/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export function LogoSpinner() {
  return (
    <div className="flex items-center justify-center">
      <svg
        role="status"
        aria-label="Loading"
        viewBox="0 0 85 52"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className="terraline-logo-spinner h-6 w-auto text-primary sm:h-11"
      >
        <rect x="0" y="0" width="85" height="14" rx="7" fill="currentColor" />
        <rect x="10.625" y="19" width="63.75" height="14" rx="7" fill="currentColor" />
        <rect x="21.25" y="38" width="42.5" height="14" rx="7" fill="currentColor" />
      </svg>
    </div>
  );
}
