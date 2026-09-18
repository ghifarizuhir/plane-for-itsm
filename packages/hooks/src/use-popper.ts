/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// oxlint-disable promise/always-return

import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { autoUpdate, computePosition, flip, offset, shift } from "@floating-ui/dom";
import type { Placement, Strategy } from "@floating-ui/dom";

type PopperModifier = {
  name: string;
  enabled?: boolean;
  options?: Record<string, any>;
};

type UsePopperOptions = {
  placement?: Placement | "auto" | "auto-start" | "auto-end" | string;
  strategy?: Strategy;
  modifiers?: PopperModifier[];
};

type ReferenceType = Element | null | undefined;
type PopperType = HTMLElement | null | undefined;

/**
 * Drop-in replacement for `react-popper`'s `usePopper`.
 *
 * Why: `react-popper@2.3.0` is unmaintained and its positioning stays stuck at
 * `{ top: 0, left: 0 }` on React 19 (dropdowns render at the viewport
 * top-left corner). It relies on `ReactDOM.flushSync` inside Popper's write
 * phase, which React 19 refuses from inside lifecycle/effects, so the
 * `updateState` modifier never commits. This hook uses `@floating-ui/dom`
 * (`computePosition` + `autoUpdate`) which is React 19 compatible and keeps
 * the same `{ styles, attributes }` return shape so existing call sites only
 * need an import change.
 *
 * React 19 note: HeadlessUI merges child refs through a callback that returns
 * its internal setter, which React 19 treats as a cleanup function. Re-renders
 * therefore invoke state-setter refs (e.g. `ref={setReferenceElement}`) with
 * `undefined` even though the button is still mounted. `undefined` is never a
 * genuine value here (mount passes the element, real unmount passes `null`),
 * so the effect below retains the last good element across `undefined`
 * flickers to keep positioning stable.
 */
export function usePopper(referenceElement: ReferenceType, popperElement: PopperType, options: UsePopperOptions = {}) {
  // `react-popper` supports `auto*` placements; floating-ui does not.
  // Map them to `bottom-start` and rely on flip+shift to find space,
  // which matches the previous visual behaviour for `CustomMenu`.
  const rawPlacement = (options.placement ?? "bottom") as string;
  const placement = (rawPlacement.startsWith("auto") ? "bottom-start" : rawPlacement) as Placement;
  const strategy = options.strategy ?? ("absolute" as Strategy);

  const middleware = useMemo(() => {
    const list = [];
    const modifiers = options.modifiers ?? [];
    const find = (name: string) => modifiers.find((m) => m.name === name);

    // Preserve declaration order semantics loosely: offset -> flip -> shift,
    // matching how our call sites declare them (offset, flip, preventOverflow).
    const offsetMod = find("offset");
    if (offsetMod?.enabled !== false && offsetMod?.options?.offset !== undefined) {
      const raw = offsetMod.options.offset;
      if (Array.isArray(raw)) {
        const [crossAxis = 0, mainAxis = 0] = raw;
        list.push(offset({ mainAxis, crossAxis }));
      } else if (typeof raw === "number") {
        list.push(offset(raw));
      }
    } else if (offsetMod && offsetMod.enabled !== false) {
      // `offset` declared without options (unlikely) -> small default gap
      // matching the previous `my-1` (4px) visual gap.
      list.push(offset(4));
    }

    const flipMod = find("flip");
    if (flipMod?.enabled !== false && flipMod) {
      list.push(
        flip({
          fallbackPlacements: flipMod.options?.fallbackPlacements,
          padding: flipMod.options?.padding ?? 8,
        })
      );
    } else if (!flipMod) {
      // Default flip so dropdowns mirror when near viewport edges,
      // equivalent to Popper's default flip behaviour.
      list.push(flip({ padding: 8 }));
    }

    const preventMod = find("preventOverflow");
    list.push(
      shift({
        padding: preventMod?.options?.padding ?? 8,
      })
    );

    // Ensure a 4px gap even when call sites declare no offset modifier,
    // matching the old `my-1` margin on the popper element.
    if (!offsetMod) list.unshift(offset(4));

    return list;
    // Stringify modifiers to keep memo stable without deep-compare dep.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(options.modifiers ?? []), placement, strategy]);

  const [isPositioned, setIsPositioned] = useState(false);
  const [data, setData] = useState<{
    styles: { popper: React.CSSProperties };
    attributes: { popper: Record<string, string> };
  }>(() => ({
    styles: {
      popper: {
        position: strategy,
        top: "0",
        left: "0",
        visibility: "hidden",
      } as React.CSSProperties,
    },
    attributes: {
      popper: {
        "data-popper-placement": placement,
      },
    },
  }));

  const refCache = useRef<{ reference: Element | null; popper: HTMLElement | null }>({
    reference: null,
    popper: null,
  });

  useLayoutEffect(() => {
    // Retain the last good element across React 19 `undefined` ref flickers
    // (see note above). `null` still clears, for genuine unmounts.
    if (referenceElement !== undefined) refCache.current.reference = referenceElement;
    if (popperElement !== undefined) refCache.current.popper = popperElement;
    const referenceEl = refCache.current.reference;
    const popperEl = refCache.current.popper;
    if (!referenceEl || !popperEl) return;

    const update = () => {
      computePosition(referenceEl, popperEl, {
        placement,
        strategy,
        middleware,
      })
        .then(({ x, y, placement: computedPlacement, strategy: computedStrategy }) => {
          setIsPositioned(true);
          setData({
            styles: {
              popper: {
                position: computedStrategy,
                top: "0",
                left: "0",
                transform: `translate3d(${Math.round(x)}px, ${Math.round(y)}px, 0)`,
                willChange: "transform",
              } as React.CSSProperties,
            },
            attributes: {
              popper: {
                "data-popper-placement": computedPlacement,
              },
            },
          });
        })
        .catch((err) => {
          console.warn("[usePopper] computePosition failed, revealing popper at last known position", err);
          setIsPositioned(true);
          setData((prev) => ({
            ...prev,
            styles: { popper: { ...prev.styles.popper, visibility: "visible" } },
          }));
        });
    };

    update();
    return autoUpdate(referenceEl, popperEl, update);
  }, [referenceElement, popperElement, placement, strategy, middleware]);

  return {
    styles: data.styles,
    attributes: data.attributes,
    isPositioned,
    state: null,
    update: () => Promise.resolve(null),
    forceUpdate: () => null,
  };
}
