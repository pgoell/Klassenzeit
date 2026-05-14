import { useTranslation } from "react-i18next";
import { Checkbox } from "@/components/ui/checkbox";
import { dayShortKey } from "@/i18n/day-keys";

const WEEKDAY_INDICES = [0, 1, 2, 3, 4] as const;

interface WeekdayPickerProps {
  value: number[] | null;
  onChange: (next: number[] | null) => void;
}

export function WeekdayPicker({ value, onChange }: WeekdayPickerProps) {
  const { t } = useTranslation();
  const selected = new Set(value ?? []);

  function toggleWeekday(day: number) {
    const next = new Set(selected);
    if (next.has(day)) next.delete(day);
    else next.add(day);
    const arr = Array.from(next).sort((a, b) => a - b);
    onChange(arr.length === 0 ? null : arr);
  }

  return (
    <div className="space-y-2">
      <p className="text-sm font-medium">{t("teachers.fields.workingDays")}</p>
      <div className="flex gap-3">
        {WEEKDAY_INDICES.map((day) => {
          const id = `working-days-${day}`;
          const label = t(dayShortKey(day));
          return (
            <div key={day} className="flex items-center gap-1.5">
              <Checkbox
                id={id}
                checked={selected.has(day)}
                onCheckedChange={() => toggleWeekday(day)}
              />
              <label htmlFor={id} className="text-sm">
                {label}
              </label>
            </div>
          );
        })}
      </div>
      <p className="text-xs text-muted-foreground">{t("teachers.fields.workingDaysHelper")}</p>
    </div>
  );
}
