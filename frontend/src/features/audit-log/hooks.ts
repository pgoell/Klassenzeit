import { useQuery } from "@tanstack/react-query";
import { ApiError, client } from "@/lib/api-client";
import type { components } from "@/lib/api-types";
import type { AuditLogSearch } from "./search";

export type AuditLogEntry = components["schemas"]["AuditLogEntryItem"];
export type AuditLogResponse = components["schemas"]["AuditLogListResponse"];
export type AuditLogDetail = components["schemas"]["AuditLogEntryDetail"];

export const auditLogQueryKey = (search: AuditLogSearch) => ["audit-log", search] as const;
export const auditLogDetailQueryKey = (id: string | null) => ["audit-log", "detail", id] as const;

export function useAuditLog(search: AuditLogSearch) {
  return useQuery({
    queryKey: auditLogQueryKey(search),
    queryFn: async (): Promise<AuditLogResponse> => {
      const { data } = await client.GET("/api/auth/admin/audit-log", {
        params: { query: search },
      });
      if (!data) {
        throw new ApiError(500, null, "Empty response from /auth/admin/audit-log");
      }
      return data;
    },
  });
}

export function useAuditLogDetail(auditLogId: string | null) {
  return useQuery({
    queryKey: auditLogDetailQueryKey(auditLogId),
    enabled: auditLogId !== null,
    queryFn: async (): Promise<AuditLogDetail> => {
      const { data } = await client.GET("/api/auth/admin/audit-log/{audit_log_id}", {
        params: { path: { audit_log_id: auditLogId as string } },
      });
      if (!data) {
        throw new ApiError(500, null, "Empty response from /auth/admin/audit-log detail");
      }
      return data;
    },
  });
}
