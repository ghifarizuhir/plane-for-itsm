/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export const INCIDENT_FLOW_ID = "incident" as const;

export type TAiFlow = {
  id: typeof INCIDENT_FLOW_ID;
  ref: string;
};

/** Feedback keys the sidebar renders through i18n (`assistant.<key>`). */
export type TFlowFeedbackKey = "incident_ref_required" | "incident_invalid_ref" | "incident_flow_ended";

export type TIncidentCommand =
  | { kind: "start"; ref: string }
  | { kind: "off" }
  | { kind: "missing" }
  | { kind: "invalid" }
  | null;

const REFERENCE_PATTERN = /^[A-Za-z0-9]+-\d+$/;

/** True when the message starts with the `/incident` slash command (case-insensitive). */
export const isIncidentCommand = (text: string): boolean => /^\/incident(?:\s|$)/i.test(text.trimStart());

export const parseIncidentCommand = (text: string): TIncidentCommand => {
  if (!isIncidentCommand(text)) return null;
  const rest = text.trimStart().slice("/incident".length).trim();
  if (!rest) return { kind: "missing" };
  if (rest.toLowerCase() === "off") return { kind: "off" };
  const ref = rest.split(/\s+/)[0];
  return REFERENCE_PATTERN.test(ref) ? { kind: "start", ref } : { kind: "invalid" };
};

/**
 * Restore rule: the newest message carrying a `flow` key wins — non-null with a
 * reference means the flow is active, an explicit null means it was ended.
 */
export const activeFlowFromMessages = (
  messages: { flow?: string | null; flowRef?: string | null }[]
): TAiFlow | undefined => {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.flow !== undefined) {
      return message.flow === INCIDENT_FLOW_ID && message.flowRef
        ? { id: INCIDENT_FLOW_ID, ref: message.flowRef }
        : undefined;
    }
  }
  return undefined;
};
