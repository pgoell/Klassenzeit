import { useNavigate, useSearch } from "@tanstack/react-router";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { EntityListTable } from "@/components/entity-list-table";
import { Button } from "@/components/ui/button";
import { AuditLogDetailDialog } from "./audit-log-detail-dialog";
import { AuditLogFilters } from "./audit-log-filters";
import { buildAuditLogColumns } from "./columns";
import { useAuditLog } from "./hooks";
import { type AuditLogSearch, AuditLogSearchSchema } from "./search";

export function AuditLogPage() {
  const { t, i18n } = useTranslation();
  const rawSearch = useSearch({ strict: false });
  const search: AuditLogSearch = AuditLogSearchSchema.parse(rawSearch ?? {});
  const navigate = useNavigate();
  const query = useAuditLog(search);
  const columns = buildAuditLogColumns(t, i18n.language);
  const [selectedRowId, setSelectedRowId] = useState<string | null>(null);

  const items = query.data?.items ?? [];
  const total = query.data?.total ?? 0;
  const from = items.length > 0 ? search.skip + 1 : 0;
  const to = search.skip + items.length;

  function paginate(deltaSkip: number) {
    void navigate({
      to: "/audit-log",
      search: { ...search, skip: Math.max(0, search.skip + deltaSkip) },
    });
  }

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">{t("auditLog.title")}</h1>
        <p className="mt-1 text-sm text-muted-foreground">{t("auditLog.subtitle")}</p>
      </div>
      <AuditLogFilters search={search} />
      {items.length === 0 && !query.isLoading ? (
        <p className="rounded-md border border-dashed bg-card px-6 py-12 text-center text-sm text-muted-foreground">
          {t("auditLog.empty")}
        </p>
      ) : (
        <EntityListTable
          rows={items}
          rowKey={(r) => r.id}
          columns={columns}
          actionsHeader={t("auditLog.detail.actionsHeader")}
          actions={(row) => (
            <Button size="sm" variant="ghost" onClick={() => setSelectedRowId(row.id)}>
              {t("auditLog.detail.actionLabel")}
            </Button>
          )}
        />
      )}
      <div className="flex items-center justify-between text-sm text-muted-foreground">
        <span>{t("auditLog.pagination.showing", { from, to, total })}</span>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            disabled={search.skip <= 0}
            onClick={() => paginate(-search.limit)}
          >
            {t("auditLog.pagination.prev")}
          </Button>
          <Button
            variant="outline"
            disabled={search.skip + search.limit >= total}
            onClick={() => paginate(search.limit)}
          >
            {t("auditLog.pagination.next")}
          </Button>
        </div>
      </div>
      <AuditLogDetailDialog auditLogId={selectedRowId} onClose={() => setSelectedRowId(null)} />
    </div>
  );
}
