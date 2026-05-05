import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import type { ReactNode } from "react";
import { beforeAll, describe, expect, test, vi } from "vitest";
import i18n from "@/i18n/init";
import { server } from "../../../tests/msw-handlers";
import { SubjectFormDialog } from "./subjects-dialogs";

function wrapSubjectDialog(children: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("SubjectFormDialog weight inputs", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  test("submits with non-binary prefer_early_period weight", async () => {
    const requestBody = vi.fn();
    server.use(
      http.post("http://localhost:3000/api/subjects", async ({ request }) => {
        const body = await request.json();
        requestBody(body);
        return HttpResponse.json(
          {
            id: "22222222-2222-2222-2222-222222222222",
            name: "Mathematik",
            short_name: "MA",
            color: "chart-1",
            prefer_early_period: 3,
            avoid_first_period: 0,
            avoid_last_period: 0,
            max_hours_per_day: 2,
            created_at: "2026-05-01T00:00:00Z",
            updated_at: "2026-05-01T00:00:00Z",
          },
          { status: 201 },
        );
      }),
    );

    render(
      wrapSubjectDialog(<SubjectFormDialog open onOpenChange={() => {}} submitLabel="Create" />),
    );
    await userEvent.type(screen.getByLabelText(/^name$/i), "Mathematik");
    await userEvent.type(screen.getByLabelText(/short name/i), "MA");
    const earlyInput = screen.getByLabelText(/prefer early periods/i);
    await userEvent.clear(earlyInput);
    await userEvent.type(earlyInput, "3");
    await userEvent.click(screen.getByRole("button", { name: /create/i }));

    await waitFor(() => {
      expect(requestBody).toHaveBeenCalledOnce();
    });
    const firstCall = requestBody.mock.calls[0];
    if (!firstCall) throw new Error("requestBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body).toMatchObject({ prefer_early_period: 3 });
  });

  test("renders max_hours_per_day with default 2 and submits the chosen value", async () => {
    const subjectMaxHoursRequestBody = vi.fn();
    server.use(
      http.post("http://localhost:3000/api/subjects", async ({ request }) => {
        const body = await request.json();
        subjectMaxHoursRequestBody(body);
        return HttpResponse.json(
          {
            id: "22222222-2222-2222-2222-222222222222",
            name: "Mathematik",
            short_name: "MA",
            color: "chart-1",
            prefer_early_period: 0,
            prefer_late_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            max_hours_per_day: 3,
            created_at: "2026-05-01T00:00:00Z",
            updated_at: "2026-05-01T00:00:00Z",
          },
          { status: 201 },
        );
      }),
    );

    render(
      wrapSubjectDialog(<SubjectFormDialog open onOpenChange={() => {}} submitLabel="Create" />),
    );
    const maxHoursInput = screen.getByLabelText(/hours per day \(max\)/i) as HTMLInputElement;
    expect(maxHoursInput.value).toBe("2");

    await userEvent.type(screen.getByLabelText(/^name$/i), "Mathematik");
    await userEvent.type(screen.getByLabelText(/short name/i), "MA");
    await userEvent.clear(maxHoursInput);
    await userEvent.type(maxHoursInput, "3");
    await userEvent.click(screen.getByRole("button", { name: /create/i }));

    await waitFor(() => {
      expect(subjectMaxHoursRequestBody).toHaveBeenCalledOnce();
    });
    const firstCall = subjectMaxHoursRequestBody.mock.calls[0];
    if (!firstCall) throw new Error("subjectMaxHoursRequestBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body).toMatchObject({ max_hours_per_day: 3 });
  });
});
