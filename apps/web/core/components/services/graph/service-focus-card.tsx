/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useEffect, useId, useState, type CSSProperties } from "react";
import { useStore, type Node } from "@xyflow/react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  ChevronDownOutline,
  ClockOutline,
  CloseOutline,
  LinkOutline,
  LockOutline,
  NewTabArrowOutline,
} from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import type { ILiteService, IService, TServiceHealth } from "@plane/types";
import { calculateTimeAgo, calculateTimeAgoShort, cn } from "@plane/utils";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceHealthDot } from "../health/service-health-dot";
import { SERVICE_KIND_ICON } from "../kind-config";
import { SERVICE_NODE_HEIGHT, SERVICE_NODE_WIDTH } from "./use-graph-layout";

const CARD_WIDTH = 348;
const VIEWPORT_INSET = 12;
const ANCHOR_GAP = 10;
const DESCRIPTION_PREVIEW_THRESHOLD = 120;

const CRITICALITY_TONE: Record<string, string> = {
  critical: "border-danger-strong/60 bg-danger-subtle text-danger-primary",
  high: "border-warning-strong/50 bg-warning-subtle text-warning-primary",
  medium: "border-subtle bg-layer-2 text-secondary",
  low: "border-subtle bg-layer-2 text-tertiary",
};

type TServiceFocusCardProps = {
  node: Node;
  service: IService | ILiteService;
  relationshipCount: number;
  health?: TServiceHealth;
  lastDeployedAt?: string | null;
  pinned?: boolean;
  onClose: () => void;
  onOpen?: () => void;
  onPointerEnter?: () => void;
  onPointerLeave?: () => void;
};

/**
 * Screen-space inspector for the selected node. It deliberately does not scale
 * with the graph: descriptions must stay readable while the map itself is
 * zoomed out to its overview level.
 */
