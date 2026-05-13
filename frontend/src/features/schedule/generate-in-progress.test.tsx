import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n/init";
import { GenerateInProgress } from "./generate-in-progress";
import { useCancelSchedule, useScheduleProgress } from "./hooks";

const stubSnapshot = {
  iter: 12345,
  placement_count: 47,
  total_lessons: 120,
  best_score: 250,
  is_feasible: false,
  cancel_requested: false,
  elapsed_ms: 2100,
  deadline_ms: 5000,
};

vi.mock("./hooks", () => ({
  useScheduleProgress: vi.fn(),
  useCancelSchedule: vi.fn(),
}));

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

function wrapInProgress(node: ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return <QueryClientProvider client={client}>{node}</QueryClientProvider>;
}

function mockProgress(
  snapshot: typeof stubSnapshot | null = stubSnapshot,
  cancelOverrides?: { mutate?: ReturnType<typeof vi.fn>; isPending?: boolean },
) {
  vi.mocked(useScheduleProgress).mockReturnValue({
    data: snapshot,
    isLoading: false,
  } as unknown as ReturnType<typeof useScheduleProgress>);
  vi.mocked(useCancelSchedule).mockReturnValue({
    mutate: cancelOverrides?.mutate ?? vi.fn(),
    isPending: cancelOverrides?.isPending ?? false,
  } as unknown as ReturnType<typeof useCancelSchedule>);
}

const CLASS_ID = "11111111-1111-1111-1111-111111111111";

describe("GenerateInProgress", () => {
  it("renders the placed-count badge", () => {
    mockProgress();
    render(wrapInProgress(<GenerateInProgress classId={CLASS_ID} />));
    expect(screen.getByText(/47 \/ 120/)).toBeInTheDocument();
  });

  it("renders the Stop button", () => {
    mockProgress();
    render(wrapInProgress(<GenerateInProgress classId={CLASS_ID} />));
    expect(screen.getByRole("button", { name: /^stop$/i })).toBeInTheDocument();
  });

  it("clicking Stop calls the cancel mutation", () => {
    const mutate = vi.fn();
    mockProgress(stubSnapshot, { mutate });
    render(wrapInProgress(<GenerateInProgress classId={CLASS_ID} />));
    fireEvent.click(screen.getByRole("button", { name: /^stop$/i }));
    expect(mutate).toHaveBeenCalledTimes(1);
  });

  it("shows the Stopping label and disables the button once cancel_requested is true", () => {
    mockProgress({ ...stubSnapshot, cancel_requested: true });
    render(wrapInProgress(<GenerateInProgress classId={CLASS_ID} />));
    const button = screen.getByRole("button", { name: /stopping/i });
    expect(button).toBeDisabled();
  });

  it("falls back to zero counts when no snapshot is available", () => {
    mockProgress(null);
    render(wrapInProgress(<GenerateInProgress classId={CLASS_ID} />));
    expect(screen.getByText(/0 \/ 0/)).toBeInTheDocument();
  });
});
