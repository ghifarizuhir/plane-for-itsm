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
        viewBox="0 0 512 512"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        className="terraline-logo-spinner h-6 w-6 sm:h-11 sm:w-11"
      >
        <rect width="512" height="512" rx="110" fill="#3F76FF" />
        <g className="terraline-logo-mark" fill="#FFFFFF" transform="translate(105.5 164.24) scale(3.5294)">
          <rect width="85" height="14" rx="7" />
          <rect x="10.625" y="19" width="63.75" height="14" rx="7" />
          <rect x="21.25" y="38" width="42.5" height="14" rx="7" />
        </g>
      </svg>
    </div>
  );
}