export function ServiceFocusCard({
  node,
  service,
  relationshipCount,
  health,
  lastDeployedAt,
  pinned,
  onClose,
  onOpen,
  onPointerEnter,
  onPointerLeave,
}: TServiceFocusCardProps) {
  // plane hooks
  const { t } = useTranslation();
  const reducedMotion = useReducedMotion();
  // ids
  const descriptionId = useId();
  // react flow store — the card follows the viewport while panning/zooming
  const transform = useStore((state) => state.transform);
  const viewportWidth = useStore((state) => state.width);
  const viewportHeight = useStore((state) => state.height);
  // states
  const [expanded, setExpanded] = useState(false);
  // derived values
  const isLite = !("kind" in service);
  const kind = "kind" in service ? service.kind : "other";
  const description = "description" in service ? service.description : "";
  const hasDescription = description.trim().length > 0;
  const isLongDescription = description.trim().length > DESCRIPTION_PREVIEW_THRESHOLD;
  const state = health ?? DEFAULT_HEALTH;
  const healthConfig = HEALTH_CONFIG[state];
  const typeLabel = "type" in service ? t(`service.type_values.${service.type}`) : null;
  const kindLabel = t(`service.kind_values.${kind}`);
  const statusLabel = t(`service.status_values.${service.status}`);
  const criticalityLabel = t(`service.criticality_values.${service.criticality}`);
  const deployedAgo = lastDeployedAt ? calculateTimeAgo(lastDeployedAt) : "";
  const deployedAgoShort = lastDeployedAt ? calculateTimeAgoShort(lastDeployedAt) : "";
  const accent = `var(--kind-${kind.replace(/_/g, "-")})`;
  const KindIcon = SERVICE_KIND_ICON[kind];

  useEffect(() => {
    setExpanded(false);
  }, [service.id]);

  if (viewportWidth === 0 || viewportHeight === 0) return null;

  const zoom = transform[2];
  const nodeWidth = (node.measured?.width ?? SERVICE_NODE_WIDTH) * zoom;
  const nodeHeight = (node.measured?.height ?? SERVICE_NODE_HEIGHT) * zoom;
  const nodeX = node.position.x * zoom + transform[0];
  const nodeY = node.position.y * zoom + transform[1];

  // Hide the inspector while its node is panned decisively off-screen.
  if (nodeX + nodeWidth < 0 || nodeX > viewportWidth || nodeY + nodeHeight < 0 || nodeY > viewportHeight) return null;

  const cardWidth = Math.min(CARD_WIDTH, viewportWidth - VIEWPORT_INSET * 2);
  const left = Math.min(
    Math.max(nodeX + nodeWidth / 2 - cardWidth / 2, VIEWPORT_INSET),
    viewportWidth - cardWidth - VIEWPORT_INSET
  );
  const placeAbove = nodeY + nodeHeight / 2 > viewportHeight * 0.55;
  const availableSpace = placeAbove
    ? nodeY - VIEWPORT_INSET - ANCHOR_GAP
    : viewportHeight - nodeY - nodeHeight - VIEWPORT_INSET - ANCHOR_GAP;
  const maxHeight = Math.max(96, availableSpace);
  const position = placeAbove
    ? { bottom: viewportHeight - nodeY + ANCHOR_GAP, left }
    : { top: nodeY + nodeHeight + ANCHOR_GAP, left };
  const pointerLeft = Math.min(Math.max(nodeX + nodeWidth / 2 - left, 20), cardWidth - 20);

  return (
    <div
      className="absolute z-20"
      style={{ ...position, width: cardWidth }}
      role="region"
      aria-label={service.name}
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
    >
      <motion.div
        className="service-focus-card relative flex flex-col overflow-hidden rounded-xl border border-strong bg-surface-1"
        style={
          {
            maxHeight,
            transformOrigin: `${pointerLeft}px ${placeAbove ? "bottom" : "top"}`,
            "--focus-accent": accent,
          } as CSSProperties
        }
        initial={reducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: placeAbove ? 8 : -8 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={
          reducedMotion
            ? { opacity: 0 }
            : { opacity: 0, scale: 0.98, y: placeAbove ? 6 : -6, transition: { duration: 0.12, ease: "easeOut" } }
        }
        transition={reducedMotion ? { duration: 0.12 } : { type: "spring", stiffness: 380, damping: 30, mass: 0.7 }}
      >
        <span
          aria-hidden
          className="pointer-events-none absolute inset-x-0 top-0 h-px opacity-70"
          style={{ backgroundImage: `linear-gradient(90deg, transparent, ${accent}, transparent)` }}
        />
        <span
          aria-hidden
          className="pointer-events-none absolute inset-x-0 top-0 h-24 opacity-[0.09]"
          style={{ backgroundImage: `radial-gradient(120% 100% at 18% 0%, ${accent}, transparent 68%)` }}
        />

        <motion.header
          className="relative flex items-start gap-3 px-3.5 pt-3.5 pb-3"
          initial={reducedMotion ? false : { opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0, duration: 0.18, ease: "easeOut" }}
        >
          <span
            className="grid h-9 w-9 shrink-0 place-items-center rounded-[10px] border"
            style={{
              borderColor: `color-mix(in oklab, ${accent} 36%, transparent)`,
              backgroundColor: `color-mix(in oklab, ${accent} 12%, transparent)`,
              color: accent,
            }}
          >
            {isLite ? <LockOutline className="h-4 w-4" /> : <KindIcon className="h-4 w-4" />}
          </span>

          <div className="min-w-0 flex-1 pt-0.5">
            <h2
              className={cn(
                "truncate text-13 font-semibold tracking-[-0.01em]",
                isLite ? "text-tertiary" : "text-primary"
              )}
              title={service.name}
            >
              {service.name}
            </h2>
            <p className="mt-0.5 flex min-w-0 items-center gap-1.5 text-10 font-medium text-tertiary">
              <span className="truncate">{isLite ? t("service.lite.restricted") : kindLabel}</span>
              {!isLite && (
                <>
                  <span aria-hidden className="text-placeholder">
                    /
                  </span>
                  <span className="truncate">{typeLabel}</span>
                </>
              )}
            </p>
          </div>

          <AnimatePresence initial={false}>
            {pinned && (
              <motion.span
                initial={{ opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.9 }}
                transition={{ duration: 0.14, ease: "easeOut" }}
                className="inline-flex shrink-0 items-center gap-1 self-center font-code text-9 font-medium tracking-[0.12em] text-tertiary uppercase"
                title={t("common.pin")}
              >
                <span aria-hidden className="h-1 w-1 rounded-full bg-[var(--focus-accent)]" />
                {t("common.pin")}
              </motion.span>
            )}
          </AnimatePresence>

          <button
            type="button"
            onClick={onClose}
            aria-label={t("common.close")}
            title={t("common.close")}
            className="-mt-0.5 -mr-1 grid h-7 w-7 shrink-0 place-items-center rounded-md text-tertiary transition-colors hover:bg-layer-transparent-hover hover:text-primary focus-visible:ring-2 focus-visible:ring-accent-strong focus-visible:outline-none"
          >
            <CloseOutline className="h-3.5 w-3.5" />
          </button>
        </motion.header>

        <motion.div
          className="relative flex flex-wrap items-center gap-1.5 px-3.5 pb-3"
          initial={reducedMotion ? false : { opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.03, duration: 0.18, ease: "easeOut" }}
        >
          <span
            className={cn(
              "inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-10 font-medium",
              healthConfig.pill
            )}
          >
            <ServiceHealthDot health={state} className={cn("h-1.5 w-1.5", state === "down" && "animate-pulse")} />
            {t(healthConfig.label_key)}
          </span>
          <span className="inline-flex items-center rounded-full border border-subtle bg-layer-2 px-2 py-0.5 text-10 font-medium text-secondary">
            {statusLabel}
          </span>
          <span
            className={cn(
              "inline-flex items-center rounded-full border px-2 py-0.5 font-code text-9 font-semibold tracking-[0.08em] uppercase",
              CRITICALITY_TONE[service.criticality] ?? CRITICALITY_TONE.medium
            )}
          >
            {criticalityLabel}
          </span>
        </motion.div>

        {hasDescription && (
          <motion.div
            className="flex min-h-0 flex-1 flex-col border-t border-subtle"
            initial={reducedMotion ? false : { opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.05, duration: 0.18, ease: "easeOut" }}
          >
            <div className="flex items-center justify-between gap-2 px-3.5 pt-2">
              <span className="font-code text-9 font-medium tracking-[0.14em] text-tertiary uppercase">
                {t("service.fields.description")}
              </span>
              {isLongDescription && (
                <button
                  type="button"
                  onClick={() => setExpanded((value) => !value)}
                  aria-expanded={expanded}
                  aria-controls={descriptionId}
                  className="inline-flex items-center gap-1 rounded-sm px-1 py-0.5 text-10 font-medium text-secondary transition-colors hover:text-primary focus-visible:ring-2 focus-visible:ring-accent-strong focus-visible:outline-none"
                >
                  {expanded ? t("common.show_less") : t("common.show_all")}
                  <motion.span
                    aria-hidden
                    animate={{ rotate: expanded ? 180 : 0 }}
                    transition={reducedMotion ? { duration: 0 } : { type: "spring", stiffness: 420, damping: 30 }}
                    className="shrink-0"
                  >
                    <ChevronDownOutline className="h-3 w-3" />
                  </motion.span>
                </button>
              )}
            </div>
            <div
              className={cn("relative px-3.5 pt-1 pb-3", expanded && "vertical-scrollbar min-h-0 overflow-y-auto")}
              style={expanded ? { maxHeight: Math.max(72, maxHeight - 164) } : undefined}
            >
              <p
                id={descriptionId}
                className={cn(
                  "text-12 leading-[1.6] break-words whitespace-pre-wrap text-secondary",
                  !expanded && isLongDescription && "line-clamp-3"
                )}
              >
                {description}
              </p>
              {!expanded && isLongDescription && (
                <span
                  aria-hidden
                  className="pointer-events-none absolute inset-x-3.5 bottom-3 h-7 bg-gradient-to-t from-surface-1 to-transparent"
                />
              )}
            </div>
          </motion.div>
        )}

        <motion.footer
          className="mt-auto flex shrink-0 items-center gap-2 border-t border-subtle px-3 py-2"
          initial={reducedMotion ? false : { opacity: 0, y: 4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.07, duration: 0.18, ease: "easeOut" }}
        >
          <span
            className="inline-flex min-w-0 items-center gap-1.5 text-10 text-tertiary"
            title={t("service.relationships.title")}
          >
            <LinkOutline className="h-3 w-3 shrink-0" />
            <span className="font-code font-medium text-secondary tabular-nums">{relationshipCount}</span>
            <span className="truncate">{t("service.relationships.title")}</span>
          </span>
          {lastDeployedAt && (
            <span
              className="inline-flex shrink-0 items-center gap-1 font-code text-10 text-tertiary tabular-nums"
              title={`${t("service.fields.deploy")}: ${deployedAgo}`}
            >
              <ClockOutline className="h-3 w-3 shrink-0" />
              {deployedAgoShort}
            </span>
          )}
          {onOpen && (
            <button
              type="button"
              onClick={onOpen}
              aria-label={`${t("common.preview")}: ${service.name}`}
              className="ml-auto inline-flex shrink-0 items-center gap-1 rounded-md border border-subtle bg-layer-2 px-2 py-1 text-10 font-medium text-secondary transition-colors hover:border-strong hover:bg-layer-2-hover hover:text-primary focus-visible:ring-2 focus-visible:ring-accent-strong focus-visible:outline-none"
            >
              {t("common.preview")}
              <NewTabArrowOutline className="h-3 w-3" />
            </button>
          )}
        </motion.footer>
      </motion.div>

      <span
        aria-hidden
        className={cn(
          "absolute h-3 w-3 rotate-45 border-strong bg-surface-1",
          placeAbove ? "-bottom-1.5 border-r border-b" : "-top-1.5 border-t border-l"
        )}
        style={{ left: pointerLeft - 6 }}
      />
    </div>
  );
}
