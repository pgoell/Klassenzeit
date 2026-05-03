import { DndContext, useDraggable, useDroppable } from "@dnd-kit/core";
import { Pin, PinOff } from "lucide-react";
import { Fragment, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { dayShortKey } from "@/i18n/day-keys";
import { cn } from "@/lib/utils";
import { useMovePlacement, usePinPlacement, useSwapPlacements } from "./hooks";
import {
  type DndPlacementCellRef,
  dndPlacementCellKey,
  useScheduleDragAndDrop,
} from "./use-schedule-drag-and-drop";

export interface ScheduleCell {
  key: string;
  day: number;
  position: number;
  subjectName: string;
  classNames?: string;
  teacherName?: string;
  roomName: string;
  // Optional placement identity. When present alongside `pinned`, the cell
  // renders a pin / unpin toggle button. Cells without these fields render
  // read-only (e.g. skeleton or detached previews).
  lessonId?: string;
  timeBlockId?: string;
  roomId?: string;
  pinned?: boolean;
}

interface ScheduleGridProps {
  cells: ScheduleCell[];
  daysPresent: number[];
  positions: number[];
  // When true, occupied cells are draggable and every (day, position) slot
  // is droppable, allowing the user to move or swap placements. Requires
  // `timeBlocksByDayPosition` so empty slots can resolve their target
  // `time_block_id`. Default off so teacher- and room-centric views remain
  // read-only without ceremony.
  dragEnabled?: boolean;
  timeBlocksByDayPosition?: Map<string, string>;
}

interface DraggablePlacementCardProps {
  lessonId: string;
  timeBlockId: string;
  roomId: string;
  children: React.ReactNode;
}

function DraggablePlacementCard({
  lessonId,
  timeBlockId,
  roomId,
  children,
}: DraggablePlacementCardProps) {
  // Pinned cards stay draggable: a "move" auto-pins the placement, so
  // disabling drag for pinned cards would trap the user in an unfixable
  // state without first clicking unpin. The pinned visual cue (border + pin
  // icon) signals the state without locking the card.
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id: `${lessonId}::${timeBlockId}`,
    data: { lesson_id: lessonId, source_time_block_id: timeBlockId, source_room_id: roomId },
  });
  return (
    <div
      ref={setNodeRef}
      {...attributes}
      {...listeners}
      className={cn(
        "relative flex flex-col leading-tight gap-0.5 cursor-grab active:cursor-grabbing",
        isDragging && "opacity-50",
      )}
    >
      {children}
    </div>
  );
}

interface DroppableSlotProps {
  timeBlockId: string;
  roomId?: string;
  children: React.ReactNode;
  className?: string;
  variant?: "period" | undefined;
}

function DroppableSlot({ timeBlockId, roomId, children, className, variant }: DroppableSlotProps) {
  const { setNodeRef, isOver } = useDroppable({
    id: roomId ? `${timeBlockId}::${roomId}` : `${timeBlockId}::__empty__`,
    data: { time_block_id: timeBlockId, room_id: roomId },
  });
  return (
    <div
      ref={setNodeRef}
      className={cn(className, isOver && "ring-2 ring-primary")}
      {...(variant ? { "data-variant": variant } : {})}
    >
      {children}
    </div>
  );
}

