/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { memo, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { observer } from "mobx-react";
import { useStore, type Node } from "@xyflow/react";
import { useParams } from "next/navigation";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  ChevronRightOutline,
  ClockOutline,
  CloseOutline,
  LockOutline,
  NewTabArrowOutline,
} from "@makeplane/propel/icons";
// plane imports
import { useTranslation } from "@plane/i18n";
import { Tabs } from "@plane/propel/tabs";
import type {
  ILiteService,
  IService,
  TServiceDependencyType,
  TServiceHealth,
  TServiceWorkItemLink,
} from "@plane/types";
import { ControlLink } from "@plane/ui";
import { calculateTimeAgo, calculateTimeAgoShort, cn } from "@plane/utils";
// components
import { RichTextEditor } from "@/components/editor/rich-text";
// hooks
import { useAppRouter } from "@/hooks/use-app-router";
import { useService } from "@/hooks/store/use-service";
// helpers
import { isLiteService } from "@/services/service.helpers";
// local imports
import { DEFAULT_HEALTH, HEALTH_CONFIG } from "../health/health-config";
import { ServiceHealthDot } from "../health/service-health-dot";
import { SERVICE_KIND_ICON } from "../kind-config";
import { SERVICE_NODE_HEIGHT, SERVICE_NODE_WIDTH } from "./use-graph-layout";

export const FOCUS_CARD_WIDTH = 360;
export const FOCUS_RAIL_WIDTH = FOCUS_CARD_WIDTH + 24;
/** Below this canvas width the inspector overlays instead of taking a rail. */
export const FOCUS_RAIL_MIN_CANVAS_WIDTH = 1100;
const EDGE_INSET = 12;
const COMPACT_BREAKPOINT = 640;
const ESTIMATED_HEIGHT = 300;

const CRITICALITY_TONE: Record<string, string> = {
  critical: "border-danger-strong/60 bg-danger-subtle text-danger-primary",
  high: "border-warning-strong/50 bg-warning-subtle text-warning-primary",
  medium: "border-subtle bg-layer-2 text-secondary",
  low: "border-subtle bg-layer-2 text-tertiary",
};

export type TFocusRelationship = {
  id: string;
  direction: "out" | "in";
  type: TServiceDependencyType;
  service: IService | ILiteService;
  health?: TServiceHealth;
};

type TFocusTab = "description" | "relationships" | "work_items";

const RELATIONSHIP_LABEL_KEYS: Record<TServiceDependencyType, { out: string; in: string }> = {
  depends_on: { out: "service.detail.depends_on", in: "service.relationships.required_by" },
  uses: { out: "service.relationships.uses", in: "service.relationships.used_by" },
  runs_on: { out: "service.relationships.runs_on", in: "service.relationships.hosts" },
  connects_to: { out: "service.relationships.connects_to", in: "service.relationships.connected_from" },
  replicates_to: { out: "service.relationships.replicates_to", in: "service.relationships.replicated_from" },
  other: { out: "service.relationships.related_to", in: "service.relationships.related_to" },
};

type TServiceDescriptionEditorProps = {
  id: string;
  html: string;
  projectId?: string;
  workspaceId: string;
  workspaceSlug: string;
};

/** Read-only editor keeps service descriptions on the same rich-text styles as the rest of the app. */
const ServiceDescriptionEditor = memo(function ServiceDescriptionEditor({
  id,
  html,
  projectId,
  workspaceId,
  workspaceSlug,
}: TServiceDescriptionEditorProps) {
  return (
    <RichTextEditor
      id={id}
      editable={false}
      initialValue={html}
      workspaceId={workspaceId}
      workspaceSlug={workspaceSlug}
      projectId={projectId}
      containerClassName="!border-none !p-0"
      editorClassName="text-12"
      displayConfig={{ fontSize: "small-font" }}
      bubbleMenuEnabled={false}
      dragDropEnabled={false}
    />
  );
});

type TServiceFocusCardProps = {
  node: Node;
  service: IService | ILiteService;
  projectIdentifier?: string | null;
  relationships: TFocusRelationship[];
  health?: TServiceHealth;
  lastDeployedAt?: string | null;
  pinned?: boolean;
  /** Pinned on a wide canvas: reserve a rail instead of covering the graph. */
  rail?: boolean;
  onClose: () => void;
  onFocusService?: (serviceId: string) => void;
};

/**
 * Screen-space inspector for the selected node. It deliberately does not scale
 * with the graph: descriptions must stay readable while the map itself is
 * zoomed out to its overview level.
 */
