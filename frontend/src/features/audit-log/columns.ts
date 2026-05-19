import type { TFunction } from "i18next";
import type { EntityColumn } from "@/components/entity-list-table";
import type { AuditLogEntry } from "./hooks";

const PLACEHOLDER = "—";

export function buildAuditLogColumns(t: TFunction, locale: string): EntityColumn<AuditLogEntry>[] {
  const fmt = new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  return [
    {
      key: "ts",
      header: t("auditLog.columns.ts"),
      cell: (r) => fmt.format(new Date(r.ts)),
    },
    {
      key: "actor",
      header: t("auditLog.columns.actor"),
      cell: (r) => r.actor_user_email,
    },
    {
      key: "method",
      header: t("auditLog.columns.method"),
      cell: (r) => r.method,
    },
    {
      key: "route",
      header: t("auditLog.columns.route"),
      cell: (r) => r.route_template,
      cellClassName: "font-mono text-[12.5px]",
    },
    {
      key: "school",
      header: t("auditLog.columns.school"),
      cell: (r) => r.target_school_name ?? PLACEHOLDER,
    },
    {
      key: "status",
      header: t("auditLog.columns.status"),
      cell: (r) => r.response_status,
    },
  ];
}
