import { GraduationCap } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/empty-state";
import { type EntityColumn, EntityListTable } from "@/components/entity-list-table";
import { EntityPageHead } from "@/components/entity-page-head";
import { Toolbar } from "@/components/toolbar";
import { Button } from "@/components/ui/button";
import { type School, useSchools } from "./hooks";
import { DeleteSchoolDialog, SchoolFormDialog } from "./schools-dialogs";

export function SchoolsPage() {
  const { t } = useTranslation();
  const schools = useSchools();

  const [q, setQ] = useState("");
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<School | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<School | null>(null);

  const rows = (schools.data ?? []).filter((row) =>
    q ? `${row.name} ${row.short_name ?? ""}`.toLowerCase().includes(q.toLowerCase()) : true,
  );
  const showEmpty = !schools.isLoading && schools.data && schools.data.length === 0 && !q;

  const columns: EntityColumn<School>[] = [
    {
      key: "name",
      header: t("schools.columns.name"),
      cell: (s) => s.name,
      cellClassName: "font-medium",
    },
    {
      key: "shortName",
      header: t("schools.columns.shortName"),
      cell: (s) => s.short_name ?? "—",
      cellClassName: "font-mono text-[12.5px]",
    },
  ];

  return (
    <div className="space-y-4">
      <EntityPageHead
        title={t("schools.title")}
        subtitle={t("schools.subtitle")}
        onCreate={() => setCreating(true)}
        createLabel={t("schools.new")}
      />
      {schools.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : schools.isError ? (
        <p className="text-sm text-destructive">{t("schools.loadError")}</p>
      ) : showEmpty ? (
        <EmptyState
          icon={<GraduationCap className="h-7 w-7" />}
          title={t("schools.empty.title")}
          body={t("schools.empty.body")}
          steps={[t("schools.empty.step1"), t("schools.empty.step2"), t("schools.empty.step3")]}
          onCreate={() => setCreating(true)}
          createLabel={t("schools.new")}
        />
      ) : (
        <>
          <Toolbar
            search={q}
            onSearch={setQ}
            placeholder={t("common.search")}
            right={
              <span className="font-mono text-xs text-muted-foreground">
                {rows.length} {t("schools.title").toLowerCase()}
              </span>
            }
          />
          <EntityListTable<School>
            rows={rows}
            rowKey={(s) => s.id}
            columns={columns}
            actions={(s) => (
              <>
                <Button size="sm" variant="outline" onClick={() => setEditing(s)}>
                  {t("common.edit")}
                </Button>
                <Button size="sm" variant="destructive" onClick={() => setConfirmDelete(s)}>
                  {t("common.delete")}
                </Button>
              </>
            )}
            actionsHeader={t("common.actions")}
          />
        </>
      )}

      <SchoolFormDialog
        open={creating}
        onOpenChange={setCreating}
        submitLabel={t("common.create")}
      />
      {editing ? (
        <SchoolFormDialog
          open={true}
          school={editing}
          onOpenChange={(open) => {
            if (!open) setEditing(null);
          }}
          submitLabel={t("common.save")}
        />
      ) : null}
      {confirmDelete ? (
        <DeleteSchoolDialog school={confirmDelete} onClose={() => setConfirmDelete(null)} />
      ) : null}
    </div>
  );
}
