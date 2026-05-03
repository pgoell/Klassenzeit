import { useTranslation } from "react-i18next";
import { SchedulePageClassView } from "./schedule-page-class-view";

export function SchedulePage() {
  const { t } = useTranslation();
  return (
    <div className="space-y-5">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{t("schedule.title")}</h1>
        <p className="mt-1 text-sm text-muted-foreground">{t("schedule.subtitle")}</p>
      </div>
      <SchedulePageClassView />
    </div>
  );
}
