/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { useState } from "react";
import { Button } from "@plane/propel/button";
import { v4 as uuidv4 } from "uuid";
import { AI_SCHEDULE_TOOLS, type TAiScheduleSpec, type TAiScheduleTool } from "@/lib/ai-schedule";

type Props = {
  value: Pick<TAiScheduleSpec, "description" | "how_to" | "tools" | "expected_output">;
  onChange: (patch: Partial<TAiScheduleSpec>) => void;
};

export function ScheduleRecipeFields({ value, onChange }: Props) {
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
    <>
      <label className="mt-2 block text-12 text-secondary">
        Description
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={2}
          value={value.description}
          onChange={(event) => onChange({ description: event.target.value })}
        />
      </label>

      <div className="mt-2 text-12 text-secondary">
        Steps
        {stepEntries.map((entry) => (
          <div key={entry.id} className="mt-1 flex items-center gap-1">
            <input
              className="w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
              value={entry.step}
              onChange={(event) => updateStep(entry.index, event.target.value)}
            />
            <Button
              size="sm"
              variant="ghost"
              disabled={value.how_to.length === 1}
              onClick={() => removeStep(entry.index)}
            >
              Remove
            </Button>
          </div>
        ))}
        <Button size="sm" variant="ghost" onClick={addStep}>
          Add step
        </Button>
      </div>

      <div className="mt-2 text-12 text-secondary">
        Tools
        <div className="mt-1 flex flex-wrap gap-2">
          {AI_SCHEDULE_TOOLS.map((tool) => (
            <label key={tool} className="flex items-center gap-1 text-12 text-primary">
              <input
                type="checkbox"
                checked={value.tools.includes(tool)}
                onChange={(event) => toggleTool(tool, event.target.checked)}
              />
              {tool}
            </label>
          ))}
        </div>
      </div>

      <label className="mt-2 block text-12 text-secondary">
        Expected output
        <textarea
          className="mt-0.5 w-full rounded border border-subtle bg-layer-2 px-2 py-1 text-12 text-primary"
          rows={2}
          value={value.expected_output}
          onChange={(event) => onChange({ expected_output: event.target.value })}
        />
      </label>
    </>
  );
}
