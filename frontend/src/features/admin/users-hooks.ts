import { useQuery } from "@tanstack/react-query";
import { client } from "@/lib/api-client";
import type { components } from "@/lib/api-types";

export type AdminUser = components["schemas"]["UserListItem"];

export const adminUsersQueryKey = ["admin-users"] as const;

export function useAdminUsers() {
  return useQuery({
    queryKey: adminUsersQueryKey,
    queryFn: async (): Promise<AdminUser[]> => {
      const { data } = await client.GET("/api/auth/admin/users");
      return data ?? [];
    },
  });
}
