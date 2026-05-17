import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError, client } from "@/lib/api-client";
import type { components } from "@/lib/api-types";

export type School = components["schemas"]["SchoolListItem"];
export type SchoolDetail = components["schemas"]["SchoolResponse"];
export type SchoolCreate = components["schemas"]["SchoolCreate"];
export type SchoolUpdate = components["schemas"]["SchoolUpdate"];

export const schoolsQueryKey = ["schools"] as const;

export function useSchools() {
  return useQuery({
    queryKey: schoolsQueryKey,
    queryFn: async (): Promise<School[]> => {
      const { data } = await client.GET("/api/schools");
      if (!data) throw new ApiError(500, null, "Empty response from /schools");
      return data;
    },
  });
}

export function useCreateSchool() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: SchoolCreate): Promise<SchoolDetail> => {
      const { data } = await client.POST("/api/schools", { body });
      if (!data) throw new ApiError(500, null, "Empty response from POST /schools");
      return data;
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: schoolsQueryKey }),
  });
}

export function useUpdateSchool() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (args: { id: string; body: SchoolUpdate }): Promise<SchoolDetail> => {
      const { data } = await client.PATCH("/api/schools/{school_id}", {
        params: { path: { school_id: args.id } },
        body: args.body,
      });
      if (!data) throw new ApiError(500, null, "Empty response from PATCH /schools/{id}");
      return data;
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: schoolsQueryKey }),
  });
}

export function useDeleteSchool() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string): Promise<void> => {
      await client.DELETE("/api/schools/{school_id}", {
        params: { path: { school_id: id } },
      });
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: schoolsQueryKey }),
  });
}
