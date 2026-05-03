import { useSearch } from "@tanstack/react-router";
import { GraduationCap } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/empty-state";
import { type EntityColumn, EntityListTable } from "@/components/entity-list-table";
import { EntityPageHead } from "@/components/entity-page-head";
import { Toolbar } from "@/components/toolbar";
import { Button } from "@/components/ui/button";
import { type Teacher, useTeachers } from "./hooks";
import { DeleteTeacherDialog, TeacherFormDialog } from "./teachers-dialogs";

export function TeachersPage() {
  const { t, i18n } = useTranslation();
  const teachers = useTeachers();
  const search = useSearch({ strict: false }) as { create?: string };

  const [q, setQ] = useState("");
  const [creating, setCreating] = useState(() => search.create === "1");
  const [editing, setEditing] = useState<Teacher | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Teacher | null>(null);

  const sorted = useMemo(() => {
    const list = teachers.data ?? [];
    return [...list].sort((a, b) => a.last_name.localeCompare(b.last_name, i18n.language));
  }, [teachers.data, i18n.language]);

  const rows = sorted.filter((row) =>
    q
      ? `${row.first_name} ${row.last_name} ${row.short_code}`
          .toLowerCase()
          .includes(q.toLowerCase())
      : true,
  );
  const showEmpty = !teachers.isLoading && teachers.data && teachers.data.length === 0 && !q;

  const teacherColumns: EntityColumn<Teacher>[] = [
    {
      key: "lastName",
      header: t("teachers.columns.lastName"),
      cell: (teacher) => teacher.last_name,
      cellClassName: "font-medium",
    },
    {
      key: "firstName",
      header: t("teachers.columns.firstName"),
      cell: (teacher) => teacher.first_name,
    },
    {
      key: "shortCode",
      header: t("teachers.columns.shortCode"),
      cell: (teacher) => teacher.short_code,
      cellClassName: "font-mono text-[12.5px]",
    },
    {
      key: "maxHoursPerWeek",
      header: t("teachers.columns.maxHoursPerWeek"),
      cell: (teacher) => teacher.max_hours_per_week,
      className: "text-right",
      cellClassName: "font-mono text-[12.5px]",
    },
  ];

  return (
    <div className="space-y-4">
      <EntityPageHead
        title={t("teachers.title")}
        subtitle={t("teachers.subtitle")}
        onCreate={() => setCreating(true)}
        createLabel={t("teachers.new")}
      />

      {teachers.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : teachers.isError ? (
        <p className="text-sm text-destructive">{t("teachers.loadError")}</p>
      ) : showEmpty ? (
        <EmptyState
          icon={<GraduationCap className="h-7 w-7" />}
          title={t("teachers.empty.title")}
          body={t("teachers.empty.body")}
          steps={[t("teachers.empty.step1"), t("teachers.empty.step2"), t("teachers.empty.step3")]}
          onCreate={() => setCreating(true)}
          createLabel={t("teachers.new")}
        />
      ) : (
        <>
          <Toolbar
            search={q}
            onSearch={setQ}
            placeholder={t("common.search")}
            right={
              <span className="font-mono text-xs text-muted-foreground">
                {rows.length} {t("teachers.title").toLowerCase()}
              </span>
            }
          />
          <EntityListTable<Teacher>
            rows={rows}
            rowKey={(teacher) => teacher.id}
            columns={teacherColumns}
            actions={(teacher) => (
              <>
                <Button size="sm" variant="outline" onClick={() => setEditing(teacher)}>
                  {t("common.edit")}
                </Button>
                <Button size="sm" variant="destructive" onClick={() => setConfirmDelete(teacher)}>
                  {t("common.delete")}
                </Button>
              </>
            )}
            actionsHeader={t("common.actions")}
          />
        </>
      )}

      <TeacherFormDialog
        open={creating}
        onOpenChange={setCreating}
        submitLabel={t("common.create")}
      />
      {editing ? (
        <TeacherFormDialog
          open={true}
          teacher={editing}
          onOpenChange={(open) => {
            if (!open) setEditing(null);
          }}
          submitLabel={t("common.save")}
        />
      ) : null}
      {confirmDelete ? (
        <DeleteTeacherDialog teacher={confirmDelete} onClose={() => setConfirmDelete(null)} />
      ) : null}
    </div>
  );
}
