/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { observer } from "mobx-react";
import { useParams } from "next/navigation";
// hooks
import { useWarRoom } from "@/hooks/store/use-war-room";
// local imports
import { WarRoomsBoardRow } from "./war-rooms-board-row";

export const WarRoomsBoard = observer(function WarRoomsBoard() {
  // router
  const { projectId } = useParams();
  // store hooks
  const { getProjectWarRoomIds } = useWarRoom();
  // derived values
  const roomIds = projectId ? (getProjectWarRoomIds(projectId.toString()) ?? []) : [];

  return (
    <div className="vertical-scrollbar min-h-0 flex-1">
      {roomIds.map((id) => (
        <WarRoomsBoardRow key={id} warRoomId={id} />
      ))}
    </div>
  );
});
