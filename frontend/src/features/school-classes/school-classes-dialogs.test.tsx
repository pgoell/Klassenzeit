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

const TEACHER_MATH_ID = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbb1";
const TEACHER_ART_ID = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbb2";
const TEACHER_NONE_ID = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbb3";
const SUBJECT_MATH_ID = "11111111-1111-1111-1111-111111111111";
const SUBJECT_ART_ID = "11111111-1111-1111-1111-111111111122";
const STUNDENTAFEL_ID = "99999999-9999-9999-9999-999999999999";
const WEEK_SCHEME_ID = "cccccccc-cccc-cccc-cccc-cccccccccccc";
const CLASS_ID = "88888888-8888-8888-8888-888888888888";

function seedClassTeacherTestHandlers(opts: { entries: "math" | "empty" }) {
  const teachers = [
    {
      id: TEACHER_MATH_ID,
      first_name: "Maria",
      last_name: "Mathlehrerin",
      short_code: "MAT",
      max_hours_per_week: 25,
      is_active: true,
      subject_ids: [SUBJECT_MATH_ID],
      created_at: "2026-04-17T00:00:00Z",
      updated_at: "2026-04-17T00:00:00Z",
    },
    {
      id: TEACHER_ART_ID,
      first_name: "Arno",
      last_name: "Artlehrer",
      short_code: "ART",
      max_hours_per_week: 25,
      is_active: true,
      subject_ids: [SUBJECT_ART_ID],
      created_at: "2026-04-17T00:00:00Z",
      updated_at: "2026-04-17T00:00:00Z",
    },
    {
      id: TEACHER_NONE_ID,
      first_name: "Nele",
      last_name: "Nichts",
      short_code: "NEL",
      max_hours_per_week: 25,
      is_active: true,
      subject_ids: [],
      created_at: "2026-04-17T00:00:00Z",
      updated_at: "2026-04-17T00:00:00Z",
    },
  ];
  const entries =
    opts.entries === "math"
      ? [
          {
            id: "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeee1",
            subject: { id: SUBJECT_MATH_ID, name: "Mathematik", short_name: "MA" },
            hours_per_week: 4,
            preferred_block_size: 1,
          },
        ]
      : [];
  server.use(
    http.get("http://localhost:3000/api/teachers", () => HttpResponse.json(teachers)),
    http.get("http://localhost:3000/api/stundentafeln/:tafel_id", () =>
      HttpResponse.json({
        id: STUNDENTAFEL_ID,
        name: "Grundschule Klasse 1",
        grade_level: 1,
        school_type: "Grundschule",
        entries,
        created_at: "2026-04-17T00:00:00Z",
        updated_at: "2026-04-17T00:00:00Z",
      }),
    ),
  );
}

function makeClassFixture(overrides: { class_teacher_id?: string | null } = {}) {
  return {
    id: CLASS_ID,
    name: "1a",
    grade_level: 1,
    stundentafel_id: STUNDENTAFEL_ID,
    week_scheme_id: WEEK_SCHEME_ID,
    home_room_id: null,
    class_teacher_id: overrides.class_teacher_id ?? null,
    max_lessons_per_day: null,
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  };
}

describe("SchoolClassFormDialog Klassenlehrer picker", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  test("shows class teacher picker filtered to qualified teachers in the curriculum", async () => {
    seedClassTeacherTestHandlers({ entries: "math" });

    render(
      wrapSchoolClassDialog(
        <SchoolClassFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          schoolClass={makeClassFixture()}
        />,
      ),
    );

    const trigger = await screen.findByRole("combobox", { name: /class teacher/i });
    await userEvent.click(trigger);

    expect(await screen.findByRole("option", { name: /no class teacher/i })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: /maria mathlehrerin/i })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /arno artlehrer/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /nele nichts/i })).not.toBeInTheDocument();
  });

  test("includes currently-pinned unqualified teacher with marker", async () => {
    seedClassTeacherTestHandlers({ entries: "math" });

    render(
      wrapSchoolClassDialog(
        <SchoolClassFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          schoolClass={makeClassFixture({ class_teacher_id: TEACHER_ART_ID })}
        />,
      ),
    );

    const trigger = await screen.findByRole("combobox", { name: /class teacher/i });
    await userEvent.click(trigger);

    const artOption = await screen.findByRole("option", { name: /arno artlehrer/i });
    expect(artOption.textContent ?? "").toMatch(/not qualified/i);
  });

  test("selecting a teacher and saving sends class_teacher_id in PATCH body", async () => {
    seedClassTeacherTestHandlers({ entries: "math" });
    const patchBody = vi.fn();
    server.use(
      http.patch("http://localhost:3000/api/classes/:class_id", async ({ request }) => {
        const body = await request.json();
        patchBody(body);
        return HttpResponse.json({
          ...makeClassFixture({ class_teacher_id: TEACHER_MATH_ID }),
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
          schoolClass={makeClassFixture()}
        />,
      ),
    );

    const trigger = await screen.findByRole("combobox", { name: /class teacher/i });
    await userEvent.click(trigger);
    await userEvent.click(await screen.findByRole("option", { name: /maria mathlehrerin/i }));

    await userEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() => {
      expect(patchBody).toHaveBeenCalledOnce();
    });
    const firstCall = patchBody.mock.calls[0];
    if (!firstCall) throw new Error("patchBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body).toMatchObject({ class_teacher_id: TEACHER_MATH_ID });
  });

  test("selecting 'No class teacher' clears class_teacher_id to null on save", async () => {
    seedClassTeacherTestHandlers({ entries: "math" });
    const patchBody = vi.fn();
    server.use(
      http.patch("http://localhost:3000/api/classes/:class_id", async ({ request }) => {
        const body = await request.json();
        patchBody(body);
        return HttpResponse.json({
          ...makeClassFixture(),
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
          schoolClass={makeClassFixture({ class_teacher_id: TEACHER_MATH_ID })}
        />,
      ),
    );

    const trigger = await screen.findByRole("combobox", { name: /class teacher/i });
    await userEvent.click(trigger);
    await userEvent.click(await screen.findByRole("option", { name: /no class teacher/i }));

    await userEvent.click(screen.getByRole("button", { name: /save/i }));

    await waitFor(() => {
      expect(patchBody).toHaveBeenCalledOnce();
    });
    const firstCall = patchBody.mock.calls[0];
    if (!firstCall) throw new Error("patchBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body).toHaveProperty("class_teacher_id", null);
  });

  test("shows all teachers when the class's Stundentafel has zero entries", async () => {
    seedClassTeacherTestHandlers({ entries: "empty" });

    render(
      wrapSchoolClassDialog(
        <SchoolClassFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          schoolClass={makeClassFixture()}
        />,
      ),
    );

    const trigger = await screen.findByRole("combobox", { name: /class teacher/i });
    await userEvent.click(trigger);

    expect(await screen.findByRole("option", { name: /maria mathlehrerin/i })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: /arno artlehrer/i })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: /nele nichts/i })).toBeInTheDocument();
  });
});

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
