/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { ModalCore } from "@plane/ui";
import { useAiSchedules } from "@/hooks/store/use-ai-schedules";
import {
  humanizeSchedule,
  validateScheduleBasics,
  validateScheduleSpec,
  type TAiSchedule,
  type TAiScheduleFrequency,
  type TAiScheduleTool,
  type TAiScheduleUpdate,
} from "@/lib/ai-schedule";
import { ScheduleRecipeFields } from "../ai/assistant-sidebar/schedule-recipe-fields";

type Props = {
  schedule: TAiSchedule;
  isOpen: boolean;
  onClose: () => void;
};

type TDraft = {
  name: string;
  frequency: TAiScheduleFrequency;
  time: string;
  timezone: string;
  day_of_week: number | null;
  day_of_month: number | null;
  description: string;
  how_to: string[];
  tools: TAiScheduleTool[];
  expected_output: string;
  prompt: string;
};

const fromSchedule = (schedule: TAiSchedule): TDraft => ({
  name: schedule.name,
  frequency: schedule.frequency,
  time: schedule.time,
  timezone: schedule.timezone,
  day_of_week: schedule.day_of_week ?? (schedule.frequency === "weekly" ? 1 : null),
  day_of_month: schedule.day_of_month ?? (schedule.frequency === "monthly" ? 1 : null),
  description: schedule.spec?.description ?? "",
  how_to: schedule.spec?.how_to ?? [],
  tools: schedule.spec?.tools ?? [],
  expected_output: schedule.spec?.expected_output ?? "",
  prompt: schedule.prompt,
});

const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

export const ScheduleEditModal = observer(function ScheduleEditModal({ schedule, isOpen, onClose }: Props) {
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const { updateSchedule } = useAiSchedules();
  // The parent mounts this modal only while it is open, so the draft is
  // initialized once per open and the 30s list poll cannot wipe edits.
  const [draft, setDraft] = useState<TDraft>(() => fromSchedule(schedule));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const structured = Boolean(schedule.spec);
  const patch = (fields: Partial<TDraft>) => setDraft((current) => ({ ...current, ...fields }));

  const basicsError = validateScheduleBasics(draft);
  const recipeError = structured
    ? validateScheduleSpec({
        version: 1,
        description: draft.description,
        how_to: draft.how_to,
        tools: draft.tools,
        expected_output: draft.expected_output,
      })
    : !draft.prompt.trim()
      ? "Prompt is required."
      : draft.prompt.length > 2000
        ? "Prompt must be at most 2000 characters."
        : null;
  const validationError = basicsError ?? recipeError;

  const save = async () => {
    if (!slug || validationError) return;
    setSaving(true);
    setError(null);
    const common = {
      name: draft.name.trim(),
      frequency: draft.frequency,
      time: draft.time,
      timezone: draft.timezone.trim(),
      day_of_week: draft.frequency === "weekly" ? (draft.day_of_week ?? 1) : null,
      day_of_month: draft.frequency === "monthly" ? (draft.day_of_month ?? 1) : null,
    };
    const changes: TAiScheduleUpdate = structured
      ? {
          ...common,
          description: draft.description,
          how_to: draft.how_to,
          tools: draft.tools,
          expected_output: draft.expected_output,
        }
      : { ...common, prompt: draft.prompt };
    try {
      await updateSchedule(slug, schedule.id, changes);
      setToast({ type: TOAST_TYPE.SUCCESS, title: "Schedule updated" });
      onClose();
    } catch (err: any) {
      setError(err?.data?.error ?? "Could not update the schedule.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <ModalCore isOpen={isOpen} handleClose={onClose}>
      <div className="max-h-[80vh] space-y-2 overflow-y-auto p-5">
        <h3 className="text-base font-semibold text-primary">Edit schedule</h3>
        <p className="text-xs text-tertiary">{humanizeSchedule(draft)}</p>

        <label className="block text-12 text-secondary">
          Name
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.name}
            onChange={(event) => patch({ name: event.target.value })}
          />
        </label>

        <div className="grid grid-cols-2 gap-2">
          <label className="text-12 text-secondary">
            Frequency
            <select
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={draft.frequency}
              onChange={(event) => patch({ frequency: event.target.value as TAiScheduleFrequency })}
            >
              <option value="hourly">Hourly</option>
              <option value="daily">Daily</option>
              <option value="weekly">Weekly</option>
              <option value="monthly">Monthly</option>
            </select>
          </label>
          <label className="text-12 text-secondary">
            Time
            <input
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={draft.time}
              onChange={(event) => patch({ time: event.target.value })}
              placeholder="09:00"
            />
          </label>
        </div>

        {draft.frequency === "weekly" && (
          <label className="block text-12 text-secondary">
            Weekday
            <select
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={draft.day_of_week ?? 1}
              onChange={(event) => patch({ day_of_week: Number(event.target.value) })}
            >
              {WEEKDAYS.map((label, index) => (
                <option key={label} value={index + 1}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        )}

        {draft.frequency === "monthly" && (
          <label className="block text-12 text-secondary">
            Day of month
            <input
              type="number"
              min={1}
              max={31}
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={draft.day_of_month ?? 1}
              onChange={(event) => patch({ day_of_month: Number(event.target.value) })}
            />
          </label>
        )}

        <label className="block text-12 text-secondary">
          Timezone (IANA)
          <input
            className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
            value={draft.timezone}
            onChange={(event) => patch({ timezone: event.target.value })}
            placeholder="Asia/Jakarta"
          />
        </label>

        {structured ? (
          <ScheduleRecipeFields key={schedule.id} value={draft} onChange={patch} />
        ) : (
          <label className="mt-2 block text-12 text-secondary">
            Prompt (legacy)
            <textarea
              className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              rows={6}
              value={draft.prompt}
              onChange={(event) => patch({ prompt: event.target.value })}
            />
          </label>
        )}

        {validationError && <p className="text-12 text-danger-primary">{validationError}</p>}
        {error && <p className="text-12 text-danger-primary">{error}</p>}
      </div>
      <div className="flex items-center justify-end gap-2 border-t border-subtle p-4">
        <Button size="sm" variant="secondary" disabled={saving} onClick={onClose}>
          Cancel
        </Button>
        <Button
          size="sm"
          variant="primary"
          loading={saving}
          disabled={Boolean(validationError)}
          onClick={() => void save()}
        >
          Save
        </Button>
      </div>
    </ModalCore>
  );
});
