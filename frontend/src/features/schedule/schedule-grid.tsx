import { Pin, PinOff } from "lucide-react";
import { Fragment } from "react";
import { useTranslation } from "react-i18next";
import { dayShortKey } from "@/i18n/day-keys";
import { cn } from "@/lib/utils";
import { usePinPlacement } from "./hooks";

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
  pinned?: boolean;
}

interface ScheduleGridProps {
  cells: ScheduleCell[];
  daysPresent: number[];
  positions: number[];
}

export function ScheduleGrid({ cells, daysPresent, positions }: ScheduleGridProps) {
  const { t } = useTranslation();
  const pinMutation = usePinPlacement();
  const byKey = new Map<string, ScheduleCell>();
  for (const cell of cells) {
    byKey.set(`${cell.day}:${cell.position}`, cell);
  }
  return (
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
            return (
              <div
                key={`${day}:${position}`}
                className={cn("kz-ws-cell", cell?.pinned && "kz-ws-cell--pinned", cell && "group")}
                {...(cell ? { "data-variant": "period" } : {})}
              >
                {cell ? (
                  <div className="relative flex flex-col leading-tight gap-0.5">
                    <span className="font-semibold text-foreground">{cell.subjectName}</span>
                    <span className="text-[10px] text-muted-foreground">
                      {[cell.classNames, cell.teacherName, cell.roomName]
                        .filter(Boolean)
                        .join(" · ")}
                    </span>
                    {togglable && cell.lessonId && cell.timeBlockId ? (
                      <button
                        type="button"
                        aria-label={
                          cell.pinned ? t("schedule.actions.unpin") : t("schedule.actions.pin")
                        }
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
                  </div>
                ) : null}
              </div>
            );
          })}
        </Fragment>
      ))}
    </div>
  );
}
