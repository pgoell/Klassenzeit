import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/lib/api-client";
import {
  roomSchedulesByRoomId,
  scheduleByClassId,
  server,
  teacherSchedulesByTeacherId,
  violationsByClassId,
} from "../../../tests/msw-handlers";
import {
  scheduleQueryKey,
  useClassSchedule,
  useGenerateAllSchedules,
  useGenerateClassSchedule,
  useMovePlacement,
  usePinPlacement,
  useRoomSchedule,
  useSwapPlacements,
  useTeacherSchedule,
} from "./hooks";

const CLASS_ID = "00000000-0000-0000-0000-00000000a001";

function wrapScheduleHook() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return { client, wrapper };
}

describe("useClassSchedule", () => {
  beforeEach(() => {
    for (const key of Object.keys(scheduleByClassId)) delete scheduleByClassId[key];
    for (const key of Object.keys(violationsByClassId)) delete violationsByClassId[key];
  });

  it("returns the placements seeded for the class", async () => {
    scheduleByClassId[CLASS_ID] = [
      {
        lesson_id: "00000000-0000-0000-0000-00000000b001",
        teacher_id: "00000000-0000-0000-0000-00000000e001",
        time_block_id: "00000000-0000-0000-0000-00000000c001",
        room_id: "00000000-0000-0000-0000-00000000d001",
        pinned: false,
      },
    ];
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useClassSchedule(CLASS_ID), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.placements).toHaveLength(1);
  });

  it("returns an empty placement list for a never-solved class", async () => {
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useClassSchedule(CLASS_ID), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.placements).toEqual([]);
  });

  it("surfaces a 404 as ApiError when the class id is unknown", async () => {
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useClassSchedule("deadbeef-dead-beef-dead-beefdeadbeef"), {
      wrapper,
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.error).toBeInstanceOf(ApiError);
    expect((result.current.error as ApiError).status).toBe(404);
  });

  it("stays disabled while classId is undefined (no request)", async () => {
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useClassSchedule(undefined), { wrapper });
    expect(result.current.fetchStatus).toBe("idle");
    expect(result.current.isPending).toBe(true);
  });
});

describe("useGenerateClassSchedule", () => {
  beforeEach(() => {
    for (const key of Object.keys(scheduleByClassId)) delete scheduleByClassId[key];
    for (const key of Object.keys(violationsByClassId)) delete violationsByClassId[key];
  });

  it("posts and writes placements into the GET cache", async () => {
    scheduleByClassId[CLASS_ID] = [];
    violationsByClassId[CLASS_ID] = [];
    const { client, wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useGenerateClassSchedule(), { wrapper });
    const response = await result.current.mutateAsync(CLASS_ID);
    expect(response.placements).toBeDefined();
    const cached = client.getQueryData(scheduleQueryKey(CLASS_ID));
    expect(cached).toEqual({ placements: response.placements });
  });
});

