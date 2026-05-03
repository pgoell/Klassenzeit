import { useSearch } from "@tanstack/react-router";
import { Users } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { EmptyState } from "@/components/empty-state";
import { type EntityColumn, EntityListTable } from "@/components/entity-list-table";
import { EntityPageHead } from "@/components/entity-page-head";
import { Toolbar } from "@/components/toolbar";
import { Button } from "@/components/ui/button";
import { useStundentafeln } from "@/features/stundentafeln/hooks";
import { useWeekSchemes } from "@/features/week-schemes/hooks";
import { GenerateLessonsConfirmDialog } from "./generate-lessons-dialog";
import { type SchoolClass, useSchoolClasses } from "./hooks";
import { DeleteSchoolClassDialog, SchoolClassFormDialog } from "./school-classes-dialogs";

export function SchoolClassesPage() {
  const { t } = useTranslation();
  const schoolClasses = useSchoolClasses();
  const stundentafeln = useStundentafeln();
  const weekSchemes = useWeekSchemes();
  const search = useSearch({ strict: false }) as { create?: string };

  const [q, setQ] = useState("");
  const [creating, setCreating] = useState(() => search.create === "1");
  const [editing, setEditing] = useState<SchoolClass | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<SchoolClass | null>(null);
  const [generateFor, setGenerateFor] = useState<SchoolClass | null>(null);

  const stundentafelNameById = new Map(
    (stundentafeln.data ?? []).map((entry) => [entry.id, entry.name]),
  );
  const weekSchemeNameById = new Map(
    (weekSchemes.data ?? []).map((entry) => [entry.id, entry.name]),
  );

  const rows = (schoolClasses.data ?? []).filter((row) =>
    q ? `${row.name} ${row.grade_level}`.toLowerCase().includes(q.toLowerCase()) : true,
  );
  const showEmpty =
    !schoolClasses.isLoading && schoolClasses.data && schoolClasses.data.length === 0 && !q;

  const schoolClassColumns: EntityColumn<SchoolClass>[] = [
    {
      key: "name",
      header: t("schoolClasses.columns.name"),
      cell: (schoolClass) => schoolClass.name,
      cellClassName: "font-medium",
    },
    {
      key: "gradeLevel",
      header: t("schoolClasses.columns.gradeLevel"),
      cell: (schoolClass) => schoolClass.grade_level,
      className: "text-right",
      cellClassName: "font-mono text-[12.5px]",
    },
    {
      key: "stundentafel",
      header: t("schoolClasses.columns.stundentafel"),
      cell: (schoolClass) => stundentafelNameById.get(schoolClass.stundentafel_id) ?? "—",
    },
    {
      key: "weekScheme",
      header: t("schoolClasses.columns.weekScheme"),
      cell: (schoolClass) => weekSchemeNameById.get(schoolClass.week_scheme_id) ?? "—",
    },
  ];

  return (
    <div className="space-y-4">
      <EntityPageHead
        title={t("schoolClasses.title")}
        subtitle={t("schoolClasses.subtitle")}
        onCreate={() => setCreating(true)}
        createLabel={t("schoolClasses.new")}
      />

      {schoolClasses.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : schoolClasses.isError ? (
        <p className="text-sm text-destructive">{t("schoolClasses.loadError")}</p>
      ) : showEmpty ? (
        <EmptyState
          icon={<Users className="h-7 w-7" />}
          title={t("schoolClasses.empty.title")}
          body={t("schoolClasses.empty.body")}
          steps={[
            t("schoolClasses.empty.step1"),
            t("schoolClasses.empty.step2"),
            t("schoolClasses.empty.step3"),
          ]}
          onCreate={() => setCreating(true)}
          createLabel={t("schoolClasses.new")}
        />
      ) : (
        <>
          <Toolbar
            search={q}
            onSearch={setQ}
            placeholder={t("common.search")}
            right={
              <span className="font-mono text-xs text-muted-foreground">
                {rows.length} {t("schoolClasses.title").toLowerCase()}
              </span>
            }
          />
          <EntityListTable<SchoolClass>
            rows={rows}
            rowKey={(schoolClass) => schoolClass.id}
            columns={schoolClassColumns}
            actions={(schoolClass) => (
              <>
                <Button size="sm" variant="outline" onClick={() => setGenerateFor(schoolClass)}>
                  {t("schoolClasses.generateLessons.action")}
                </Button>
                <Button size="sm" variant="outline" onClick={() => setEditing(schoolClass)}>
                  {t("common.edit")}
                </Button>
                <Button
                  size="sm"
                  variant="destructive"
                  onClick={() => setConfirmDelete(schoolClass)}
                >
                  {t("common.delete")}
                </Button>
              </>
            )}
            actionsHeader={t("common.actions")}
          />
        </>
      )}

      <SchoolClassFormDialog
        open={creating}
        onOpenChange={setCreating}
        submitLabel={t("common.create")}
      />
      {editing ? (
        <SchoolClassFormDialog
          open={true}
          schoolClass={editing}
          onOpenChange={(open) => {
            if (!open) setEditing(null);
          }}
          submitLabel={t("common.save")}
        />
      ) : null}
      {confirmDelete ? (
        <DeleteSchoolClassDialog
          schoolClass={confirmDelete}
          onClose={() => setConfirmDelete(null)}
        />
      ) : null}
      {generateFor ? (
        <GenerateLessonsConfirmDialog
          schoolClass={generateFor}
          onDone={(count) => {
            setGenerateFor(null);
            if (count < 0) return;
            if (count === 0) {
              toast.info(t("schoolClasses.generateLessons.noneCreated"));
            } else {
              toast.success(t("schoolClasses.generateLessons.created", { count }));
            }
          }}
        />
      ) : null}
    </div>
  );
}
