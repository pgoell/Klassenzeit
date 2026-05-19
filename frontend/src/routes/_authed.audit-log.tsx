import { createFileRoute } from "@tanstack/react-router";
import { AuditLogPage } from "@/features/audit-log/audit-log-page";
import { AuditLogSearchSchema } from "@/features/audit-log/search";

export const Route = createFileRoute("/_authed/audit-log")({
  component: AuditLogPage,
  validateSearch: (raw) => AuditLogSearchSchema.parse(raw),
});
