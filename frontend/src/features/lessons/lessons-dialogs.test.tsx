import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import type { ReactNode } from "react";
import { afterAll, beforeAll, beforeEach, describe, expect, test } from "vitest";
import type { Lesson } from "@/features/lessons/hooks";
import i18n from "@/i18n/init";
import { initialSchoolClasses, server } from "../../../tests/msw-handlers";
import { LessonFormDialog } from "./lessons-dialogs";

const SECOND_CLASS_ID = "88888888-8888-8888-8888-888888888889";
const BASE = "http://localhost:3000";

function wrapLessonDialog(children: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

function seedTwoClasses() {
  server.use(
    http.get(`${BASE}/api/classes`, () =>
      HttpResponse.json([
        ...initialSchoolClasses,
        {
          id: SECOND_CLASS_ID,
          name: "1b",
          grade_level: 1,
          stundentafel_id: "99999999-9999-9999-9999-999999999999",
          week_scheme_id: "cccccccc-cccc-cccc-cccc-cccccccccccc",
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        },
      ]),
    ),
  );
}

describe("LessonFormDialog multi-class selection", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  beforeEach(() => {
    seedTwoClasses();
  });

  afterAll(() => {
    server.resetHandlers();
  });

  test("renders a checkbox per available school class", async () => {
    render(
      wrapLessonDialog(<LessonFormDialog open onOpenChange={() => {}} submitLabel="Create" />),
    );
    await screen.findByRole("checkbox", { name: /^1a$/i });
    await screen.findByRole("checkbox", { name: /^1b$/i });
  });

  test("rejects submit when no class selected", async () => {
    const user = userEvent.setup();
    render(
      wrapLessonDialog(<LessonFormDialog open onOpenChange={() => {}} submitLabel="Create" />),
    );
    // Wait for class checkboxes to mount before submitting.
    await screen.findByRole("checkbox", { name: /^1a$/i });
    await user.click(screen.getByRole("button", { name: /^create$/i }));
    expect(await screen.findByText(/at least one class/i)).toBeVisible();
  });

  test("seeds checkbox state from lesson.school_classes", async () => {
    const lesson: Lesson = {
      id: "55555555-5555-5555-5555-555555555555",
      school_classes: [{ id: "88888888-8888-8888-8888-888888888888", name: "1a" }],
      subject: {
        id: "11111111-1111-1111-1111-111111111111",
        name: "Mathematik",
        short_name: "MA",
      },
      teacher: null,
      hours_per_week: 4,
      preferred_block_size: 1,
      lesson_group_id: null,
      created_at: "2026-04-20T00:00:00Z",
      updated_at: "2026-04-20T00:00:00Z",
    };
    render(
      wrapLessonDialog(
        <LessonFormDialog open onOpenChange={() => {}} submitLabel="Save" lesson={lesson} />,
      ),
    );
    const cb = await screen.findByRole("checkbox", { name: /^1a$/i });
    expect(cb).toBeChecked();
    const cbB = await screen.findByRole("checkbox", { name: /^1b$/i });
    expect(cbB).not.toBeChecked();
  });
});

const SUBJECT_MA_ID = "11111111-1111-1111-1111-111111111111";
const SUBJECT_DE_ID = "22222222-2222-2222-2222-222222222222";
const TEACHER_QUALIFIED_ID = "ccccdddd-aaaa-bbbb-cccc-ddddddddccdd";
const TEACHER_OTHER_ID = "ddddeeee-aaaa-bbbb-cccc-eeeeeeeeddee";
const LESSON_ID = "55555555-5555-5555-5555-555555555555";

function makeLesson(opts: { teacherId: string | null; subjectId?: string }): Lesson {
  const teacher =
    opts.teacherId === null
      ? null
      : { id: opts.teacherId, first_name: "Anna", last_name: "Schmidt", short_code: "SCH" };
  return {
    id: LESSON_ID,
    school_classes: [{ id: "88888888-8888-8888-8888-888888888888", name: "1a" }],
    subject: {
      id: opts.subjectId ?? SUBJECT_MA_ID,
      name: "Mathematik",
      short_name: "MA",
    },
    teacher,
    hours_per_week: 4,
    preferred_block_size: 1,
    lesson_group_id: null,
    created_at: "2026-04-20T00:00:00Z",
    updated_at: "2026-04-20T00:00:00Z",
  };
}

function seedTeachersWithQualifications() {
  server.use(
    http.get(`${BASE}/api/teachers`, () =>
      HttpResponse.json([
        {
          id: TEACHER_QUALIFIED_ID,
          first_name: "Anna",
          last_name: "Schmidt",
          short_code: "SCH",
          max_hours_per_week: 25,
          is_active: true,
          subject_ids: [SUBJECT_MA_ID],
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        },
        {
          id: TEACHER_OTHER_ID,
          first_name: "Bruno",
          last_name: "Weber",
          short_code: "WEB",
          max_hours_per_week: 25,
          is_active: true,
          subject_ids: [SUBJECT_DE_ID],
          created_at: "2026-04-17T00:00:00Z",
          updated_at: "2026-04-17T00:00:00Z",
        },
      ]),
    ),
  );
}

