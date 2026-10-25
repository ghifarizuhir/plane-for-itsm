/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { ReactNode } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { InfoOutline, TickCircleOutline, WarningTriangleOutline } from "@makeplane/propel/icons";
import { cn } from "@plane/utils";

export function DocsSectionHeading({ children }: { children: ReactNode }) {
  return <h2 className="mt-8 mb-3 text-16 font-semibold text-primary">{children}</h2>;
}

export function DocsParagraph({ children }: { children: ReactNode }) {
  return <p className="mt-3 text-13 leading-6 text-secondary">{children}</p>;
}

export function DocsInlineCode({ children }: { children: ReactNode }) {
  return <code className="font-mono rounded bg-layer-1 px-1 py-0.5 text-12 text-primary">{children}</code>;
}

export function DocsCodeBlock({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <div className="mt-4 overflow-hidden rounded-lg border border-subtle">
      {title && (
        <div className="border-b border-subtle bg-layer-1 px-4 py-2 text-11 font-medium text-tertiary">{title}</div>
      )}
      <pre className="font-mono overflow-x-auto bg-surface-2 p-4 text-12 leading-6 text-secondary">
        <code>{children}</code>
      </pre>
    </div>
  );
}

type TCalloutVariant = "info" | "tip" | "warning";

const CALLOUT_STYLES: Record<TCalloutVariant, { container: string; icon: ReactNode }> = {
  info: {
    container: "border-subtle bg-layer-1",
    icon: <InfoOutline className="size-4 flex-shrink-0 text-tertiary" />,
  },
  tip: {
    container: "border-subtle bg-success-subtle",
    icon: <TickCircleOutline className="size-4 flex-shrink-0 text-success-primary" />,
  },
  warning: {
    container: "border-subtle bg-warning-subtle",
    icon: <WarningTriangleOutline className="size-4 flex-shrink-0 text-warning-primary" />,
  },
};

export function DocsCallout({
  variant = "info",
  title,
  children,
}: {
  variant?: TCalloutVariant;
  title?: string;
  children: ReactNode;
}) {
  const styles = CALLOUT_STYLES[variant];
  return (
    <div className={cn("mt-4 flex gap-3 rounded-lg border px-4 py-3", styles.container)}>
      <div className="mt-0.5">{styles.icon}</div>
      <div>
        {title && <p className="text-13 font-semibold text-primary">{title}</p>}
        <div className="text-13 leading-6 text-secondary">{children}</div>
      </div>
    </div>
  );
}

export function DocsStepList({ steps }: { steps: { title: string; description?: string }[] }) {
  return (
    <ol className="mt-4 flex flex-col gap-3">
      {steps.map((step, index) => (
        <li key={step.title} className="flex gap-3">
          <span className="mt-0.5 flex size-6 flex-shrink-0 items-center justify-center rounded-full bg-layer-1 text-11 font-semibold text-secondary">
            {index + 1}
          </span>
          <div>
            <p className="text-13 font-medium text-primary">{step.title}</p>
            {step.description && <p className="mt-0.5 text-13 leading-6 text-secondary">{step.description}</p>}
          </div>
        </li>
      ))}
    </ol>
  );
}

export function DocsFeatureGrid({ children }: { children: ReactNode }) {
  return <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2">{children}</div>;
}

export function DocsFeatureCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="rounded-lg border border-subtle bg-surface-1 p-4">
      <h3 className="text-13 font-semibold text-primary">{title}</h3>
      <div className="mt-1 text-13 leading-6 text-secondary">{children}</div>
    </div>
  );
}

export function DocsTermTable({ terms }: { terms: { term: string; meaning: string }[] }) {
  return (
    <div className="mt-4 overflow-hidden rounded-lg border border-subtle">
      <table className="w-full border-collapse text-left">
        <thead>
          <tr className="bg-layer-1">
            <th className="tracking-wider w-1/3 border-b border-subtle px-4 py-2 text-11 font-semibold text-tertiary uppercase">
              Istilah di app
            </th>
            <th className="tracking-wider border-b border-subtle px-4 py-2 text-11 font-semibold text-tertiary uppercase">
              Artinya
            </th>
          </tr>
        </thead>
        <tbody>
          {terms.map((term) => (
            <tr key={term.term} className="border-b border-subtle last:border-b-0">
              <td className="px-4 py-2.5 align-top text-13 font-medium text-primary">{term.term}</td>
              <td className="px-4 py-2.5 align-top text-13 leading-6 text-secondary">{term.meaning}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export type TPracticeUsage = "primary" | "secondary" | "none";

const PRACTICE_USAGE_STYLES: Record<TPracticeUsage, { symbol: string; className: string; label: string }> = {
  primary: { symbol: "●", className: "text-accent-primary", label: "sering dipakai" },
  secondary: { symbol: "○", className: "text-tertiary", label: "kadang dipakai" },
  none: { symbol: "–", className: "text-placeholder", label: "tidak dipakai" },
};

export function DocsPracticeMatrix({
  columns,
  rows,
  legend = "● sering dipakai · ○ kadang dipakai · – tidak dipakai",
}: {
  columns: string[];
  rows: { practice: string; cells: TPracticeUsage[] }[];
  legend?: string;
}) {
  return (
    <div className="mt-4">
      <div className="overflow-x-auto rounded-lg border border-subtle">
        <table className="w-full border-collapse text-left">
          <thead>
            <tr className="bg-layer-1">
              <th className="tracking-wider border-b border-subtle px-4 py-2 text-11 font-semibold text-tertiary uppercase">
                Praktik
              </th>
              {columns.map((column) => (
                <th
                  key={column}
                  className="tracking-wider border-b border-subtle px-4 py-2 text-center text-11 font-semibold text-tertiary uppercase"
                >
                  {column}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.practice} className="border-b border-subtle last:border-b-0">
                <th scope="row" className="px-4 py-2.5 text-left align-top text-13 font-medium text-primary">
                  {row.practice}
                </th>
                {columns.map((column, index) => {
                  const usage = PRACTICE_USAGE_STYLES[row.cells[index] ?? "none"];
                  return (
                    <td key={column} className="px-4 py-2.5 text-center align-top">
                      <span aria-hidden className={cn("text-13", usage.className)}>
                        {usage.symbol}
                      </span>
                      <span className="sr-only">{usage.label}</span>
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="mt-2 text-11 text-tertiary">{legend}</p>
    </div>
  );
}

export function DocsTopicLink({ slug, children }: { slug: string; children: ReactNode }) {
  const { workspaceSlug } = useParams();
  return (
    <Link
      href={`/${workspaceSlug?.toString() ?? ""}/docs/${slug}`}
      className="font-medium text-accent-primary hover:underline"
    >
      {children}
    </Link>
  );
}
