import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useScheduleDragAndDrop } from "./use-schedule-drag-and-drop";

describe("useScheduleDragAndDrop", () => {
  it("dispatches move when dropping onto an empty slot", () => {
    const move = vi.fn();
    const swap = vi.fn();
    const { result } = renderHook(() =>
      useScheduleDragAndDrop({
        moveMutate: move,
        swapMutate: swap,
        placementByCell: new Map([
          ["TB1::R1", { lesson_id: "L1", time_block_id: "TB1", room_id: "R1" }],
        ]),
      }),
    );
    act(() => {
      result.current.onDragEnd({
        active: {
          id: "L1::TB1",
          data: { current: { lesson_id: "L1", source_time_block_id: "TB1" } },
        },
        over: { id: "TB2::R2", data: { current: { time_block_id: "TB2", room_id: "R2" } } },
      } as never);
    });
    expect(move).toHaveBeenCalledWith({
      lesson_id: "L1",
      source_time_block_id: "TB1",
      time_block_id: "TB2",
      room_id: "R2",
    });
    expect(swap).not.toHaveBeenCalled();
  });

  it("dispatches swap when dropping onto an occupied slot", () => {
    const move = vi.fn();
    const swap = vi.fn();
    const { result } = renderHook(() =>
      useScheduleDragAndDrop({
        moveMutate: move,
        swapMutate: swap,
        placementByCell: new Map([
          ["TB1::R1", { lesson_id: "L1", time_block_id: "TB1", room_id: "R1" }],
          ["TB2::R2", { lesson_id: "L2", time_block_id: "TB2", room_id: "R2" }],
        ]),
      }),
    );
    act(() => {
      result.current.onDragEnd({
        active: {
          id: "L1::TB1",
          data: { current: { lesson_id: "L1", source_time_block_id: "TB1" } },
        },
        over: { id: "TB2::R2", data: { current: { time_block_id: "TB2", room_id: "R2" } } },
      } as never);
    });
    expect(swap).toHaveBeenCalledWith({
      a: { lesson_id: "L1", time_block_id: "TB1" },
      b: { lesson_id: "L2", time_block_id: "TB2" },
    });
    expect(move).not.toHaveBeenCalled();
  });

  it("is a no-op when over is null", () => {
    const move = vi.fn();
    const swap = vi.fn();
    const { result } = renderHook(() =>
      useScheduleDragAndDrop({
        moveMutate: move,
        swapMutate: swap,
        placementByCell: new Map(),
      }),
    );
    act(() => {
      result.current.onDragEnd({
        active: {
          id: "L1::TB1",
          data: { current: { lesson_id: "L1", source_time_block_id: "TB1" } },
        },
        over: null,
      } as never);
    });
    expect(move).not.toHaveBeenCalled();
    expect(swap).not.toHaveBeenCalled();
  });
});
