import type { DragEndEvent } from "@dnd-kit/core";
import type { MovePlacementVars, SwapPlacementsVars } from "./hooks";

/**
 * Minimal placement reference indexed by `${timeBlockId}::${roomId}` so the
 * drop handler can decide whether the target cell is empty (move) or already
 * holds another lesson (swap). Keys mirror the camelCase used elsewhere in
 * the frontend; the underlying placement payload still uses the snake_case
 * shape that comes back from the API.
 */
export interface DndPlacementCellRef {
  lesson_id: string;
  time_block_id: string;
  room_id: string;
}

export interface ScheduleDragAndDropArgs {
  moveMutate: (vars: MovePlacementVars) => unknown;
  swapMutate: (vars: SwapPlacementsVars) => unknown;
  placementByCell: Map<string, DndPlacementCellRef>;
}

export interface ScheduleDragAndDropApi {
  onDragEnd: (event: DragEndEvent) => void;
}

interface DndActiveData {
  lesson_id: string;
  source_time_block_id: string;
  source_room_id?: string;
}

interface DndOverData {
  time_block_id: string;
  // Empty-slot droppables in the class view leave `room_id` undefined; the
  // drop handler then falls back to `source_room_id` from the active card so
  // the lesson keeps its current room across a move.
  room_id?: string;
}

export function dndPlacementCellKey(timeBlockId: string, roomId: string): string {
  return `${timeBlockId}::${roomId}`;
}

export function useScheduleDragAndDrop(args: ScheduleDragAndDropArgs): ScheduleDragAndDropApi {
  return {
    onDragEnd(event) {
      if (event.over === null) return;
      const activeData = event.active.data.current as DndActiveData | undefined;
      const overData = event.over.data.current as DndOverData | undefined;
      if (activeData === undefined || overData === undefined) return;
      const targetRoomId = overData.room_id ?? activeData.source_room_id;
      if (targetRoomId === undefined) return;
      const targetKey = dndPlacementCellKey(overData.time_block_id, targetRoomId);
      const occupant = args.placementByCell.get(targetKey);
      if (occupant !== undefined && occupant.lesson_id === activeData.lesson_id) {
        // Dropped onto the source cell itself; nothing to do.
        return;
      }
      if (occupant !== undefined) {
        args.swapMutate({
          a: {
            lesson_id: activeData.lesson_id,
            time_block_id: activeData.source_time_block_id,
          },
          b: {
            lesson_id: occupant.lesson_id,
            time_block_id: occupant.time_block_id,
          },
        });
        return;
      }
      args.moveMutate({
        lesson_id: activeData.lesson_id,
        source_time_block_id: activeData.source_time_block_id,
        time_block_id: overData.time_block_id,
        room_id: targetRoomId,
      });
    },
  };
}