describe("LessonFormDialog teacher pin switch", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  beforeEach(() => {
    seedTwoClasses();
  });

  afterAll(() => {
    server.resetHandlers();
  });

  test("editing an unpinned lesson opens with switch off and hint", async () => {
    seedTeachersWithQualifications();
    render(
      wrapLessonDialog(
        <LessonFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          lesson={makeLesson({ teacherId: null })}
        />,
      ),
    );
    const sw = await screen.findByRole("switch", { name: /pin to specific teacher/i });
    expect(sw).not.toBeChecked();
    expect(await screen.findByText(/solver will pick/i)).toBeVisible();
    expect(screen.queryByRole("combobox", { name: /^teacher$/i })).toBeNull();
  });

  test("editing a pinned lesson opens with switch on and dropdown", async () => {
    seedTeachersWithQualifications();
    render(
      wrapLessonDialog(
        <LessonFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          lesson={makeLesson({ teacherId: TEACHER_QUALIFIED_ID })}
        />,
      ),
    );
    const sw = await screen.findByRole("switch", { name: /pin to specific teacher/i });
    expect(sw).toBeChecked();
    expect(await screen.findByRole("combobox", { name: /^teacher$/i })).toBeVisible();
  });

  test("toggling switch off sends teacher_id null on submit", async () => {
    seedTeachersWithQualifications();
    const captured: Array<Record<string, unknown>> = [];
    server.use(
      http.patch(`${BASE}/api/lessons/${LESSON_ID}`, async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        captured.push(body);
        return HttpResponse.json({
          ...makeLesson({ teacherId: null }),
          teacher: null,
        });
      }),
    );
    const user = userEvent.setup();
    render(
      wrapLessonDialog(
        <LessonFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          lesson={makeLesson({ teacherId: TEACHER_QUALIFIED_ID })}
        />,
      ),
    );
    const sw = await screen.findByRole("switch", { name: /pin to specific teacher/i });
    await user.click(sw);
    await user.click(screen.getByRole("button", { name: /^save$/i }));
    await waitFor(() => expect(captured.length).toBe(1));
    expect(captured[0]?.teacher_id).toBeNull();
  });

  test("toggling on after off restores prior teacher_id", async () => {
    seedTeachersWithQualifications();
    const user = userEvent.setup();
    render(
      wrapLessonDialog(
        <LessonFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          lesson={makeLesson({ teacherId: TEACHER_QUALIFIED_ID })}
        />,
      ),
    );
    const sw = await screen.findByRole("switch", { name: /pin to specific teacher/i });
    await user.click(sw);
    expect(sw).not.toBeChecked();
    await user.click(sw);
    expect(sw).toBeChecked();
    const trigger = await screen.findByRole("combobox", { name: /^teacher$/i });
    expect(trigger).toHaveTextContent(/Schmidt/);
  });

  test("dropdown filters to subject-qualified teachers", async () => {
    seedTeachersWithQualifications();
    const user = userEvent.setup();
    render(
      wrapLessonDialog(
        <LessonFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          lesson={makeLesson({ teacherId: TEACHER_QUALIFIED_ID, subjectId: SUBJECT_MA_ID })}
        />,
      ),
    );
    await screen.findByRole("switch", { name: /pin to specific teacher/i });
    const trigger = screen.getByRole("combobox", { name: /^teacher$/i });
    await user.click(trigger);
    expect(await screen.findByRole("option", { name: /Schmidt/ })).toBeVisible();
    expect(screen.queryByRole("option", { name: /Weber/ })).toBeNull();
  });

  test("renders pinned-but-no-longer-qualified teacher with suffix", async () => {
    // The pinned teacher (Schmidt, qualified for MA) is rendered for a Lesson whose subject is DE,
    // for which Schmidt is NOT qualified. Schmidt must still appear with the suffix.
    seedTeachersWithQualifications();
    render(
      wrapLessonDialog(
        <LessonFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          lesson={makeLesson({ teacherId: TEACHER_QUALIFIED_ID, subjectId: SUBJECT_DE_ID })}
        />,
      ),
    );
    await screen.findByRole("switch", { name: /pin to specific teacher/i });
    const trigger = screen.getByRole("combobox", { name: /^teacher$/i });
    await waitFor(() => expect(trigger).toHaveTextContent(/Schmidt/));
    expect(trigger).toHaveTextContent(/no longer qualified/i);
  });
});
