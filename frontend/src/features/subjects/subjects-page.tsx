import { useSearch } from "@tanstack/react-router";
import { BookOpen } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/empty-state";
import { type EntityColumn, EntityListTable } from "@/components/entity-list-table";
import { EntityPageHead } from "@/components/entity-page-head";
import { Toolbar } from "@/components/toolbar";
import { Button } from "@/components/ui/button";
import { resolveSubjectColor } from "./color";
import { type Subject, useSubjects } from "./hooks";
import { DeleteSubjectDialog, SubjectFormDialog } from "./subjects-dialogs";

export function SubjectsPage() {
  const { t } = useTranslation();
  const subjects = useSubjects();
  const search = useSearch({ strict: false }) as { create?: string };

  const [q, setQ] = useState("");
  const [creating, setCreating] = useState(() => search.create === "1");
  const [editing, setEditing] = useState<Subject | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Subject | null>(null);

  const rows = (subjects.data ?? []).filter((row) =>
    q ? `${row.name} ${row.short_name}`.toLowerCase().includes(q.toLowerCase()) : true,
  );
  const showEmpty = !subjects.isLoading && subjects.data && subjects.data.length === 0 && !q;

  const subjectColumns: EntityColumn<Subject>[] = [
    {
      key: "name",
      header: t("subjects.columns.name"),
      cell: (subject) => (
        <span className="inline-flex items-center gap-2">
          <span className="kz-swatch" style={{ background: resolveSubjectColor(subject.color) }} />
          {subject.name}
        </span>
      ),
      cellClassName: "font-medium",
    },
    {
      key: "shortName",
      header: t("subjects.columns.shortName"),
      cell: (subject) => subject.short_name,
      cellClassName: "font-mono text-[12.5px]",
    },
  ];

  return (
    <div className="space-y-4">
      <EntityPageHead
        title={t("subjects.title")}
        subtitle={t("subjects.subtitle")}
        onCreate={() => setCreating(true)}
        createLabel={t("subjects.new")}
      />

      {subjects.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : subjects.isError ? (
        <p className="text-sm text-destructive">{t("subjects.loadError")}</p>
      ) : showEmpty ? (
        <EmptyState
          icon={<BookOpen className="h-7 w-7" />}
          title={t("subjects.empty.title")}
          body={t("subjects.empty.body")}
          steps={[t("subjects.empty.step1"), t("subjects.empty.step2"), t("subjects.empty.step3")]}
          onCreate={() => setCreating(true)}
          createLabel={t("subjects.new")}
        />
      ) : (
        <>
          <Toolbar
            search={q}
            onSearch={setQ}
            placeholder={t("common.search")}
            right={
              <span className="font-mono text-xs text-muted-foreground">
                {rows.length} {t("subjects.title").toLowerCase()}
              </span>
            }
          />
          <EntityListTable<Subject>
            rows={rows}
            rowKey={(subject) => subject.id}
            columns={subjectColumns}
            actions={(subject) => (
              <>
                <Button size="sm" variant="outline" onClick={() => setEditing(subject)}>
                  {t("common.edit")}
                </Button>
                <Button size="sm" variant="destructive" onClick={() => setConfirmDelete(subject)}>
                  {t("common.delete")}
                </Button>
              </>
            )}
            actionsHeader={t("common.actions")}
          />
        </>
      )}

      <SubjectFormDialog
        open={creating}
        onOpenChange={setCreating}
        submitLabel={t("common.create")}
      />
      {editing ? (
        <SubjectFormDialog
          open={true}
          subject={editing}
          onOpenChange={(open) => {
            if (!open) setEditing(null);
          }}
          submitLabel={t("common.save")}
        />
      ) : null}
      {confirmDelete ? (
        <DeleteSubjectDialog subject={confirmDelete} onClose={() => setConfirmDelete(null)} />
      ) : null}
    </div>
  );
}
