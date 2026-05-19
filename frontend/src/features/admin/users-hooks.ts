import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError, client } from "@/lib/api-client";
import type { components } from "@/lib/api-types";

export type AdminUser = components["schemas"]["UserListItem"];
export type Membership = components["schemas"]["MembershipListItem"];

export const adminUsersQueryKey = ["admin-users"] as const;
export const userMembershipsQueryKey = (userId: string) =>
  ["admin-user-memberships", userId] as const;

export function useAdminUsers() {
  return useQuery({
    queryKey: adminUsersQueryKey,
    queryFn: async (): Promise<AdminUser[]> => {
      const { data } = await client.GET("/api/auth/admin/users");
      return data ?? [];
    },
  });
}

export function useUserMemberships(userId: string | null) {
  return useQuery({
    queryKey: userId ? userMembershipsQueryKey(userId) : ["admin-user-memberships", "__none__"],
    enabled: userId !== null,
    queryFn: async (): Promise<Membership[]> => {
      if (!userId) return [];
      const { data } = await client.GET("/api/auth/admin/users/{user_id}/memberships", {
        params: { path: { user_id: userId } },
      });
      return data ?? [];
    },
  });
}

export function useGrantMembership(userId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (schoolId: string) => {
      const { data } = await client.POST("/api/auth/admin/users/{user_id}/memberships", {
        params: { path: { user_id: userId } },
        body: { school_id: schoolId },
      });
      if (!data) throw new ApiError(500, null, "Empty response from grant");
      return data;
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: userMembershipsQueryKey(userId) }),
  });
}

export function useRevokeMembership(userId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (schoolId: string) => {
      await client.DELETE("/api/auth/admin/users/{user_id}/memberships/{school_id}", {
        params: { path: { user_id: userId, school_id: schoolId } },
      });
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: userMembershipsQueryKey(userId) }),
  });
}
