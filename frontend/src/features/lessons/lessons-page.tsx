import { useSearch } from "@tanstack/react-router";
import { Layers } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/empty-state";
import { type EntityColumn, EntityListTable } from "@/components/entity-list-table";
import { EntityPageHead } from "@/components/entity-page-head";
import { Toolbar } from "@/components/toolbar";
import { Button } from "@/components/ui/button";
import { type Lesson, useLessons } from "./hooks";
import { DeleteLessonDialog, LessonFormDialog } from "./lessons-dialogs";

export function LessonsPage() {
  const { t } = useTranslation();
  const lessons = useLessons();
  const search = useSearch({ strict: false }) as { create?: string };

  const [q, setQ] = useState("");
  const [creating, setCreating] = useState(() => search.create === "1");
  const [editing, setEditing] = useState<Lesson | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Lesson | null>(null);

  const rows = (lessons.data ?? []).filter((row) => {
    if (!q) return true;
    const needle = q.toLowerCase();
    const teacherName = row.teacher
      ? `${row.teacher.first_name} ${row.teacher.last_name} ${row.teacher.short_code}`
      : "";
    const classNames = row.school_classes.map((c) => c.name).join(" ");
    return `${classNames} ${row.subject.name} ${row.subject.short_name} ${teacherName}`
      .toLowerCase()
      .includes(needle);
  });
  const showEmpty = !lessons.isLoading && lessons.data && lessons.data.length === 0 && !q;

  const lessonColumns: EntityColumn<Lesson>[] = [
    {
      key: "schoolClasses",
      header: t("lessons.columns.classes"),
      cell: (lesson) => lesson.school_classes.map((c) => c.name).join(", "),
      cellClassName: "font-medium",
    },
    {
      key: "subject",
      header: t("lessons.columns.subject"),
      cell: (lesson) => (
        <>
          {lesson.subject.name}{" "}
          <span className="text-muted-foreground">· {lesson.subject.short_name}</span>
        </>
      ),
    },
    {
      key: "teacher",
      header: t("lessons.columns.teacher"),
      cell: (lesson) => (
        <span
          title={
            lesson.teacher
              ? `${lesson.teacher.first_name} ${lesson.teacher.last_name}`
              : t("lessons.fields.teacherUnassigned")
          }
        >
          {lesson.teacher ? lesson.teacher.short_code : "—"}
        </span>
      ),
      cellClassName: "font-mono text-[12.5px]",
    },
    {
      key: "hoursPerWeek",
      header: t("lessons.columns.hoursPerWeek"),
      cell: (lesson) => lesson.hours_per_week,
      className: "text-right",
      cellClassName: "font-mono text-[12.5px]",
    },
    {
      key: "blockSize",
      header: t("lessons.columns.blockSize"),
      cell: (lesson) =>
        lesson.preferred_block_size === 2
          ? t("lessons.fields.blockSizeDouble")
          : t("lessons.fields.blockSizeSingle"),
    },
  ];

  return (
    <div className="space-y-4">
      <EntityPageHead
        title={t("lessons.title")}
        subtitle={t("lessons.subtitle")}
        onCreate={() => setCreating(true)}
        createLabel={t("lessons.new")}
      />

      {lessons.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : lessons.isError ? (
        <p className="text-sm text-destructive">{t("lessons.loadError")}</p>
      ) : showEmpty ? (
        <EmptyState
          icon={<Layers className="h-7 w-7" />}
          title={t("lessons.empty.title")}
          body={t("lessons.empty.body")}
          steps={[t("lessons.empty.step1"), t("lessons.empty.step2"), t("lessons.empty.step3")]}
          onCreate={() => setCreating(true)}
          createLabel={t("lessons.new")}
        />
      ) : (
        <>
          <Toolbar
            search={q}
            onSearch={setQ}
            placeholder={t("common.search")}
            right={
              <span className="font-mono text-xs text-muted-foreground">
                {rows.length} {t("lessons.title").toLowerCase()}
              </span>
            }
          />
          <EntityListTable<Lesson>
            rows={rows}
            rowKey={(lesson) => lesson.id}
            columns={lessonColumns}
            actions={(lesson) => (
              <>
                <Button size="sm" variant="outline" onClick={() => setEditing(lesson)}>
                  {t("common.edit")}
                </Button>
                <Button size="sm" variant="destructive" onClick={() => setConfirmDelete(lesson)}>
                  {t("common.delete")}
                </Button>
              </>
            )}
            actionsHeader={t("common.actions")}
          />
        </>
      )}

      <LessonFormDialog
        open={creating}
        onOpenChange={setCreating}
        submitLabel={t("common.create")}
      />
      {editing ? (
        <LessonFormDialog
          open={true}
          lesson={editing}
          onOpenChange={(open) => {
            if (!open) setEditing(null);
          }}
          submitLabel={t("common.save")}
        />
      ) : null}
      {confirmDelete ? (
        <DeleteLessonDialog lesson={confirmDelete} onClose={() => setConfirmDelete(null)} />
      ) : null}
    </div>
  );
}
