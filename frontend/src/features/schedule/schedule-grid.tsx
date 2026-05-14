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

type PinKind = "hard" | "soft" | null;

// Three-state pin cycle: null → "hard" → "soft" → null. The tooltip mapping
// in the click handler keys off this same cycle (next-state describes the
// click outcome).
function nextPinKind(current: PinKind | undefined): PinKind {
  if (current == null) return "hard";
  if (current === "hard") return "soft";
  return null;
}

export interface ScheduleCell {
  key: string;
  day: number;
  position: number;
  subjectName: string;
  classNames?: string;
  teacherName?: string;
  roomName: string;
  // Optional placement identity. When present alongside `pinKind`, the cell
  // renders a pin / unpin toggle button. Cells without these fields render
  // read-only (e.g. skeleton or detached previews).
  lessonId?: string;
  timeBlockId?: string;
  roomId?: string;
  // Three-state pin discriminator (mirrors `PlacementResponse.pin_kind`).
  // Task 1 ships the storage shape; Task 6 widens click handling into a
  // three-state cycle. For now the click handler treats this as a two-state
  // toggle: any non-null kind clears, null sets `"hard"`.
  pinKind?: "hard" | "soft" | null;
  // Discriminator from `TimeBlockResponse.kind`. When "break", the cell
  // renders a non-bookable variant (no drag, drop, or click affordances).
  kind: "lesson" | "break";
  // Teacher-week-only: when the current teacher is the assigned Hofpause
  // supervisor for this break slot, render a supervision badge inside the
  // break cell. Defaults to false; lesson cells ignore this flag.
  isSupervised?: boolean;
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
  // Cells to render with a temporary amber ring (quality-issue highlight).
  // Matched by raw (day, position); pass an empty / undefined list to clear.
  highlightedCells?: ReadonlyArray<{ day_of_week: number; position: number }>;
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
      data-testid={`placement-card-${lessonId}`}
      data-lesson-id={lessonId}
      data-time-block-id={timeBlockId}
      data-room-id={roomId}
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
  day: number;
  position: number;
  highlighted?: boolean;
}

function DroppableSlot({
  timeBlockId,
  roomId,
  children,
  className,
  variant,
  day,
  position,
  highlighted,
}: DroppableSlotProps) {
  const { setNodeRef, isOver } = useDroppable({
    id: roomId ? `${timeBlockId}::${roomId}` : `${timeBlockId}::__empty__`,
    data: { time_block_id: timeBlockId, room_id: roomId },
  });
  return (
    <div
      ref={setNodeRef}
      data-testid={
        variant === "period" ? `placement-slot-${timeBlockId}` : `empty-slot-${timeBlockId}`
      }
      data-time-block-id={timeBlockId}
      data-cell-day={day}
      data-cell-pos={position}
      data-highlight={highlighted ? "true" : undefined}
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
  highlightedCells,
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
            const isHighlighted =
              highlightedCells?.some((h) => h.day_of_week === day && h.position === position) ??
              false;
            const highlightClass =
              isHighlighted && "ring-2 ring-amber-500 ring-offset-2 transition-shadow";
            const isBreak = cell?.kind === "break";
            if (isBreak) {
              return (
                <div
                  key={`${day}:${position}`}
                  className={cn("kz-ws-cell", "bg-muted text-muted-foreground", highlightClass)}
                  data-variant="break"
                  data-cell-day={day}
                  data-cell-pos={position}
                  data-highlight={isHighlighted ? "true" : undefined}
                >
                  <span className="text-xs">{t("weekSchemes.timeBlocks.kind.break")}</span>
                  {cell?.isSupervised && (
                    <span className="text-xs font-medium">{t("schedule.supervision.label")}</span>
                  )}
                </div>
              );
            }
            const isPinnedHard = cell?.pinKind === "hard";
            const isPinnedSoft = cell?.pinKind === "soft";
            const isPinned = isPinnedHard || isPinnedSoft;
            const togglable = cell?.lessonId && cell.timeBlockId && cell.pinKind !== undefined;
            const cellClassName = cn(
              "kz-ws-cell",
              isPinnedHard && "kz-ws-cell--pinned-hard",
              isPinnedSoft && "kz-ws-cell--pinned-soft",
              cell && "group",
              highlightClass,
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
                      cell.pinKind === "hard"
                        ? t("schedule.actions.pinCycle.softenToSoft")
                        : cell.pinKind === "soft"
                          ? t("schedule.actions.pinCycle.clear")
                          : t("schedule.actions.pinCycle.setHard")
                    }
                    onPointerDown={(e) => e.stopPropagation()}
                    onClick={() => {
                      pinMutation.mutate({
                        lesson_id: cell.lessonId as string,
                        time_block_id: cell.timeBlockId as string,
                        pin_kind: nextPinKind(cell.pinKind),
                      });
                    }}
                    className={cn(
                      "absolute right-0 top-0 rounded p-0.5 transition-opacity",
                      isPinned
                        ? "text-primary"
                        : "text-muted-foreground opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
                    )}
                  >
                    {isPinnedHard ? (
                      <Pin className="h-3 w-3" fill="currentColor" />
                    ) : isPinnedSoft ? (
                      <Pin className="h-3 w-3" />
                    ) : (
                      <PinOff className="h-3 w-3" />
                    )}
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
                  day={day}
                  position={position}
                  highlighted={isHighlighted}
                >
                  {inner}
                </DroppableSlot>
              );
            }
            return (
              <div
                key={`${day}:${position}`}
                className={cellClassName}
                data-cell-day={day}
                data-cell-pos={position}
                data-highlight={isHighlighted ? "true" : undefined}
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
