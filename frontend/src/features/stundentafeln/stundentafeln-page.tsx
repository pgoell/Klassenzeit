import { useSearch } from "@tanstack/react-router";
import { ClipboardList } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/empty-state";
import { type EntityColumn, EntityListTable } from "@/components/entity-list-table";
import { EntityPageHead } from "@/components/entity-page-head";
import { Toolbar } from "@/components/toolbar";
import { Button } from "@/components/ui/button";
import { type Stundentafel, useStundentafeln } from "./hooks";
import {
  DeleteStundentafelDialog,
  StundentafelEditDialog,
  StundentafelFormDialog,
} from "./stundentafeln-dialogs";

export function StundentafelnPage() {
  const { t } = useTranslation();
  const stundentafeln = useStundentafeln();
  const search = useSearch({ strict: false }) as { create?: string };

  const [q, setQ] = useState("");
  const [creating, setCreating] = useState(() => search.create === "1");
  const [editing, setEditing] = useState<Stundentafel | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Stundentafel | null>(null);

  const rows = (stundentafeln.data ?? []).filter((row) => {
    if (!q) return true;
    return row.name.toLowerCase().includes(q.toLowerCase());
  });
  const showEmpty =
    !stundentafeln.isLoading && stundentafeln.data && stundentafeln.data.length === 0 && !q;

  const stundentafelColumns: EntityColumn<Stundentafel>[] = [
    {
      key: "name",
      header: t("stundentafeln.columns.name"),
      cell: (tafel) => tafel.name,
      cellClassName: "font-medium",
    },
    {
      key: "gradeLevel",
      header: t("stundentafeln.columns.gradeLevel"),
      cell: (tafel) => tafel.grade_level,
      className: "text-right",
      cellClassName: "font-mono text-[12.5px]",
    },
  ];

  return (
    <div className="space-y-4">
      <EntityPageHead
        title={t("stundentafeln.title")}
        subtitle={t("stundentafeln.subtitle")}
        onCreate={() => setCreating(true)}
        createLabel={t("stundentafeln.new")}
      />

      {stundentafeln.isLoading ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : stundentafeln.isError ? (
        <p className="text-sm text-destructive">{t("stundentafeln.loadError")}</p>
      ) : showEmpty ? (
        <EmptyState
          icon={<ClipboardList className="h-7 w-7" />}
          title={t("stundentafeln.empty.title")}
          body={t("stundentafeln.empty.body")}
          steps={[
            t("stundentafeln.empty.step1"),
            t("stundentafeln.empty.step2"),
            t("stundentafeln.empty.step3"),
          ]}
          onCreate={() => setCreating(true)}
          createLabel={t("stundentafeln.new")}
        />
      ) : (
        <>
          <Toolbar
            search={q}
            onSearch={setQ}
            placeholder={t("common.search")}
            right={
              <span className="font-mono text-xs text-muted-foreground">
                {rows.length} {t("stundentafeln.title").toLowerCase()}
              </span>
            }
          />
          <EntityListTable<Stundentafel>
            rows={rows}
            rowKey={(tafel) => tafel.id}
            columns={stundentafelColumns}
            actions={(tafel) => (
              <>
                <Button size="sm" variant="outline" onClick={() => setEditing(tafel)}>
                  {t("common.edit")}
                </Button>
                <Button size="sm" variant="destructive" onClick={() => setConfirmDelete(tafel)}>
                  {t("common.delete")}
                </Button>
              </>
            )}
            actionsHeader={t("common.actions")}
          />
        </>
      )}

      <StundentafelFormDialog open={creating} onOpenChange={setCreating} />
      {editing ? (
        <StundentafelEditDialog stundentafel={editing} onClose={() => setEditing(null)} />
      ) : null}
      {confirmDelete ? (
        <DeleteStundentafelDialog
          stundentafel={confirmDelete}
          onClose={() => setConfirmDelete(null)}
        />
      ) : null}
    </div>
  );
}
