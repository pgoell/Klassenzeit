import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { useWeekSchemeDetail, useWeekSchemes } from "@/features/week-schemes/hooks";
import { dayLongKey, dayShortKey } from "@/i18n/day-keys";
import { cn } from "@/lib/utils";
import {
  type TeacherAvailabilityEntry,
  type TeacherDetail,
  useSaveTeacherAvailability,
  useTeacherDetail,
} from "./hooks";

type TeacherAvailabilityStatus = "available" | "preferred" | "unavailable";

const TEACHER_AVAILABILITY_STATUSES: readonly TeacherAvailabilityStatus[] = [
  "available",
  "preferred",
  "unavailable",
] as const;

function isTeacherAvailabilityStatus(value: string): value is TeacherAvailabilityStatus {
  return value === "available" || value === "preferred" || value === "unavailable";
}

export function TeacherAvailabilityGrid({ teacherId }: { teacherId: string }) {
  const detail = useTeacherDetail(teacherId);
  if (!detail.isSuccess) return null;
  return <TeacherAvailabilityGridLoaded teacher={detail.data} />;
}

function TeacherAvailabilityGridLoaded({ teacher }: { teacher: TeacherDetail }) {
  const { t } = useTranslation();
  const schemes = useWeekSchemes();
  const save = useSaveTeacherAvailability();
  const workingDays = teacher.working_days ?? null;

  const [statuses, setStatuses] = useState<Map<string, TeacherAvailabilityStatus>>(() => {
    const map = new Map<string, TeacherAvailabilityStatus>();
    for (const entry of teacher.availability) {
      if (
        isTeacherAvailabilityStatus(entry.status) &&
        (entry.status === "preferred" || entry.status === "unavailable")
      ) {
        map.set(entry.time_block_id, entry.status);
      }
    }
    return map;
  });

  function setTeacherAvailabilityStatus(blockId: string, next: TeacherAvailabilityStatus) {
    setStatuses((prev) => {
      const map = new Map(prev);
      if (next === "available") map.delete(blockId);
      else map.set(blockId, next);
      return map;
    });
  }

  async function handleTeacherAvailabilitySave() {
    const entries: TeacherAvailabilityEntry[] = [];
    for (const [id, status] of statuses) {
      entries.push({ time_block_id: id, status });
    }
    try {
      await save.mutateAsync({ id: teacher.id, entries });
      toast.success(t("teachers.availability.saved"));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t("teachers.availability.saveError"));
    }
  }

  return (
    <div className="space-y-3 border-t pt-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold">{t("teachers.availability.sectionTitle")}</h3>
      </div>
      {schemes.data && schemes.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("teachers.availability.noSchemes")}</p>
      ) : (
        (schemes.data ?? []).map((scheme) => (
          <TeacherAvailabilitySchemeSection
            key={scheme.id}
            schemeId={scheme.id}
            schemeName={scheme.name}
            statuses={statuses}
            onSetStatus={setTeacherAvailabilityStatus}
            workingDays={workingDays}
          />
        ))
      )}
      <div className="flex justify-end">
        <Button size="sm" onClick={handleTeacherAvailabilitySave} disabled={save.isPending}>
          {save.isPending ? t("common.saving") : t("teachers.availability.save")}
        </Button>
      </div>
    </div>
  );
}

function TeacherAvailabilitySchemeSection({
  schemeId,
  schemeName,
  statuses,
  onSetStatus,
  workingDays,
}: {
  schemeId: string;
  schemeName: string;
  statuses: Map<string, TeacherAvailabilityStatus>;
  onSetStatus: (blockId: string, next: TeacherAvailabilityStatus) => void;
  workingDays: number[] | null;
}) {
  const { t } = useTranslation();
  const detail = useWeekSchemeDetail(schemeId);
  const blocks = detail.data?.time_blocks ?? [];
  if (detail.isLoading) {
    return <p className="text-sm text-muted-foreground">{t("common.loading")}</p>;
  }
  if (blocks.length === 0) {
    return (
      <section>
        <h4 className="text-sm font-medium">{schemeName}</h4>
        <p className="text-sm text-muted-foreground">{t("teachers.availability.noBlocks")}</p>
      </section>
    );
  }
  const days = [0, 1, 2, 3, 4] as const;
  const positions = Array.from(new Set(blocks.map((b) => b.position))).sort((a, b) => a - b);
  const byKey = new Map(blocks.map((b) => [`${b.day_of_week}-${b.position}`, b]));

  return (
    <section className="space-y-1">
      <h4 className="text-sm font-medium">{schemeName}</h4>
      <table className="w-full text-xs">
        <thead>
          <tr>
            <th className="w-16 p-1 text-left font-medium">{t("common.position")}</th>
            {days.map((d) => {
              const isOffDay = workingDays !== null && !workingDays.includes(d);
              return (
                <th
                  key={d}
                  data-off-day={isOffDay ? "true" : undefined}
                  className={cn("p-1 text-left font-medium", isOffDay && "opacity-40")}
                >
                  {t(dayShortKey(d))}
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {positions.map((p) => (
            <tr key={p}>
              <td className="p-1 font-mono">{p}</td>
              {days.map((d) => {
                const block = byKey.get(`${d}-${p}`);
                const isOffDay = workingDays !== null && !workingDays.includes(d);
                if (!block) return <td key={d} className="p-1" aria-hidden="true" />;
                const current = statuses.get(block.id) ?? "available";
                const dayName = t(dayLongKey(d));
                return (
                  <td
                    key={d}
                    data-off-day={isOffDay ? "true" : undefined}
                    className={cn("p-1", isOffDay && "opacity-40")}
                  >
                    <div className="flex gap-0.5">
                      {TEACHER_AVAILABILITY_STATUSES.map((status) => {
                        const isActive = current === status;
                        const letter = t(
                          `teachers.availability.status.${status}` as "teachers.availability.status.available",
                        );
                        const statusLabel = t(
                          `teachers.availability.statusLabel.${status}` as "teachers.availability.statusLabel.available",
                        );
                        const ariaLabel = t("teachers.availability.cellLabel", {
                          status: statusLabel,
                          day: dayName,
                          position: p,
                        });
                        return (
                          <button
                            key={status}
                            type="button"
                            aria-pressed={isActive}
                            aria-label={ariaLabel}
                            onClick={() => onSetStatus(block.id, status)}
                            className={cn(
                              "flex h-6 w-6 items-center justify-center rounded border font-mono text-[10px]",
                              isActive
                                ? status === "preferred"
                                  ? "border-foreground bg-accent text-accent-foreground"
                                  : status === "unavailable"
                                    ? "border-destructive bg-destructive text-destructive-foreground"
                                    : "border-foreground bg-muted text-foreground"
                                : "border-border/60 bg-background text-muted-foreground",
                            )}
                          >
                            {letter}
                          </button>
                        );
                      })}
                    </div>
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
