import { Fragment } from "react";
import { useTranslation } from "react-i18next";
import { dayShortKey } from "@/i18n/day-keys";

export interface ScheduleCell {
  key: string;
  day: number;
  position: number;
  subjectName: string;
  classNames?: string;
  teacherName?: string;
  roomName: string;
}

interface ScheduleGridProps {
  cells: ScheduleCell[];
  daysPresent: number[];
  positions: number[];
}

export function ScheduleGrid({ cells, daysPresent, positions }: ScheduleGridProps) {
  const { t } = useTranslation();
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
            return (
              <div
                key={`${day}:${position}`}
                className="kz-ws-cell"
                {...(cell ? { "data-variant": "period" } : {})}
              >
                {cell ? (
                  <div className="flex flex-col leading-tight gap-0.5">
                    <span className="font-semibold text-foreground">{cell.subjectName}</span>
                    <span className="text-[10px] text-muted-foreground">
                      {[cell.classNames, cell.teacherName, cell.roomName]
                        .filter(Boolean)
                        .join(" · ")}
                    </span>
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