describe("useGenerateAllSchedules", () => {
  beforeEach(() => {
    for (const key of Object.keys(scheduleByClassId)) delete scheduleByClassId[key];
    for (const key of Object.keys(violationsByClassId)) delete violationsByClassId[key];
  });

  it("posts to /api/schedule/all and returns the WholeSchoolScheduleResponse", async () => {
    scheduleByClassId[CLASS_ID] = [
      {
        lesson_id: "00000000-0000-0000-0000-00000000b001",
        teacher_id: "00000000-0000-0000-0000-00000000e001",
        time_block_id: "00000000-0000-0000-0000-00000000c001",
        room_id: "00000000-0000-0000-0000-00000000d001",
        pinned: false,
      },
    ];
    violationsByClassId[CLASS_ID] = [];
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useGenerateAllSchedules(), { wrapper });
    const response = await result.current.mutateAsync({ respect_pins: true });
    expect(response.total_placements).toBe(1);
    expect(response.total_violations).toBe(0);
    expect(response.classes).toEqual([
      { class_id: CLASS_ID, placements_count: 1, violations_count: 0 },
    ]);
  });

  it("threads respect_pins=true onto the request as a query param", async () => {
    let receivedUrl = "";
    server.use(
      http.post("http://localhost:3000/api/schedule/all", ({ request }) => {
        receivedUrl = request.url;
        return HttpResponse.json({
          classes: [],
          total_placements: 0,
          total_violations: 0,
        });
      }),
    );
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useGenerateAllSchedules(), { wrapper });
    await result.current.mutateAsync({ respect_pins: true });
    expect(receivedUrl).toContain("respect_pins=true");
  });

  it("threads respect_pins=false onto the request as a query param", async () => {
    let receivedUrl = "";
    server.use(
      http.post("http://localhost:3000/api/schedule/all", ({ request }) => {
        receivedUrl = request.url;
        return HttpResponse.json({
          classes: [],
          total_placements: 0,
          total_violations: 0,
        });
      }),
    );
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useGenerateAllSchedules(), { wrapper });
    await result.current.mutateAsync({ respect_pins: false });
    expect(receivedUrl).toContain("respect_pins=false");
  });

  it("invalidates ['schedule'] queries on success so open class schedules refetch", async () => {
    scheduleByClassId[CLASS_ID] = [];
    violationsByClassId[CLASS_ID] = [];
    const { client, wrapper } = wrapScheduleHook();
    // Seed a cached entry under the schedule prefix; the mutation's onSuccess
    // should mark it stale, triggering a refetch on next observer.
    client.setQueryData(scheduleQueryKey(CLASS_ID), { placements: [] });
    const { result } = renderHook(() => useGenerateAllSchedules(), { wrapper });
    await result.current.mutateAsync({ respect_pins: true });
    const state = client.getQueryState(scheduleQueryKey(CLASS_ID));
    expect(state?.isInvalidated).toBe(true);
  });
});

describe("useTeacherSchedule", () => {
  beforeEach(() => {
    for (const k of Object.keys(teacherSchedulesByTeacherId)) {
      delete teacherSchedulesByTeacherId[k];
    }
  });

  it("fetches placements for a teacher", async () => {
    const teacherId = "11111111-1111-1111-1111-111111111111";
    const lessonId = "22222222-2222-2222-2222-222222222222";
    const blockId = "33333333-3333-3333-3333-333333333333";
    const roomId = "44444444-4444-4444-4444-444444444444";
    teacherSchedulesByTeacherId[teacherId] = [
      {
        lesson_id: lessonId,
        teacher_id: teacherId,
        time_block_id: blockId,
        room_id: roomId,
        pinned: false,
      },
    ];
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useTeacherSchedule(teacherId), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.placements).toEqual([
      {
        lesson_id: lessonId,
        teacher_id: teacherId,
        time_block_id: blockId,
        room_id: roomId,
        pinned: false,
      },
    ]);
  });

  it("does not fetch when teacherId is undefined", () => {
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useTeacherSchedule(undefined), { wrapper });
    expect(result.current.fetchStatus).toBe("idle");
  });
});

describe("usePinPlacement", () => {
  it("PATCHes the pin endpoint and invalidates ['schedule'] queries", async () => {
    const lessonId = "00000000-0000-0000-0000-000000000001";
    const timeBlockId = "00000000-0000-0000-0000-000000000002";
    const roomId = "00000000-0000-0000-0000-000000000003";
    let receivedBody: unknown = null;
    server.use(
      http.patch(
        `http://localhost:3000/api/placements/${lessonId}/${timeBlockId}/pin`,
        async ({ request }) => {
          receivedBody = await request.json();
          return HttpResponse.json({
            lesson_id: lessonId,
            time_block_id: timeBlockId,
            room_id: roomId,
            pinned: true,
          });
        },
      ),
    );
    const { client, wrapper } = wrapScheduleHook();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => usePinPlacement(), { wrapper });
    const data = await result.current.mutateAsync({
      lesson_id: lessonId,
      time_block_id: timeBlockId,
      pin_kind: "hard",
    });
    expect(receivedBody).toEqual({ pin_kind: "hard" });
    expect(data).toEqual({
      lesson_id: lessonId,
      time_block_id: timeBlockId,
      room_id: roomId,
      pinned: true,
    });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["schedule"] });
  });
});

