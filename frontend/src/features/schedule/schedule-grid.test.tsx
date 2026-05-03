import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeAll, describe, expect, it } from "vitest";
import i18n from "@/i18n/init";
import { type ScheduleCell, ScheduleGrid } from "./schedule-grid";

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

function wrapScheduleGrid() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

function samplePinnedCell(overrides: Partial<ScheduleCell> = {}): ScheduleCell {
  return {
    key: "0:1",
    day: 0,
    position: 1,
    subjectName: "Mathematics",
    teacherName: "Mueller",
    roomName: "Room 101",
    lessonId: "00000000-0000-0000-0000-00000000b001",
    timeBlockId: "00000000-0000-0000-0000-00000000c001",
    pinned: true,
    ...overrides,
  };
}

describe("ScheduleGrid", () => {
  it("renders a day header for every day present and a row for every position", () => {
    const cells: ScheduleCell[] = [
      {
        key: "0-1",
        day: 0,
        position: 1,
        subjectName: "Mathematics",
        teacherName: "Mueller",
        roomName: "Room 101",
      },
      {
        key: "1-2",
        day: 1,
        position: 2,
        subjectName: "German",
        teacherName: "Schmidt",
        roomName: "Room 102",
      },
    ];
    const Wrapper = wrapScheduleGrid();
    render(
      <Wrapper>
        <ScheduleGrid cells={cells} daysPresent={[0, 1]} positions={[1, 2]} />
      </Wrapper>,
    );
    expect(screen.getByText("Mon")).toBeInTheDocument();
    expect(screen.getByText("Tue")).toBeInTheDocument();
    expect(screen.getByText("P1")).toBeInTheDocument();
    expect(screen.getByText("P2")).toBeInTheDocument();
    expect(screen.getByText("Mathematics")).toBeInTheDocument();
    expect(screen.getByText("German")).toBeInTheDocument();
  });

  it("renders an empty cell when no placement exists at (day, position)", () => {
    const Wrapper = wrapScheduleGrid();
    render(
      <Wrapper>
        <ScheduleGrid cells={[]} daysPresent={[0]} positions={[1]} />
      </Wrapper>,
    );
    const cells = document.querySelectorAll<HTMLElement>(".kz-ws-cell");
    expect(cells.length).toBeGreaterThan(0);
    for (const cell of cells) {
      const text = cell.textContent ?? "";
      expect(text === "Mon" || text === "P1" || text === "").toBe(true);
    }
  });

  it("renders an unpin button on pinned cells and a pin button on unpinned cells", () => {
    const pinned = samplePinnedCell({ key: "0:1", day: 0, position: 1, pinned: true });
    const unpinned = samplePinnedCell({
      key: "1:2",
      day: 1,
      position: 2,
      subjectName: "German",
      lessonId: "00000000-0000-0000-0000-00000000b002",
      timeBlockId: "00000000-0000-0000-0000-00000000c002",
      pinned: false,
    });
    const Wrapper = wrapScheduleGrid();
    render(
      <Wrapper>
        <ScheduleGrid cells={[pinned, unpinned]} daysPresent={[0, 1]} positions={[1, 2]} />
      </Wrapper>,
    );
    expect(screen.getByRole("button", { name: "Unpin this lesson" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Pin this lesson" })).toBeInTheDocument();
  });

  it("highlights pinned cells with a primary border", () => {
    const Wrapper = wrapScheduleGrid();
    render(
      <Wrapper>
        <ScheduleGrid
          cells={[samplePinnedCell({ pinned: true })]}
          daysPresent={[0]}
          positions={[1]}
        />
      </Wrapper>,
    );
    const card = screen.getByText("Mathematics").closest(".kz-ws-cell");
    expect(card).not.toBeNull();
    expect(card?.className ?? "").toContain("kz-ws-cell--pinned");
  });

  it("does not render a pin button on cells lacking lessonId/timeBlockId", () => {
    const Wrapper = wrapScheduleGrid();
    render(
      <Wrapper>
        <ScheduleGrid
          cells={[
            {
              key: "0:1",
              day: 0,
              position: 1,
              subjectName: "Mathematics",
              roomName: "Room 101",
            },
          ]}
          daysPresent={[0]}
          positions={[1]}
        />
      </Wrapper>,
    );
    expect(screen.queryByRole("button", { name: /pin/i })).not.toBeInTheDocument();
  });
});