export const ServiceFocusCard = observer(function ServiceFocusCard({
  node,
  service,
  projectIdentifier,
  relationships,
  health,
  lastDeployedAt,
  pinned,
  rail = false,
  onClose,
  onFocusService,
}: TServiceFocusCardProps) {
  // plane hooks
  const { t } = useTranslation();
  const reducedMotion = useReducedMotion();
  const router = useAppRouter();
  const { workspaceSlug } = useParams<{ workspaceSlug?: string }>();
  const { getWorkItemLinksByService } = useService();
  // react flow store — the card follows the viewport while panning/zooming
  const transform = useStore((state) => state.transform);
  const viewportWidth = useStore((state) => state.width);
  const viewportHeight = useStore((state) => state.height);
  // states
  const [activeTab, setActiveTab] = useState<TFocusTab>("description");
  const [cardHeight, setCardHeight] = useState(0);
  // refs
  const cardRef = useRef<HTMLDivElement>(null);
  // derived values
  const isLite = !("kind" in service);
  const kind = "kind" in service ? service.kind : "other";
  const description = "description" in service ? service.description : "";
  const descriptionHtml = "description_html" in service ? service.description_html : "";
  const hasDescription = description.trim().length > 0;
  const workspaceId = "workspace_id" in service ? service.workspace_id : "";
  const ownerProjectId = "owner_project_id" in service ? service.owner_project_id : null;
  const slug = workspaceSlug ?? "";
  const canRenderRichDescription = hasDescription && descriptionHtml.trim().length > 0 && !!slug;
  const workItemLinks = getWorkItemLinksByService(service.id);
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

  useLayoutEffect(() => {
    const element = cardRef.current;
    if (!element) return;
    const measure = () => setCardHeight(element.offsetHeight);
    measure();
    const resizeObserver = new ResizeObserver(measure);
    resizeObserver.observe(element);
    return () => resizeObserver.disconnect();
  }, [service.id, viewportWidth, viewportHeight]);

  if (viewportWidth === 0 || viewportHeight === 0) return null;

  const zoom = transform[2];
  const nodeWidth = (node.measured?.width ?? SERVICE_NODE_WIDTH) * zoom;
  const nodeHeight = (node.measured?.height ?? SERVICE_NODE_HEIGHT) * zoom;
  const nodeX = node.position.x * zoom + transform[0];
  const nodeY = node.position.y * zoom + transform[1];
  const nodeCenterX = nodeX + nodeWidth / 2;
  const nodeCenterY = nodeY + nodeHeight / 2;
  const nodeOffscreen =
    nodeX + nodeWidth < 0 || nodeX > viewportWidth || nodeY + nodeHeight < 0 || nodeY > viewportHeight;

  // A pinned inspector is a workspace, not a tooltip: keep it on screen even
  // when the node itself is panned out of view.
  if (nodeOffscreen && !pinned) return null;

  const isCompact = viewportWidth < COMPACT_BREAKPOINT;
  const isRail = rail && !isCompact;
  const cardWidth = isRail
    ? FOCUS_CARD_WIDTH
    : isCompact
      ? viewportWidth - EDGE_INSET * 2
      : Math.min(FOCUS_CARD_WIDTH, viewportWidth - EDGE_INSET * 2);
  const bottomClearance = EDGE_INSET;
  const maxHeight = Math.max(
    160,
    isCompact
      ? Math.min(viewportHeight * 0.62, viewportHeight - EDGE_INSET * 2)
      : viewportHeight - EDGE_INSET - bottomClearance
  );
  const compactHeight = Math.min(cardHeight > 0 ? cardHeight : ESTIMATED_HEIGHT, maxHeight);
  // The rail is a workspace, not a tooltip: it uses the full height available.
  const height = isCompact ? compactHeight : maxHeight;
  // A rail sits outside the graph area; otherwise dock on the edge opposite
  // the node so the selection and its immediate relationships stay uncovered.
  const placeOnLeft = !isCompact && !isRail && nodeCenterX > viewportWidth / 2;
  const left = isRail ? viewportWidth + EDGE_INSET : placeOnLeft ? EDGE_INSET : viewportWidth - cardWidth - EDGE_INSET;
  const top = isCompact
    ? Math.max(EDGE_INSET, viewportHeight - EDGE_INSET - height)
    : Math.min(
        Math.max(nodeCenterY - height / 2, EDGE_INSET),
        Math.max(EDGE_INSET, viewportHeight - bottomClearance - height)
      );

  const showLeader = !isCompact && !isRail && !nodeOffscreen;
  const leaderAnchorX = placeOnLeft ? left + cardWidth : left;
  const leaderAnchorY = Math.min(Math.max(nodeCenterY, top + 20), top + height - 20);
  const leaderTargetX = placeOnLeft ? nodeX : nodeX + nodeWidth;
  const leaderTargetY = nodeCenterY;
  const leaderDirection = placeOnLeft ? 1 : -1;
  const leaderBend = Math.max(26, Math.abs(leaderTargetX - leaderAnchorX) * 0.3);
  const leaderPath = `M ${leaderAnchorX} ${leaderAnchorY} C ${leaderAnchorX + leaderDirection * leaderBend} ${leaderAnchorY}, ${leaderTargetX - leaderDirection * leaderBend} ${leaderTargetY}, ${leaderTargetX} ${leaderTargetY}`;
  const enterOffset = isCompact ? { y: 8 } : { x: placeOnLeft ? -8 : 8 };
  const exitOffset = isCompact ? { y: 6 } : { x: placeOnLeft ? -6 : 6 };

  const renderRelationship = (relationship: TFocusRelationship) => {
    const related = relationship.service;
    const relatedIsLite = isLiteService(related);
    const relatedKind = "kind" in related ? related.kind : "other";
    const RelatedIcon = relatedIsLite ? LockOutline : SERVICE_KIND_ICON[relatedKind];
    const relatedAccent = `var(--kind-${relatedKind.replace(/_/g, "-")})`;

    return (
      <button
        key={relationship.id}
        type="button"
        onClick={() => onFocusService?.(related.id)}
        className="group flex w-full items-center gap-2.5 rounded-md border border-subtle bg-layer-1/40 px-2.5 py-2 text-left transition-colors hover:border-strong hover:bg-layer-2 focus-visible:ring-2 focus-visible:ring-accent-strong focus-visible:outline-none"
      >
        <span
          className="grid size-7 shrink-0 place-items-center rounded-md border"
          style={{
            borderColor: `color-mix(in oklab, ${relatedAccent} 30%, transparent)`,
            backgroundColor: `color-mix(in oklab, ${relatedAccent} 10%, transparent)`,
            color: relatedAccent,
          }}
        >
          <RelatedIcon className="h-3.5 w-3.5" />
        </span>
        <span className="min-w-0 flex-1">
          <span className={cn("block truncate text-12 font-medium", relatedIsLite ? "text-tertiary" : "text-primary")}>
            {related.name}
          </span>
          <span className="mt-0.5 flex min-w-0 items-center gap-1.5 text-10 text-tertiary">
            <span aria-hidden className="font-code">
              {relationship.direction === "out" ? "→" : "←"}
            </span>
            <span className="truncate">{t(RELATIONSHIP_LABEL_KEYS[relationship.type][relationship.direction])}</span>
            {relatedIsLite ? (
              <span className="truncate">· {t("service.lite.restricted")}</span>
            ) : (
              <>
                <span aria-hidden>·</span>
                <ServiceHealthDot health={relationship.health ?? "unknown"} className="h-1.5 w-1.5 shrink-0" />
                <span className="truncate capitalize">{t(`service.status_values.${related.status}`)}</span>
              </>
            )}
          </span>
        </span>
        {onFocusService && (
          <ChevronRightOutline className="h-3 w-3 shrink-0 text-placeholder transition-colors group-hover:text-tertiary" />
        )}
      </button>
    );
  };

  const renderWorkItem = (link: TServiceWorkItemLink) => {
    const href = slug ? `/${slug}/projects/${link.project_id}/issues/${link.issue_id}` : "#";

    return (
      <ControlLink
        key={link.id}
        href={href}
        onClick={() => slug && router.push(href)}
        className="group flex w-full items-start gap-2 rounded-md border border-subtle bg-layer-1/40 px-2.5 py-2 transition-colors hover:border-strong hover:bg-layer-2 focus-visible:ring-2 focus-visible:ring-accent-strong focus-visible:outline-none"
      >
        <span className="mt-0.5 shrink-0 rounded-sm border border-subtle bg-layer-2 px-1.5 py-0.5 font-code text-9 font-semibold tracking-[0.08em] text-secondary uppercase">
          {link.issue_identifier ?? t("service.detail.untitled")}
        </span>
        <span className="min-w-0 flex-1 text-12 leading-snug text-primary">
          {link.issue_name ?? t("service.detail.untitled")}
        </span>
        <NewTabArrowOutline className="mt-0.5 h-3 w-3 shrink-0 text-placeholder transition-colors group-hover:text-tertiary" />
      </ControlLink>
    );
  };

  return (
    <>
      {showLeader && (
        <svg
          aria-hidden
          className="pointer-events-none absolute inset-0 z-[9] overflow-visible"
          width={viewportWidth}
          height={viewportHeight}
        >
          <motion.path
            d={leaderPath}
            fill="none"
            stroke={accent}
            strokeWidth={1.25}
            strokeLinecap="round"
            strokeDasharray="2 5"
            initial={reducedMotion ? false : { pathLength: 0, opacity: 0 }}
            animate={{ pathLength: 1, opacity: pinned ? 0.38 : 0.62 }}
            transition={{ duration: reducedMotion ? 0 : 0.32, ease: "easeOut" }}
          />
          <motion.circle
            cx={leaderTargetX}
            cy={leaderTargetY}
            r={2.5}
            fill={accent}
            style={{ transformBox: "fill-box", transformOrigin: "center" }}
            initial={reducedMotion ? false : { opacity: 0, scale: 0.6 }}
            animate={{ opacity: pinned ? 0.5 : 0.8, scale: 1 }}
            transition={{ duration: reducedMotion ? 0 : 0.2, ease: "easeOut" }}
          />
        </svg>
      )}

      <div
        ref={cardRef}
        className="absolute z-20"
        style={{ top, left: isRail ? `calc(100% + ${EDGE_INSET}px)` : left, width: cardWidth }}
        role="region"
        aria-label={service.name}
      >
        <motion.div
          className="service-focus-card relative flex flex-col overflow-hidden rounded-xl border border-strong bg-surface-1"
          style={
            {
              height: isCompact ? undefined : height,
              maxHeight,
              transformOrigin: isCompact ? "50% 100%" : placeOnLeft ? "0% 50%" : "100% 50%",
              "--focus-accent": accent,
            } as CSSProperties
          }
          initial={reducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, ...enterOffset }}
          animate={{ opacity: 1, scale: 1, x: 0, y: 0 }}
          exit={
            reducedMotion
              ? { opacity: 0 }
              : { opacity: 0, scale: 0.98, ...exitOffset, transition: { duration: 0.12, ease: "easeOut" } }
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
            className="relative shrink-0 px-4 pt-4 pb-3"
            initial={reducedMotion ? false : { opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0, duration: 0.18, ease: "easeOut" }}
          >
            <div className="flex items-start gap-3">
              <span
                className="grid h-10 w-10 shrink-0 place-items-center rounded-xl border"
                style={{
                  borderColor: `color-mix(in oklab, ${accent} 36%, transparent)`,
                  backgroundColor: `color-mix(in oklab, ${accent} 12%, transparent)`,
                  color: accent,
                  boxShadow: `inset 0 1px 0 color-mix(in oklab, ${accent} 22%, transparent)`,
                }}
              >
                {isLite ? <LockOutline className="h-4 w-4" /> : <KindIcon className="h-4 w-4" />}
              </span>

              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-0.5 font-code text-9 font-medium tracking-[0.14em] text-tertiary uppercase">
                  {projectIdentifier && (
                    <>
                      <span className="max-w-full truncate whitespace-nowrap" style={{ color: accent }}>
                        {projectIdentifier}
                      </span>
                      <span aria-hidden className="text-placeholder">
                        /
                      </span>
                    </>
                  )}
                  <span className="whitespace-nowrap">{isLite ? t("service.lite.restricted") : kindLabel}</span>
                  {!isLite && typeLabel && (
                    <>
                      <span aria-hidden className="text-placeholder">
                        /
                      </span>
                      <span className="whitespace-nowrap">{typeLabel}</span>
                    </>
                  )}
                </div>
                <h2
                  className={cn(
                    "mt-1.5 line-clamp-2 text-[17px] leading-[1.3] font-semibold tracking-[-0.015em]",
                    isLite ? "text-tertiary" : "text-primary"
                  )}
                  title={service.name}
                >
                  <span className="service-focus-name">{service.name}</span>
                </h2>
              </div>

              <div className="flex shrink-0 items-start gap-0.5">
                <AnimatePresence initial={false}>
                  {pinned && (
                    <motion.span
                      initial={{ opacity: 0, scale: 0.9 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0, scale: 0.9 }}
                      transition={{ duration: 0.14, ease: "easeOut" }}
                      className="mt-1 inline-flex items-center gap-1 font-code text-9 font-medium tracking-[0.12em] text-tertiary uppercase"
                      title={t("pin")}
                    >
                      <span aria-hidden className="h-1 w-1 rounded-full bg-[var(--focus-accent)]" />
                      {t("pin")}
                    </motion.span>
                  )}
                </AnimatePresence>

                <button
                  type="button"
                  onClick={onClose}
                  aria-label={t("close")}
                  title={t("close")}
                  className="-mt-1 -mr-1.5 grid h-7 w-7 place-items-center rounded-md text-tertiary transition-colors hover:bg-layer-transparent-hover hover:text-primary focus-visible:ring-2 focus-visible:ring-accent-strong focus-visible:outline-none"
                >
                  <CloseOutline className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          </motion.header>

          <motion.div
            className="relative flex shrink-0 flex-wrap items-center gap-1.5 px-4 pb-3"
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

          <motion.div
            className="flex min-h-0 flex-1 flex-col"
            initial={reducedMotion ? false : { opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.05, duration: 0.18, ease: "easeOut" }}
          >
            <Tabs
              value={activeTab}
              onValueChange={(value) => setActiveTab(value as TFocusTab)}
              className="flex min-h-0 flex-1 flex-col"
            >
              <div className="shrink-0 border-y border-subtle px-3 py-2">
                <Tabs.List className="h-8 gap-0.5 rounded-md p-0.5">
                  <Tabs.Trigger value="description" size="sm" className="h-7 min-w-0 gap-1 px-1.5">
                    <span className="truncate">{t("service.fields.description")}</span>
                  </Tabs.Trigger>
                  <Tabs.Trigger value="relationships" size="sm" className="h-7 min-w-0 gap-1 px-1.5">
                    <span className="truncate">{t("service.relationships.title")}</span>
                    {relationships.length > 0 && (
                      <span className="font-code text-9 text-tertiary tabular-nums">{relationships.length}</span>
                    )}
                  </Tabs.Trigger>
                  <Tabs.Trigger value="work_items" size="sm" className="h-7 min-w-0 gap-1 px-1.5">
                    <span className="truncate">{t("service.tabs.work_items")}</span>
                    {workItemLinks.length > 0 && (
                      <span className="font-code text-9 text-tertiary tabular-nums">{workItemLinks.length}</span>
                    )}
                  </Tabs.Trigger>
                </Tabs.List>
              </div>

              <Tabs.Content
                value="description"
                keepMounted
                className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto px-4 pt-3 pb-4"
              >
                {canRenderRichDescription ? (
                  <ServiceDescriptionEditor
                    id={`${service.id}-focus-description`}
                    html={descriptionHtml}
                    projectId={ownerProjectId ?? undefined}
                    workspaceId={workspaceId}
                    workspaceSlug={slug}
                  />
                ) : hasDescription ? (
                  <p className="text-12 leading-[1.6] break-words whitespace-pre-wrap text-secondary">{description}</p>
                ) : (
                  <p className="text-12 text-tertiary">{t("none")}</p>
                )}
              </Tabs.Content>

              <Tabs.Content
                value="relationships"
                keepMounted
                className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto p-3"
              >
                {relationships.length === 0 ? (
                  <p className="text-12 text-tertiary">{t("service.relationships.empty")}</p>
                ) : (
                  <div className="flex flex-col gap-1.5">{relationships.map(renderRelationship)}</div>
                )}
              </Tabs.Content>

              <Tabs.Content
                value="work_items"
                keepMounted
                className="vertical-scrollbar min-h-0 flex-1 overflow-y-auto p-3"
              >
                {workItemLinks.length === 0 ? (
                  <p className="text-12 text-tertiary">{t("service.detail.no_work_items")}</p>
                ) : (
                  <div className="flex flex-col gap-1.5">{workItemLinks.map(renderWorkItem)}</div>
                )}
              </Tabs.Content>
            </Tabs>
          </motion.div>

          {lastDeployedAt && (
            <motion.footer
              className="mt-auto flex shrink-0 items-center gap-2 border-t border-subtle px-4 py-2"
              initial={reducedMotion ? false : { opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.07, duration: 0.18, ease: "easeOut" }}
            >
              <span
                className="inline-flex shrink-0 items-center gap-1 font-code text-10 text-tertiary tabular-nums"
                title={`${t("service.fields.deploy")}: ${deployedAgo}`}
              >
                <ClockOutline className="h-3 w-3 shrink-0" />
                <span className="font-medium text-secondary">{t("service.fields.deploy")}</span>
                <span>{deployedAgoShort}</span>
              </span>
            </motion.footer>
          )}
        </motion.div>
      </div>
    </>
  );
});
