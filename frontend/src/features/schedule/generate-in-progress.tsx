import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useCancelSchedule, useScheduleProgress } from "./hooks";
import { ProgressBar } from "./progress-bar";

interface GenerateInProgressProps {
  classId: string;
}

export function GenerateInProgress({ classId }: GenerateInProgressProps) {
  const { t } = useTranslation();
  const { data: snapshot } = useScheduleProgress(classId, true);
  const cancel = useCancelSchedule(classId);

  const fraction =
    snapshot && snapshot.deadline_ms > 0 ? snapshot.elapsed_ms / snapshot.deadline_ms : 0;
  const placed = snapshot?.placement_count ?? 0;
  const total = snapshot?.total_lessons ?? 0;
  const stopping = snapshot?.cancel_requested === true || cancel.isPending;

  return (
    <div className="flex min-w-[220px] flex-col gap-2" aria-live="polite">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm">{t("schedule.progress.placedCount", { placed, total })}</span>
        <Button variant="destructive" size="sm" disabled={stopping} onClick={() => cancel.mutate()}>
          {stopping ? t("schedule.generate.stopping") : t("schedule.generate.stop")}
        </Button>
      </div>
      <ProgressBar value={fraction} />
    </div>
  );
}
