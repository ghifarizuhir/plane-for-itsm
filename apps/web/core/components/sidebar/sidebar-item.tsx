/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import Link from "next/link";
import { cn } from "@plane/utils";

// ============================================================================
// TYPES
// ============================================================================

interface AppSidebarItemData {
  href?: string;
  label?: string;
  icon?: React.ReactNode;
  isActive?: boolean;
  onClick?: () => void;
  disabled?: boolean;
  showLabel?: boolean;
}

interface AppSidebarItemProps {
  variant?: "link" | "button";
  item?: AppSidebarItemData;
  size?: "sm" | "md";
}

interface AppSidebarItemLabelProps {
  highlight?: boolean;
  label?: string;
  size?: "sm" | "md";
}

interface AppSidebarItemIconProps {
  icon?: React.ReactNode;
  highlight?: boolean;
  size?: "sm" | "md";
}

interface AppSidebarLinkItemProps {
  href?: string;
  isActive?: boolean;
  ariaLabel?: string;
  title?: string;
  children: React.ReactNode;
  className?: string;
}

interface AppSidebarButtonItemProps {
  ariaLabel?: string;
  title?: string;
  children: React.ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  className?: string;
}

// ============================================================================
// STYLES
// ============================================================================

const styles = {
  base: "group flex w-full flex-col items-center justify-center gap-0.5 rounded-md text-tertiary focus-visible:ring-2 focus-visible:ring-accent-strong focus-visible:outline-none",
  icon: "flex items-center justify-center gap-2 rounded-md",
  iconSm: "size-7 [&_svg]:size-4.5",
  iconMd: "size-8",
  iconActive: "bg-layer-transparent-selected text-icon-primary",
  iconInactive: "text-icon-tertiary group-hover:bg-layer-transparent-hover group-hover:text-icon-secondary",
  label: "max-w-full truncate font-medium",
  labelSm: "text-10 leading-3",
  labelMd: "text-11",
  labelActive: "text-secondary",
  labelInactive: "group-hover:text-secondary text-tertiary",
} as const;

// ============================================================================
// SUB-COMPONENTS
// ============================================================================

function AppSidebarItemLabel({ highlight = false, label, size = "md" }: AppSidebarItemLabelProps) {
  if (!label) return null;

  return (
    <span
      className={cn(styles.label, size === "sm" ? styles.labelSm : styles.labelMd, {
        [styles.labelActive]: highlight,
        [styles.labelInactive]: !highlight,
      })}
    >
      {label}
    </span>
  );
}

function AppSidebarItemIcon({ icon, highlight, size = "md" }: AppSidebarItemIconProps) {
  if (!icon) return null;

  return (
    <div
      className={cn(styles.icon, size === "sm" ? styles.iconSm : styles.iconMd, {
        [styles.iconActive]: highlight,
        [styles.iconInactive]: !highlight,
      })}
    >
      {icon}
    </div>
  );
}

function AppSidebarLinkItem({ href, isActive, ariaLabel, title, children, className }: AppSidebarLinkItemProps) {
  if (!href) return null;

  return (
    <Link
      href={href}
      aria-label={ariaLabel}
      aria-current={isActive ? "page" : undefined}
      title={title}
      className={cn(styles.base, className)}
    >
      {children}
    </Link>
  );
}

function AppSidebarButtonItem({
  ariaLabel,
  title,
  children,
  onClick,
  disabled = false,
  className,
}: AppSidebarButtonItemProps) {
  return (
    <button
      className={cn(styles.base, className)}
      aria-label={ariaLabel}
      title={title}
      onClick={onClick}
      disabled={disabled}
      type="button"
    >
      {children}
    </button>
  );
}

// ============================================================================
// MAIN COMPONENT
// ============================================================================

export type AppSidebarItemComponent = React.FC<AppSidebarItemProps> & {
  Label: React.FC<AppSidebarItemLabelProps>;
  Icon: React.FC<AppSidebarItemIconProps>;
  Link: React.FC<AppSidebarLinkItemProps>;
  Button: React.FC<AppSidebarButtonItemProps>;
};

function AppSidebarItem({ variant = "link", item, size = "md" }: AppSidebarItemProps) {
  if (!item) return null;

  const { icon, isActive, label, href, onClick, disabled, showLabel = true } = item;

  const commonItems = (
    <>
      <AppSidebarItemIcon icon={icon} highlight={isActive} size={size} />
      {showLabel && <AppSidebarItemLabel highlight={isActive} label={label} size={size} />}
    </>
  );

  if (variant === "link") {
    return (
      <AppSidebarLinkItem href={href} isActive={isActive} ariaLabel={label} title={showLabel ? undefined : label}>
        {commonItems}
      </AppSidebarLinkItem>
    );
  }

  return (
    <AppSidebarButtonItem ariaLabel={label} title={showLabel ? undefined : label} onClick={onClick} disabled={disabled}>
      {commonItems}
    </AppSidebarButtonItem>
  );
}

// ============================================================================
// COMPOUND COMPONENT ASSIGNMENT
// ============================================================================

AppSidebarItem.Label = AppSidebarItemLabel;
AppSidebarItem.Icon = AppSidebarItemIcon;
AppSidebarItem.Link = AppSidebarLinkItem;
AppSidebarItem.Button = AppSidebarButtonItem;

export { AppSidebarItem };
export type { AppSidebarItemData, AppSidebarItemProps };
