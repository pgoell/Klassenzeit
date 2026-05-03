import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { SchoolClass } from "@/features/school-classes/hooks";
import { ApiError } from "@/lib/api-client";
import { useGenerateAllSchedules } from "./hooks";

interface ScheduleToolbarProps {
  classes: SchoolClass[];
  classId: string | undefined;
  onClassChange: (id: string) => void;
  onGenerate: () => void;
  onCancelConfirm: () => void;
  placementsCount: number;
  confirming: boolean;
  pending: boolean;
}

export function ScheduleToolbar({
  classes,
  classId,
  onClassChange,
  onGenerate,
  onCancelConfirm,
  placementsCount,
  confirming,
  pending,
}: ScheduleToolbarProps) {
  const { t } = useTranslation();
  const generateAll = useGenerateAllSchedules();
  const disabled = pending || !classId;

  const runGenerateAll = async () => {
    try {
      const result = await generateAll.mutateAsync();
      toast.success(
        t("schedule.generate.allSuccessToast", {
          classes: result.classes.length,
          placements: result.total_placements,
          violations: result.total_violations,
        }),
      );
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : t("schedule.generate.allErrorToast");
      toast.error(msg || t("schedule.generate.allErrorToast"));
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-end justify-between gap-3 rounded-lg border bg-card px-4 py-3">
        <div className="min-w-[220px] space-y-1">
          <label
            htmlFor="schedule-class-picker"
            className="block text-xs font-medium text-muted-foreground"
          >
            {t("schedule.picker.label")}
          </label>
          <Select value={classId ?? ""} onValueChange={onClassChange}>
            <SelectTrigger id="schedule-class-picker" aria-label={t("schedule.picker.label")}>
              <SelectValue placeholder={t("schedule.picker.placeholder")} />
            </SelectTrigger>
            <SelectContent>
              {classes.map((c) => (
                <SelectItem key={c.id} value={c.id}>
                  {c.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="flex gap-2">
          <Button onClick={onGenerate} variant="secondary" disabled={disabled}>
            {pending ? t("common.saving") : t("schedule.generate.action")}
          </Button>
          <Button variant="default" onClick={runGenerateAll} disabled={generateAll.isPending}>
            {t("schedule.generate.allAction")}
          </Button>
        </div>
      </div>
      {confirming ? (
        <div
          role="alert"
          aria-live="polite"
          className="flex flex-wrap items-center gap-3 rounded-md border border-amber-400/60 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-400/40 dark:bg-amber-950/40 dark:text-amber-200"
        >
          <span>{t("schedule.generate.replaceBanner", { count: placementsCount })}</span>
          <div className="ml-auto flex gap-2">
            <Button size="sm" variant="outline" onClick={onCancelConfirm}>
              {t("schedule.generate.cancel")}
            </Button>
            <Button size="sm" onClick={onGenerate} disabled={pending}>
              {t("schedule.generate.confirmReplace")}
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
