/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState, type ChangeEventHandler, type ReactNode } from "react";
import { observer } from "mobx-react";
import { useParams } from "next/navigation";
import { ChevronDownOutline } from "@makeplane/propel/icons";
import { Button } from "@plane/propel/button";
import { TOAST_TYPE, setToast } from "@plane/propel/toast";
import { Input, TextArea } from "@plane/ui";
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
  onCancel: () => void;
  onSaved: () => void;
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

const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

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

export const ScheduleEditForm = observer(function ScheduleEditForm({ schedule, onCancel, onSaved }: Props) {
  const { workspaceSlug } = useParams<{ workspaceSlug: string }>();
  const slug = Array.isArray(workspaceSlug) ? workspaceSlug[0] : workspaceSlug;
  const { updateSchedule } = useAiSchedules();
  // The panel mounts this form only while editing, so the draft is initialized
  // once per edit session and the 30s list poll cannot wipe edits.
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
      onSaved();
    } catch (err: any) {
      setError(err?.data?.error ?? "Could not update the schedule.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex min-h-full flex-col">
      <div className="flex-1 space-y-6 px-5 py-4">
        <div>
          <p className="text-11 text-tertiary">{humanizeSchedule(draft)}</p>
        </div>

        <section className="space-y-3">
          <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">Basics</h4>
          <label htmlFor="schedule-edit-name" className="block text-12 text-secondary">
            Name
            <Input
              id="schedule-edit-name"
              className="mt-1"
              value={draft.name}
              onChange={(event) => patch({ name: event.target.value })}
              placeholder="Weekly overdue report"
            />
          </label>
        </section>

        <section className="space-y-3">
          <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">Cadence</h4>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <SelectField
              id="schedule-edit-frequency"
              label="Frequency"
              value={draft.frequency}
              onChange={(event) => patch({ frequency: event.target.value as TAiScheduleFrequency })}
            >
              <option value="hourly">Hourly</option>
              <option value="daily">Daily</option>
              <option value="weekly">Weekly</option>
              <option value="monthly">Monthly</option>
            </SelectField>
            <label htmlFor="schedule-edit-time" className="block text-12 text-secondary">
              Time
              <Input
                id="schedule-edit-time"
                className="mt-1"
                value={draft.time}
                onChange={(event) => patch({ time: event.target.value })}
                placeholder="09:00"
              />
            </label>
            {draft.frequency === "weekly" && (
              <SelectField
                id="schedule-edit-weekday"
                label="Weekday"
                value={String(draft.day_of_week ?? 1)}
                onChange={(event) => patch({ day_of_week: Number(event.target.value) })}
              >
                {WEEKDAYS.map((label, index) => (
                  <option key={label} value={index + 1}>
                    {label}
                  </option>
                ))}
              </SelectField>
            )}
            {draft.frequency === "monthly" && (
              <label htmlFor="schedule-edit-day" className="block text-12 text-secondary">
                Day of month
                <Input
                  id="schedule-edit-day"
                  className="mt-1"
                  type="number"
                  min={1}
                  max={31}
                  value={draft.day_of_month ?? 1}
                  onChange={(event) => patch({ day_of_month: Number(event.target.value) })}
                />
              </label>
            )}
            <label htmlFor="schedule-edit-timezone" className="block text-12 text-secondary sm:col-span-2">
              Timezone (IANA)
              <Input
                id="schedule-edit-timezone"
                className="mt-1"
                value={draft.timezone}
                onChange={(event) => patch({ timezone: event.target.value })}
                placeholder="Asia/Jakarta"
              />
            </label>
          </div>
        </section>

        <section className="space-y-3">
          <h4 className="text-11 font-medium tracking-wide text-tertiary uppercase">
            {structured ? "Recipe" : "Prompt (legacy)"}
          </h4>
          {structured ? (
            <ScheduleRecipeFields key={schedule.id} value={draft} onChange={patch} />
          ) : (
            <TextArea
              className="min-h-32"
              value={draft.prompt}
              onChange={(event) => patch({ prompt: event.target.value })}
              placeholder="Describe what the agent should do on every run"
            />
          )}
        </section>

        {(validationError || error) && <p className="text-12 text-danger-primary">{validationError ?? error}</p>}
      </div>
      <div className="sticky bottom-0 flex items-center justify-end gap-2 border-t border-subtle bg-surface-1 px-5 py-3">
        <Button size="sm" variant="secondary" disabled={saving} onClick={onCancel}>
          Cancel
        </Button>
        <Button
          size="sm"
          variant="primary"
          loading={saving}
          disabled={Boolean(validationError)}
          onClick={() => void save()}
        >
          Save changes
        </Button>
      </div>
    </div>
  );
});

function SelectField({
  id,
  label,
  value,
  onChange,
  children,
}: {
  id: string;
  label: string;
  value: string;
  onChange: ChangeEventHandler<HTMLSelectElement>;
  children: ReactNode;
}) {
  return (
    <label htmlFor={id} className="block text-12 text-secondary">
      {label}
      <div className="relative mt-1">
        <select
          id={id}
          value={value}
          onChange={onChange}
          className="h-9 w-full appearance-none rounded-md border border-subtle bg-layer-2 px-2.5 pr-8 text-13 text-primary transition-colors outline-none focus:border-accent-strong"
        >
          {children}
        </select>
        <ChevronDownOutline className="pointer-events-none absolute top-1/2 right-2.5 size-3.5 -translate-y-1/2 text-tertiary" />
      </div>
    </label>
  );
}
