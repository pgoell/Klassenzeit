import { useSearch } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { SchedulePageClassView } from "./schedule-page-class-view";
import { SchedulePageRoomView } from "./schedule-page-room-view";
import { SchedulePageTeacherView } from "./schedule-page-teacher-view";
import { ScheduleTabs } from "./schedule-tabs";

type ScheduleView = "class" | "teacher" | "room";

export function SchedulePage() {
  const { t } = useTranslation();
  const search = useSearch({ strict: false }) as { view?: ScheduleView };
  const view: ScheduleView = search.view ?? "class";
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{t("schedule.title")}</h1>
        <p className="mt-1 text-sm text-muted-foreground">{t("schedule.subtitle")}</p>
      </div>
      <ScheduleTabs active={view} />
      {view === "class" ? <SchedulePageClassView /> : null}
      {view === "teacher" ? <SchedulePageTeacherView /> : null}
      {view === "room" ? <SchedulePageRoomView /> : null}
    </div>
  );
}
