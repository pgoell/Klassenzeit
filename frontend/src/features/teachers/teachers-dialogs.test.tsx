import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import type { ReactNode } from "react";
import { beforeAll, describe, expect, test } from "vitest";
import i18n from "@/i18n/init";
import { server } from "../../../tests/msw-handlers";
import { TeacherFormDialog } from "./teachers-dialogs";

function wrapTeacherDialog(children: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

const TEACHER_ID = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbb1";
const OTHER_TEACHER_ID = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbb2";
const STUNDENTAFEL_ID = "99999999-9999-9999-9999-999999999999";
const WEEK_SCHEME_ID = "cccccccc-cccc-cccc-cccc-cccccccccccc";

const teacherFixture = {
  id: TEACHER_ID,
  first_name: "Maria",
  last_name: "Müller",
  short_code: "MUE",
  max_hours_per_week: 25,
  is_active: true,
  subject_ids: [],
  created_at: "2026-04-17T00:00:00Z",
  updated_at: "2026-04-17T00:00:00Z",
};

function makeSchoolClass(overrides: {
  id: string;
  name: string;
  class_teacher_id?: string | null;
}) {
  return {
    id: overrides.id,
    name: overrides.name,
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

function stubTeacherSubresources(teacherId: string) {
  // The dialog mounts TeacherQualificationsEditor and TeacherAvailabilityGrid
  // when in edit mode; both load their own data. Stub the underlying detail
  // endpoint with empty arrays so unhandled-request errors don't fire.
  server.use(
    http.get(`http://localhost:3000/api/teachers/${teacherId}`, () =>
      HttpResponse.json({
        ...teacherFixture,
        id: teacherId,
        qualifications: [],
        availability: [],
      }),
    ),
  );
}

describe("TeacherFormDialog Klassenlehrer-of badge", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  test("shows 'Class teacher of' badge in edit mode when teacher is Klassenlehrer of >= 1 class", async () => {
    stubTeacherSubresources(TEACHER_ID);
    server.use(
      http.get("http://localhost:3000/api/classes", () =>
        HttpResponse.json([
          makeSchoolClass({
            id: "88888888-8888-8888-8888-888888888881",
            name: "1a",
            class_teacher_id: TEACHER_ID,
          }),
          makeSchoolClass({
            id: "88888888-8888-8888-8888-888888888882",
            name: "2b",
            class_teacher_id: TEACHER_ID,
          }),
        ]),
      ),
    );

    render(
      wrapTeacherDialog(
        <TeacherFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          teacher={teacherFixture}
        />,
      ),
    );

    await waitFor(() => {
      expect(screen.getByText(/class teacher of/i)).toBeInTheDocument();
    });
    expect(screen.getByText("1a, 2b")).toBeInTheDocument();
  });

  test("hides 'Class teacher of' badge when no class points at this teacher", async () => {
    stubTeacherSubresources(TEACHER_ID);
    server.use(
      http.get("http://localhost:3000/api/classes", () =>
        HttpResponse.json([
          makeSchoolClass({
            id: "88888888-8888-8888-8888-888888888881",
            name: "1a",
            class_teacher_id: OTHER_TEACHER_ID,
          }),
        ]),
      ),
    );

    render(
      wrapTeacherDialog(
        <TeacherFormDialog
          open
          onOpenChange={() => {}}
          submitLabel="Save"
          teacher={teacherFixture}
        />,
      ),
    );

    // Wait for the dialog to render, then assert the badge is absent.
    await screen.findByRole("dialog");
    await waitFor(() => {
      expect(screen.queryByText(/class teacher of/i)).not.toBeInTheDocument();
    });
  });

  test("hides 'Class teacher of' badge in create mode", async () => {
    server.use(
      http.get("http://localhost:3000/api/classes", () =>
        HttpResponse.json([
          makeSchoolClass({
            id: "88888888-8888-8888-8888-888888888881",
            name: "1a",
            class_teacher_id: TEACHER_ID,
          }),
        ]),
      ),
    );

    render(
      wrapTeacherDialog(<TeacherFormDialog open onOpenChange={() => {}} submitLabel="Create" />),
    );

    await screen.findByRole("dialog");
    expect(screen.queryByText(/class teacher of/i)).not.toBeInTheDocument();
  });
});
