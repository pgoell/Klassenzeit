import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError, client } from "@/lib/api-client";
import type { components } from "@/lib/api-types";

export type Placement = components["schemas"]["PlacementResponse"];
export type Violation = components["schemas"]["ViolationResponse"];
export type SchedulePostResponse = components["schemas"]["ScheduleResponse"];
export type ScheduleGetResponse = components["schemas"]["ScheduleReadResponse"];
export type WholeSchoolScheduleResponse = components["schemas"]["WholeSchoolScheduleResponse"];
export type SwapPlacementsResponse = components["schemas"]["SwapPlacementsResponse"];

export function scheduleQueryKey(classId: string) {
  return ["schedule", classId] as const;
}

export function useClassSchedule(classId: string | undefined) {
  return useQuery({
    enabled: Boolean(classId),
    queryKey: classId ? scheduleQueryKey(classId) : ["schedule", "disabled"],
    queryFn: async (): Promise<ScheduleGetResponse> => {
      if (!classId) {
        throw new ApiError(400, null, "useClassSchedule called without classId");
      }
      const { data } = await client.GET("/api/classes/{class_id}/schedule", {
        params: { path: { class_id: classId } },
      });
      if (!data) {
        throw new ApiError(500, null, "Empty response from GET /schedule");
      }
      return data;
    },
  });
}

export function useGenerateClassSchedule() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (classId: string): Promise<SchedulePostResponse> => {
      const { data } = await client.POST("/api/classes/{class_id}/schedule", {
        params: { path: { class_id: classId } },
      });
      if (!data) {
        throw new ApiError(500, null, "Empty response from POST /schedule");
      }
      return data;
    },
    onSuccess: (result, classId) => {
      queryClient.setQueryData(scheduleQueryKey(classId), {
        placements: result.placements,
      } satisfies ScheduleGetResponse);
    },
  });
}

export function teacherScheduleQueryKey(teacherId: string) {
  return ["schedule", "teacher", teacherId] as const;
}

export function roomScheduleQueryKey(roomId: string) {
  return ["schedule", "room", roomId] as const;
}

export function useTeacherSchedule(teacherId: string | undefined) {
  return useQuery({
    enabled: Boolean(teacherId),
    queryKey: teacherId ? teacherScheduleQueryKey(teacherId) : ["schedule", "teacher", "disabled"],
    queryFn: async (): Promise<ScheduleGetResponse> => {
      if (!teacherId) {
        throw new ApiError(400, null, "useTeacherSchedule called without teacherId");
      }
      const { data } = await client.GET("/api/teachers/{teacher_id}/schedule", {
        params: { path: { teacher_id: teacherId } },
      });
      if (!data) {
        throw new ApiError(500, null, "Empty response from GET /teachers/{id}/schedule");
      }
      return data;
    },
  });
}

export function useRoomSchedule(roomId: string | undefined) {
  return useQuery({
    enabled: Boolean(roomId),
    queryKey: roomId ? roomScheduleQueryKey(roomId) : ["schedule", "room", "disabled"],
    queryFn: async (): Promise<ScheduleGetResponse> => {
      if (!roomId) {
        throw new ApiError(400, null, "useRoomSchedule called without roomId");
      }
      const { data } = await client.GET("/api/rooms/{room_id}/schedule", {
        params: { path: { room_id: roomId } },
      });
      if (!data) {
        throw new ApiError(500, null, "Empty response from GET /rooms/{id}/schedule");
      }
      return data;
    },
  });
}

export interface PinPlacementVars {
  lesson_id: string;
  time_block_id: string;
  pinned: boolean;
}

export function usePinPlacement() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (vars: PinPlacementVars): Promise<Placement> => {
      const { lesson_id, time_block_id, pinned } = vars;
      const { data } = await client.PATCH("/api/placements/{lesson_id}/{time_block_id}/pin", {
        params: { path: { lesson_id, time_block_id } },
        body: { pinned },
      });
      if (!data) {
        throw new ApiError(500, null, "Empty response from PATCH /placements/{ids}/pin");
      }
      return data;
    },
    onSuccess: () => {
      // Broad invalidation: a pin toggle can affect class, teacher, and room
      // schedule views, so refetch every schedule subkey.
      queryClient.invalidateQueries({ queryKey: ["schedule"] });
    },
  });
}

export interface MovePlacementVars {
  lesson_id: string;
  source_time_block_id: string;
  time_block_id: string;
  room_id: string;
}

export function useMovePlacement() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (vars: MovePlacementVars): Promise<Placement> => {
      const { lesson_id, source_time_block_id, time_block_id, room_id } = vars;
      const { data } = await client.PATCH("/api/placements/{lesson_id}/{time_block_id}", {
        params: { path: { lesson_id, time_block_id: source_time_block_id } },
        body: { time_block_id, room_id },
      });
      if (!data) {
        throw new ApiError(500, null, "Empty response from PATCH /placements/{ids}");
      }
      return data;
    },
    onSuccess: () => {
      // Broad invalidation: a move can shift the lesson out of one class,
      // teacher, or room view and into another.
      queryClient.invalidateQueries({ queryKey: ["schedule"] });
    },
  });
}

export interface SwapPlacementsVars {
  a: { lesson_id: string; time_block_id: string };
  b: { lesson_id: string; time_block_id: string };
}

export function useSwapPlacements() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (vars: SwapPlacementsVars): Promise<SwapPlacementsResponse> => {
      const { data } = await client.POST("/api/placements/swap", { body: vars });
      if (!data) {
        throw new ApiError(500, null, "Empty response from POST /placements/swap");
      }
      return data;
    },
    onSuccess: () => {
      // Broad invalidation: a swap touches two placements that may live in
      // different class, teacher, or room views.
      queryClient.invalidateQueries({ queryKey: ["schedule"] });
    },
  });
}

export interface GenerateAllSchedulesVars {
  respect_pins: boolean;
}

export function useGenerateAllSchedules() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (vars: GenerateAllSchedulesVars): Promise<WholeSchoolScheduleResponse> => {
      const { data } = await client.POST("/api/schedule/all", {
        params: { query: { respect_pins: vars.respect_pins } },
      });
      if (!data) {
        throw new ApiError(500, null, "Empty response from POST /schedule/all");
      }
      return data;
    },
    onSuccess: () => {
      // Invalidate every per-class schedule query so any open class view
      // refetches its placements after a whole-school re-solve.
      queryClient.invalidateQueries({ queryKey: ["schedule"] });
    },
  });
}
