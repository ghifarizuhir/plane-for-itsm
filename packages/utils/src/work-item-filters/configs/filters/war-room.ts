/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import type { TFilterProperty, TSupportedOperators } from "@plane/types";
import { COLLECTION_OPERATOR, EQUALITY_OPERATOR } from "@plane/types";
// local imports
import type { TCreateFilterConfigParams, IFilterIconConfig, TCreateFilterConfig } from "../../../rich-filters";
import { createFilterConfig, getMultiSelectConfig, createOperatorConfigEntry } from "../../../rich-filters";

// ------------ War room filter (fork) ------------

export type TWarRoomFilterValue = "on" | "off";

const WAR_ROOM_FILTER_OPTIONS: { key: TWarRoomFilterValue; title: string }[] = [
  { key: "on", title: "On" },
  { key: "off", title: "Off" },
];

export type TCreateWarRoomFilterParams = TCreateFilterConfigParams & IFilterIconConfig<TWarRoomFilterValue>;

export const getWarRoomMultiSelectConfig = (
  params: TCreateWarRoomFilterParams,
  singleValueOperator: TSupportedOperators
) =>
  getMultiSelectConfig<{ key: TWarRoomFilterValue; title: string }, TWarRoomFilterValue, TWarRoomFilterValue>(
    {
      items: WAR_ROOM_FILTER_OPTIONS,
      getId: (option) => option.key,
      getLabel: (option) => option.title,
      getValue: (option) => option.key,
      getIconData: (option) => option.key,
    },
    {
      singleValueOperator,
      ...params,
    },
    {
      ...params,
    }
  );

export const getWarRoomFilterConfig =
  <P extends TFilterProperty>(key: P): TCreateFilterConfig<P, TCreateWarRoomFilterParams> =>
  (params: TCreateWarRoomFilterParams) =>
    createFilterConfig<P>({
      id: key,
      label: "War room",
      ...params,
      icon: params.filterIcon,
      supportedOperatorConfigsMap: new Map([
        createOperatorConfigEntry(COLLECTION_OPERATOR.IN, params, (updatedParams) =>
          getWarRoomMultiSelectConfig(updatedParams, EQUALITY_OPERATOR.EXACT)
        ),
      ]),
    });
