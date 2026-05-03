import { Link } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";

const TAB_VIEWS = ["class", "teacher", "room"] as const;
type TabView = (typeof TAB_VIEWS)[number];

const TAB_LABEL_KEY = {
  class: "schedule.tabs.class",
  teacher: "schedule.tabs.teacher",
  room: "schedule.tabs.room",
} as const;

interface ScheduleTabsProps {
  active: TabView;
}

export function ScheduleTabs({ active }: ScheduleTabsProps) {
  const { t } = useTranslation();
  return (
    <div
      role="tablist"
      aria-label={t("schedule.title")}
      className="inline-flex rounded-lg border bg-card p-1"
    >
      {TAB_VIEWS.map((view) => (
        <Link
          key={view}
          to="/schedule"
          search={{ view }}
          role="tab"
          aria-selected={view === active}
          className={
            view === active
              ? "rounded-md bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground"
              : "rounded-md px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground"
          }
        >
          {t(TAB_LABEL_KEY[view])}
        </Link>
      ))}
    </div>
  );
}