export function ScheduleGrid({
  cells,
  daysPresent,
  positions,
  dragEnabled = false,
  timeBlocksByDayPosition,
}: ScheduleGridProps) {
  const { t } = useTranslation();
  const pinMutation = usePinPlacement();
  const moveMutation = useMovePlacement();
  const swapMutation = useSwapPlacements();
  const byKey = new Map<string, ScheduleCell>();
  for (const cell of cells) {
    byKey.set(`${cell.day}:${cell.position}`, cell);
  }
  const placementByCell = useMemo(() => {
    const map = new Map<string, DndPlacementCellRef>();
    for (const cell of cells) {
      if (cell.lessonId && cell.timeBlockId && cell.roomId) {
        map.set(dndPlacementCellKey(cell.timeBlockId, cell.roomId), {
          lesson_id: cell.lessonId,
          time_block_id: cell.timeBlockId,
          room_id: cell.roomId,
        });
      }
    }
    return map;
  }, [cells]);
  const { onDragEnd } = useScheduleDragAndDrop({
    moveMutate: moveMutation.mutate,
    swapMutate: swapMutation.mutate,
    placementByCell,
  });
  const grid = (
    <div
      className="kz-ws-grid"
      style={{ gridTemplateColumns: `56px repeat(${daysPresent.length}, 1fr)` }}
    >
      <div className="kz-ws-cell" data-variant="header" />
      {daysPresent.map((day) => (
        <div key={`head-${day}`} className="kz-ws-cell" data-variant="header">
          {t(dayShortKey(day))}
        </div>
      ))}
      {positions.map((position) => (
        <Fragment key={`row-${position}`}>
          <div className="kz-ws-cell" data-variant="time">
            P{position}
          </div>
          {daysPresent.map((day) => {
            const cell = byKey.get(`${day}:${position}`);
            const togglable = cell?.lessonId && cell.timeBlockId && cell.pinned !== undefined;
            const cellClassName = cn(
              "kz-ws-cell",
              cell?.pinned && "kz-ws-cell--pinned",
              cell && "group",
            );
            const slotTimeBlockId =
              cell?.timeBlockId ?? timeBlocksByDayPosition?.get(`${day}:${position}`);
            const draggable = dragEnabled && cell?.lessonId && cell.timeBlockId && cell.roomId;
            const cardBody = cell ? (
              <>
                <span className="font-semibold text-foreground">{cell.subjectName}</span>
                <span className="text-[10px] text-muted-foreground">
                  {[cell.classNames, cell.teacherName, cell.roomName].filter(Boolean).join(" · ")}
                </span>
                {togglable && cell.lessonId && cell.timeBlockId ? (
                  <button
                    type="button"
                    aria-label={
                      cell.pinned ? t("schedule.actions.unpin") : t("schedule.actions.pin")
                    }
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={() => {
                      pinMutation.mutate({
                        lesson_id: cell.lessonId as string,
                        time_block_id: cell.timeBlockId as string,
                        pinned: !cell.pinned,
                      });
                    }}
                    className={cn(
                      "absolute right-0 top-0 rounded p-0.5 transition-opacity",
                      cell.pinned
                        ? "text-primary"
                        : "text-muted-foreground opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
                    )}
                  >
                    {cell.pinned ? <Pin className="h-3 w-3" /> : <PinOff className="h-3 w-3" />}
                  </button>
                ) : null}
              </>
            ) : null;
            const inner =
              draggable && cell?.lessonId && cell.timeBlockId && cell.roomId ? (
                <DraggablePlacementCard
                  lessonId={cell.lessonId}
                  timeBlockId={cell.timeBlockId}
                  roomId={cell.roomId}
                >
                  {cardBody}
                </DraggablePlacementCard>
              ) : cell ? (
                <div className="relative flex flex-col leading-tight gap-0.5">{cardBody}</div>
              ) : null;
            if (dragEnabled && slotTimeBlockId) {
              return (
                <DroppableSlot
                  key={`${day}:${position}`}
                  timeBlockId={slotTimeBlockId}
                  roomId={cell?.roomId}
                  className={cellClassName}
                  variant={cell ? "period" : undefined}
                >
                  {inner}
                </DroppableSlot>
              );
            }
            return (
              <div
                key={`${day}:${position}`}
                className={cellClassName}
                {...(cell ? { "data-variant": "period" } : {})}
              >
                {inner}
              </div>
            );
          })}
        </Fragment>
      ))}
    </div>
  );
  return dragEnabled ? <DndContext onDragEnd={onDragEnd}>{grid}</DndContext> : grid;
}
