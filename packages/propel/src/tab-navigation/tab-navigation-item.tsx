/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { motion, AnimatePresence, useReducedMotion } from "framer-motion";
import { MOTION_TRANSITION } from "@plane/constants";
import { cn } from "../utils";
import type { TTabNavigationItemProps } from "./tab-navigation-types";

export function TabNavigationItem({ children, isActive, className }: TTabNavigationItemProps) {
  const reducedMotion = useReducedMotion();

  return (
    <div
      className={cn(
        "relative z-10 flex items-center gap-2 rounded-md px-2 py-1.5 text-13 font-medium transition-colors",
        isActive ? "text-primary" : "text-secondary hover:bg-layer-transparent-hover hover:text-primary",
        className
      )}
    >
      {reducedMotion ? (
        isActive && <div className="absolute inset-0 -z-10 rounded-md bg-layer-transparent-active" />
      ) : (
        <AnimatePresence>
          {isActive && (
            <motion.div
              className="absolute inset-0 -z-10 rounded-md bg-layer-transparent-active"
              initial={{ opacity: 0, scale: 0.96 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.96 }}
              transition={MOTION_TRANSITION.panel}
            />
          )}
        </AnimatePresence>
      )}
      {children}
    </div>
  );
}

TabNavigationItem.displayName = "TabNavigationItem";
