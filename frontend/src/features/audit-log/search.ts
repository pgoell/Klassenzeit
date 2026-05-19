import { z } from "zod";

export const AuditLogSearchSchema = z.object({
  skip: z.coerce.number().int().min(0).default(0),
  limit: z.coerce.number().int().min(1).max(200).default(50),
  actor_user_id: z.string().min(1).optional(),
  target_school_id: z.string().min(1).optional(),
  from_ts: z.string().min(1).optional(),
  to_ts: z.string().min(1).optional(),
});

export type AuditLogSearch = z.infer<typeof AuditLogSearchSchema>;
