import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import type { ReactNode } from "react";
import { beforeAll, describe, expect, test, vi } from "vitest";
import i18n from "@/i18n/init";
import { server } from "../../../tests/msw-handlers";
import { SchoolClassFormDialog } from "./school-classes-dialogs";

function wrapSchoolClassDialog(children: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("SchoolClassFormDialog max_lessons_per_day", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  test("round-trips an explicit max_lessons_per_day on create", async () => {
    const createBody = vi.fn();
    server.use(
      http.post("http://localhost:3000/api/classes", async ({ request }) => {
        const body = await request.json();
        createBody(body);
        return HttpResponse.json(
          {
            id: "77777777-7777-7777-7777-777777777777",
            name: "1b",
            grade_level: 1,
            stundentafel_id: "99999999-9999-9999-9999-999999999999",
            week_scheme_id: "cccccccc-cccc-cccc-cccc-cccccccccccc",
            home_room_id: null,
            max_lessons_per_day: 5,
            created_at: "2026-05-01T00:00:00Z",
            updated_at: "2026-05-01T00:00:00Z",
          },
          { status: 201 },
        );
      }),
    );

    render(
      wrapSchoolClassDialog(
        <SchoolClassFormDialog open onOpenChange={() => {}} submitLabel="Create" />,
      ),
    );

    await userEvent.type(screen.getByLabelText(/^name$/i), "1b");
    const stundentafelTrigger = await screen.findByRole("combobox", { name: /curriculum/i });
    await userEvent.click(stundentafelTrigger);
    await userEvent.click(await screen.findByRole("option", { name: /grundschule klasse 1/i }));
    const weekSchemeTrigger = await screen.findByRole("combobox", { name: /week scheme/i });
    await userEvent.click(weekSchemeTrigger);
    await userEvent.click(await screen.findByRole("option", { name: /standardwoche/i }));

    const maxLessonsInput = screen.getByLabelText(/lessons per day \(max\)/i) as HTMLInputElement;
    expect(maxLessonsInput.value).toBe("");
    await userEvent.type(maxLessonsInput, "5");

    await userEvent.click(screen.getByRole("button", { name: /create/i }));

    await waitFor(() => {
      expect(createBody).toHaveBeenCalledOnce();
    });
    const firstCall = createBody.mock.calls[0];
    if (!firstCall) throw new Error("createBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body).toMatchObject({ max_lessons_per_day: 5 });
  });

  test("clearing an existing max_lessons_per_day sends null on PATCH", async () => {
    const patchBody = vi.fn();
    server.use(
      http.patch("http://localhost:3000/api/classes/:class_id", async ({ request }) => {
        const body = await request.json();
        patchBody(body);
        return HttpResponse.json({
          id: "88888888-8888-8888-8888-888888888888",
          name: "1a",
          grade_level: 1,
          stundentafel_id: "99999999-9999-9999-9999-999999999999",
          week_scheme_id: "cccccccc-cccc-cccc-cccc-cccccccccccc",
          home_room_id: null,
          max_lessons_per_day: null,
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-05-01T00:00:00Z",
        });
      }),
    );

    render(
      wrapSchoolClassDialog(
        <SchoolClassFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          schoolClass={{
            id: "88888888-8888-8888-8888-888888888888",
            name: "1a",
            grade_level: 1,
            stundentafel_id: "99999999-9999-9999-9999-999999999999",
            week_scheme_id: "cccccccc-cccc-cccc-cccc-cccccccccccc",
            home_room_id: null,
            max_lessons_per_day: 6,
            created_at: "2026-04-17T00:00:00Z",
            updated_at: "2026-04-17T00:00:00Z",
          }}
        />,
      ),
    );

    const maxLessonsInput = (await screen.findByLabelText(
      /lessons per day \(max\)/i,
    )) as HTMLInputElement;
    expect(maxLessonsInput.value).toBe("6");
    await userEvent.clear(maxLessonsInput);

    await userEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() => {
      expect(patchBody).toHaveBeenCalledOnce();
    });
    const firstCall = patchBody.mock.calls[0];
    if (!firstCall) throw new Error("patchBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body).toHaveProperty("max_lessons_per_day", null);
  });
});