describe("useMovePlacement", () => {
  it("PATCHes the move endpoint and invalidates ['schedule'] queries", async () => {
    const lessonId = "00000000-0000-0000-0000-000000000010";
    const sourceBlockId = "00000000-0000-0000-0000-000000000020";
    const targetBlockId = "00000000-0000-0000-0000-000000000021";
    const targetRoomId = "00000000-0000-0000-0000-000000000030";
    let receivedBody: unknown = null;
    let receivedUrl = "";
    server.use(
      http.patch(
        `http://localhost:3000/api/placements/${lessonId}/${sourceBlockId}`,
        async ({ request }) => {
          receivedUrl = request.url;
          receivedBody = await request.json();
          return HttpResponse.json({
            lesson_id: lessonId,
            time_block_id: targetBlockId,
            room_id: targetRoomId,
            pinned: true,
          });
        },
      ),
    );
    const { client, wrapper } = wrapScheduleHook();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useMovePlacement(), { wrapper });
    const data = await result.current.mutateAsync({
      lesson_id: lessonId,
      source_time_block_id: sourceBlockId,
      time_block_id: targetBlockId,
      room_id: targetRoomId,
    });
    expect(receivedUrl).toContain(`/api/placements/${lessonId}/${sourceBlockId}`);
    expect(receivedBody).toEqual({
      time_block_id: targetBlockId,
      room_id: targetRoomId,
    });
    expect(data).toEqual({
      lesson_id: lessonId,
      time_block_id: targetBlockId,
      room_id: targetRoomId,
      pinned: true,
    });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["schedule"] });
  });
});

describe("useSwapPlacements", () => {
  it("POSTs the swap endpoint and invalidates ['schedule'] queries", async () => {
    const lessonA = "00000000-0000-0000-0000-0000000000a1";
    const blockA = "00000000-0000-0000-0000-0000000000a2";
    const roomA = "00000000-0000-0000-0000-0000000000a3";
    const lessonB = "00000000-0000-0000-0000-0000000000b1";
    const blockB = "00000000-0000-0000-0000-0000000000b2";
    const roomB = "00000000-0000-0000-0000-0000000000b3";
    let receivedBody: unknown = null;
    server.use(
      http.post("http://localhost:3000/api/placements/swap", async ({ request }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          a: { lesson_id: lessonA, time_block_id: blockB, room_id: roomB, pinned: true },
          b: { lesson_id: lessonB, time_block_id: blockA, room_id: roomA, pinned: true },
        });
      }),
    );
    const { client, wrapper } = wrapScheduleHook();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = renderHook(() => useSwapPlacements(), { wrapper });
    const data = await result.current.mutateAsync({
      a: { lesson_id: lessonA, time_block_id: blockA },
      b: { lesson_id: lessonB, time_block_id: blockB },
    });
    expect(receivedBody).toEqual({
      a: { lesson_id: lessonA, time_block_id: blockA },
      b: { lesson_id: lessonB, time_block_id: blockB },
    });
    expect(data.a.lesson_id).toBe(lessonA);
    expect(data.a.time_block_id).toBe(blockB);
    expect(data.b.lesson_id).toBe(lessonB);
    expect(data.b.time_block_id).toBe(blockA);
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["schedule"] });
  });
});

describe("useRoomSchedule", () => {
  beforeEach(() => {
    for (const k of Object.keys(roomSchedulesByRoomId)) {
      delete roomSchedulesByRoomId[k];
    }
  });

  it("fetches placements for a room", async () => {
    const roomId = "55555555-5555-5555-5555-555555555555";
    const lessonId = "66666666-6666-6666-6666-666666666666";
    const blockId = "77777777-7777-7777-7777-777777777777";
    const teacherId = "88888888-8888-8888-8888-888888888888";
    roomSchedulesByRoomId[roomId] = [
      {
        lesson_id: lessonId,
        teacher_id: teacherId,
        time_block_id: blockId,
        room_id: roomId,
        pinned: false,
      },
    ];
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useRoomSchedule(roomId), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.placements).toEqual([
      {
        lesson_id: lessonId,
        teacher_id: teacherId,
        time_block_id: blockId,
        room_id: roomId,
        pinned: false,
      },
    ]);
  });

  it("does not fetch when roomId is undefined", () => {
    const { wrapper } = wrapScheduleHook();
    const { result } = renderHook(() => useRoomSchedule(undefined), { wrapper });
    expect(result.current.fetchStatus).toBe("idle");
  });
});
