import { useTranslation } from "react-i18next";
import { useSchoolClasses } from "@/features/school-classes/hooks";
import { useStundentafel } from "@/features/stundentafeln/hooks";
import { useTeachers } from "@/features/teachers/hooks";

interface ClassHeaderBandProps {
  classId: string;
}

export function ClassHeaderBand({ classId }: ClassHeaderBandProps) {
  const { t } = useTranslation();
  const classes = useSchoolClasses();
  const teachers = useTeachers();
  const schoolClass = classes.data?.find((c) => c.id === classId);
  const stundentafel = useStundentafel(schoolClass?.stundentafel_id ?? null);

  if (!schoolClass) return null;

  const klassenlehrer = schoolClass.class_teacher_id
    ? (teachers.data?.find((tt) => tt.id === schoolClass.class_teacher_id) ?? null)
    : null;
  const curriculumSubjectIds = new Set(stundentafel.data?.entries.map((e) => e.subject.id) ?? []);
  const showCoverage = klassenlehrer !== null && curriculumSubjectIds.size > 0;
  const covered = showCoverage
    ? [...curriculumSubjectIds].filter((id) => (klassenlehrer.subject_ids ?? []).includes(id))
        .length
    : 0;
  const total = curriculumSubjectIds.size;

  return (
    <div className="flex flex-wrap items-center gap-2 rounded-md border bg-muted/30 px-3 py-2 text-sm">
      <span className="font-medium">{t("schedule.classHeader.klassenlehrerLabel")}:</span>
      {klassenlehrer ? (
        <span>{`${klassenlehrer.first_name} ${klassenlehrer.last_name}`}</span>
      ) : (
        <span className="text-muted-foreground">{t("schedule.classHeader.notAssigned")}</span>
      )}
      {showCoverage ? (
        <span className="text-muted-foreground">
          {t("schedule.classHeader.coverage", { covered, total })}
        </span>
      ) : null}
    </div>
  );
}
