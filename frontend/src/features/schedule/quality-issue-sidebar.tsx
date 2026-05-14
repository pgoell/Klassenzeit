import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { dayLongKey } from "@/i18n/day-keys";
import type { components } from "@/lib/api-types";

type QualityIssue = components["schemas"]["QualityIssueResponse"];

interface QualityIssueSidebarProps {
  issues: QualityIssue[];
  onIssueClick: (issue: QualityIssue) => void;
  subjectMap?: Map<string, string>;
}

const KIND_ORDER: QualityIssue["kind"][] = [
  "room_hop",
  "imbalance",
  "home_room_miss",
  "day_too_long",
  "interior_gap",
  "class_teacher_subject_share",
];

export function QualityIssueSidebar({
  issues,
  onIssueClick,
  subjectMap,
}: QualityIssueSidebarProps) {
  const { t } = useTranslation();

  if (issues.length === 0) {
    return (
      <aside className="rounded-lg border bg-card p-4">
        <h2 className="font-semibold text-foreground">{t("schedule.qualityIssues.title")}</h2>
        <p className="mt-2 text-sm text-muted-foreground">{t("schedule.qualityIssues.empty")}</p>
      </aside>
    );
  }

  const grouped = new Map<QualityIssue["kind"], QualityIssue[]>();
  for (const issue of issues) {
    const bucket = grouped.get(issue.kind) ?? [];
    bucket.push(issue);
    grouped.set(issue.kind, bucket);
  }

  return (
    <aside className="rounded-lg border bg-card p-4">
      <div className="flex items-center gap-2 font-semibold text-foreground">
        <AlertTriangle className="h-4 w-4 text-amber-600 dark:text-amber-400" />
        {t("schedule.qualityIssues.title")} ({issues.length})
      </div>
      <div className="mt-3 space-y-4">
        {KIND_ORDER.filter((kind) => grouped.has(kind)).map((kind) => {
          const kindIssues = grouped.get(kind) ?? [];
          return (
            <section key={kind}>
              <h3 className="text-sm font-medium text-foreground">
                {t(`schedule.qualityIssues.kind.${kind}.title`)} ({kindIssues.length})
              </h3>
              <ul className="mt-1 space-y-1">
                {kindIssues.map((issue) => {
                  const cells = issue.cells ?? [];
                  const hasCells = cells.length > 0;
                  const subjectName =
                    issue.subject_id != null
                      ? (subjectMap?.get(issue.subject_id) ?? issue.subject_id)
                      : "";
                  const dayLabel =
                    issue.day_of_week != null ? t(dayLongKey(issue.day_of_week)) : "";
                  const stableKey = `${issue.kind}:${issue.school_class_id}:${issue.day_of_week ?? ""}:${issue.subject_id ?? ""}`;
                  return (
                    <li key={stableKey}>
                      <button
                        type="button"
                        className="w-full text-left text-sm text-muted-foreground hover:text-foreground hover:underline disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:no-underline"
                        onClick={() => onIssueClick(issue)}
                        disabled={!hasCells}
                        aria-disabled={!hasCells}
                      >
                        <span className="font-medium text-foreground">
                          {t(`schedule.qualityIssues.kind.${issue.kind}.title`)}
                        </span>
                        {": "}
                        {t(`schedule.qualityIssues.kind.${issue.kind}.description`, {
                          subject: subjectName,
                          day: dayLabel,
                          ...issue.detail,
                        })}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </section>
          );
        })}
      </div>
    </aside>
  );
}
