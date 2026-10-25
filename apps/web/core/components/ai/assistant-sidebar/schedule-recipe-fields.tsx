/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useId, useState } from "react";
import { AddOutline, CloseOutline, TickOutline } from "@makeplane/propel/icons";
import { Button } from "@plane/propel/button";
import { Input, TextArea } from "@plane/ui";
import { cn } from "@plane/utils";
import { v4 as uuidv4 } from "uuid";
import { AI_SCHEDULE_TOOLS, type TAiScheduleSpec, type TAiScheduleTool } from "@/lib/ai-schedule";

type Props = {
  value: Pick<TAiScheduleSpec, "description" | "how_to" | "tools" | "expected_output">;
  onChange: (patch: Partial<TAiScheduleSpec>) => void;
};

export function ScheduleRecipeFields({ value, onChange }: Props) {
  const fieldId = useId();
  const [stepKeys, setStepKeys] = useState<string[]>(() => value.how_to.map(() => uuidv4()));

  const updateStep = (index: number, step: string) => {
    const howTo = [...value.how_to];
    howTo[index] = step;
    onChange({ how_to: howTo });
  };
  const removeStep = (index: number) => {
    onChange({ how_to: value.how_to.filter((_, stepIndex) => stepIndex !== index) });
    setStepKeys((keys) => keys.filter((_, keyIndex) => keyIndex !== index));
  };
  const addStep = () => {
    onChange({ how_to: [...value.how_to, ""] });
    setStepKeys((keys) => [...keys, uuidv4()]);
  };
  const toggleTool = (tool: TAiScheduleTool, checked: boolean) =>
    onChange({
      tools: checked ? [...value.tools, tool] : value.tools.filter((current) => current !== tool),
    });

  const stepEntries = value.how_to.map((step, index) => ({
    id: stepKeys[index] ?? uuidv4(),
    step,
    index,
  }));

  return (
    <div className="space-y-4">
      <label htmlFor={`${fieldId}-description`} className="block text-12 text-secondary">
        Description
        <TextArea
          id={`${fieldId}-description`}
          className="mt-1"
          rows={2}
          value={value.description}
          onChange={(event) => onChange({ description: event.target.value })}
          placeholder="What should this schedule produce?"
        />
      </label>

      <div className="text-12 text-secondary">
        <div className="flex items-center justify-between gap-2">
          <span>Steps</span>
          <span className="text-11 text-tertiary">{value.how_to.length}</span>
        </div>
        <div className="mt-1.5 space-y-1.5">
          {stepEntries.map((entry) => (
            <div key={entry.id} className="flex items-center gap-2">
              <span className="grid size-6 flex-shrink-0 place-items-center rounded-full border border-subtle bg-layer-1 text-10 font-medium text-tertiary">
                {entry.index + 1}
              </span>
              <Input
                value={entry.step}
                onChange={(event) => updateStep(entry.index, event.target.value)}
                placeholder="Describe a step"
              />
              <button
                type="button"
                aria-label={`Remove step ${entry.index + 1}`}
                disabled={value.how_to.length === 1}
                onClick={() => removeStep(entry.index)}
                className="grid size-7 flex-shrink-0 place-items-center rounded-md text-tertiary transition-colors hover:bg-layer-1 hover:text-danger-primary disabled:pointer-events-none disabled:opacity-40"
              >
                <CloseOutline className="size-3.5" />
              </button>
            </div>
          ))}
        </div>
        <Button
          size="sm"
          variant="ghost"
          className="mt-1"
          prependIcon={<AddOutline className="size-3.5" />}
          onClick={addStep}
        >
          Add step
        </Button>
      </div>

      <div className="text-12 text-secondary">
        <div className="flex items-center justify-between gap-2">
          <span>Tools</span>
          <span className="text-11 text-tertiary">{value.tools.length} selected</span>
        </div>
        <div className="mt-1.5 flex flex-wrap gap-1.5">
          {AI_SCHEDULE_TOOLS.map((tool) => {
            const checked = value.tools.includes(tool);
            return (
              <button
                key={tool}
                type="button"
                title={tool}
                aria-pressed={checked}
                onClick={() => toggleTool(tool, !checked)}
                className={cn(
                  "flex items-center gap-1 rounded-full border px-2 py-1 text-11 transition-colors",
                  checked
                    ? "border-accent-strong bg-accent-primary/10 text-accent-primary"
                    : "border-subtle bg-layer-1 text-secondary hover:border-strong hover:text-primary"
                )}
              >
                {checked && <TickOutline className="size-3" />}
                {tool.replaceAll("_", " ")}
              </button>
            );
          })}
        </div>
      </div>

      <label htmlFor={`${fieldId}-expected-output`} className="block text-12 text-secondary">
        Expected output
        <TextArea
          id={`${fieldId}-expected-output`}
          className="mt-1"
          rows={2}
          value={value.expected_output}
          onChange={(event) => onChange({ expected_output: event.target.value })}
          placeholder="Describe the report or result"
        />
      </label>
    </div>
  );
}
