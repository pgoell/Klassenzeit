import { HttpResponse, http } from "msw";
import { setupServer } from "msw/node";
import type { components } from "@/lib/api-types";

// jsdom's default window.location.origin is http://localhost:3000, which is
// what the api-client resolves its baseUrl against during tests.
export const BASE = "http://localhost:3000";

export const adminMe = {
  id: "00000000-0000-0000-0000-000000000001",
  email: "admin@example.com",
  role: "admin",
  force_password_change: false,
  school_id: "00000000-0000-0000-0000-000000000001",
  school_name: "Default Schule",
  active_school_id: "00000000-0000-0000-0000-000000000001",
  active_school_name: "Default Schule",
  accessible_schools: [{ id: "00000000-0000-0000-0000-000000000001", name: "Default Schule" }],
  created_at: "2026-04-17T00:00:00Z",
};

export const superAdminMe = {
  id: "00000000-0000-0000-0000-000000000002",
  email: "superadmin@example.com",
  role: "super_admin",
  force_password_change: false,
  school_id: "00000000-0000-0000-0000-000000000001",
  school_name: "Default Schule",
  active_school_id: "00000000-0000-0000-0000-000000000001",
  active_school_name: "Default Schule",
  accessible_schools: [
    { id: "00000000-0000-0000-0000-000000000001", name: "Default Schule" },
    { id: "ffffffff-ffff-ffff-ffff-ffffffffffff", name: "Zweite Grundschule" },
  ],
  created_at: "2026-04-17T00:00:00Z",
};

export const initialSubjects = [
  {
    id: "11111111-1111-1111-1111-111111111111",
    name: "Mathematik",
    short_name: "MA",
    color: "chart-3",
    prefer_early_period: 0,
    prefer_late_period: 0,
    avoid_first_period: 0,
    avoid_last_period: 0,
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  },
];

export const initialSchools = [
  {
    id: "00000000-0000-0000-0000-000000000001",
    name: "Default Schule",
    short_name: "DS",
  },
  {
    id: "ffffffff-ffff-ffff-ffff-ffffffffffff",
    name: "Zweite Grundschule",
    short_name: "ZWG",
  },
];

export const initialRooms = [
  {
    id: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
    name: "Raum 101",
    short_name: "101",
    capacity: 30,
    is_external: false,
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  },
  {
    id: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaab",
    name: "Klasse 1a",
    short_name: "1a",
    capacity: 25,
    is_external: false,
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  },
];

export const initialTeachers = [
  {
    id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
    first_name: "Anna",
    last_name: "Schmidt",
    short_code: "SCH",
    max_hours_per_week: 25,
    reserve_hours_per_week: 0,
    is_active: true,
    subject_ids: [],
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  },
];

export const initialWeekSchemes = [
  {
    id: "cccccccc-cccc-cccc-cccc-cccccccccccc",
    name: "Standardwoche",
    description: "Mo-Fr, 8 Blöcke",
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  },
];

export type TimeBlock = {
  id: string;
  day_of_week: number;
  position: number;
  start_time: string;
  end_time: string;
};

// Mutable per-test store so POST/PATCH/DELETE on time blocks see a consistent
// view. Tests reset the buckets in `beforeEach`; seed values here are only used
// when a test does not override them.
export const timeBlocksBySchemeId: Record<string, TimeBlock[]> = {
  "cccccccc-cccc-cccc-cccc-cccccccccccc": [],
};

export const initialStundentafeln = [
  {
    id: "99999999-9999-9999-9999-999999999999",
    name: "Grundschule Klasse 1",
    grade_level: 1,
    school_type: "Grundschule" as const,
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  },
];

// Mutable per-test store so POST/PATCH/DELETE on entries see a consistent view.
// Tests never share state across describe blocks; MSW handlers reset if you
// mutate this between `beforeEach` runs.
export const stundentafelEntriesByTafelId: Record<
  string,
  Array<{
    id: string;
    subject: { id: string; name: string; short_name: string };
    hours_per_week: number;
    preferred_block_size: number;
  }>
> = {
  "99999999-9999-9999-9999-999999999999": [],
};

export const roomSuitabilityByRoomId: Record<string, string[]> = {
  "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa": [],
};

export const roomAvailabilityByRoomId: Record<string, string[]> = {
  "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa": [],
};

export const teacherQualsByTeacherId: Record<string, string[]> = {
  "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb": [],
};
export const teacherAvailabilityByTeacherId: Record<
  string,
  Array<{ time_block_id: string; status: string }>
> = {
  "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb": [],
};

export const initialAdminUsers: components["schemas"]["UserListItem"][] = [
  {
    id: "00000000-0000-0000-0000-000000000001",
    email: "admin@example.com",
    role: "admin",
    is_active: true,
    last_login_at: null,
    school_id: "00000000-0000-0000-0000-000000000001",
    school_name: "Default Schule",
  },
  {
    id: "00000000-0000-0000-0000-000000000002",
    email: "superadmin@example.com",
    role: "super_admin",
    is_active: true,
    last_login_at: null,
    school_id: "00000000-0000-0000-0000-000000000001",
    school_name: "Default Schule",
  },
];

// Mutable per-test bag for the admin-users handler. Reset in `beforeEach`
// via `resetAdminUsers()`.
export const adminUsers: components["schemas"]["UserListItem"][] = [...initialAdminUsers];
export function resetAdminUsers() {
  adminUsers.splice(0, adminUsers.length, ...initialAdminUsers);
}

// Mutable per-test bag for the audit-log handler. Reset in `beforeEach`
// via `resetAuditLogRows()`.
export const auditLogRows: components["schemas"]["AuditLogEntryItem"][] = [];
export function resetAuditLogRows() {
  auditLogRows.length = 0;
}

export const initialSchoolClasses = [
  {
    id: "88888888-8888-8888-8888-888888888888",
    name: "1a",
    grade_level: 1,
    stundentafel_id: "99999999-9999-9999-9999-999999999999",
    week_scheme_id: "cccccccc-cccc-cccc-cccc-cccccccccccc",
    home_room_id: null,
    created_at: "2026-04-17T00:00:00Z",
    updated_at: "2026-04-17T00:00:00Z",
  },
];

// Mutable per-test store for schedule placements and violations. Tests
// assign the arrays they want the GET / POST handlers to return, and reset
// them in `beforeEach` by iterating `Object.keys`.
export const scheduleByClassId: Record<string, components["schemas"]["PlacementResponse"][]> = {};
export const violationsByClassId: Record<string, components["schemas"]["ViolationResponse"][]> = {};
export const teacherSchedulesByTeacherId: Record<
  string,
  components["schemas"]["PlacementResponse"][]
> = {};
export const roomSchedulesByRoomId: Record<string, components["schemas"]["PlacementResponse"][]> =
  {};

// Zero-value QualityReport fixture used as the default `quality_report` on the
// class POST handler when a test has not seeded a per-class override.
const EMPTY_QUALITY_REPORT: components["schemas"]["QualityReportResponse"] = {
  hard_violations: 0,
  unplaced_hours: 0,
  class_gap_hours: 0,
  class_gap_hours_by_class: {},
  teacher_gap_hours: 0,
  teacher_gap_hours_by_teacher: {},
  class_day_balance_cost: 0,
  class_day_balance_cost_by_class: {},
  home_room_misses: 0,
  home_room_misses_by_class: {},
  prefer_early_units: 0,
  avoid_first_units: 0,
  avoid_last_units: 0,
  prefer_late_units: 0,
  prefer_class_teacher_misses: 0,
  weighted_score: 0,
  worst_per_class_spread: 0,
  worst_per_class_interior_gaps: 0,
  soft_pin_misses: 0,
  supervision_spread_raw: 0,
};

// Mutable per-test override so individual test cases can seed populated
// attribution. Keys: classId. Reset to `{}` in `beforeEach` (tests/setup.ts).
export const qualityReportByClassId: Record<
  string,
  components["schemas"]["QualityReportResponse"]
> = {};

// Mutable per-test override so individual test cases can seed populated
// per-teacher attribution. Keys: teacherId. Reset to `{}` in `beforeEach`
// (tests/setup.ts).
export const qualityReportByTeacherId: Record<
  string,
  components["schemas"]["QualityReportResponse"]
> = {};

export const initialLessons = [
  {
    id: "55555555-5555-5555-5555-555555555555",
    school_classes: [
      {
        id: "88888888-8888-8888-8888-888888888888",
        name: "1a",
      },
    ],
    subject: {
      id: "11111111-1111-1111-1111-111111111111",
      name: "Mathematik",
      short_name: "MA",
    },
    teacher: {
      id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
      first_name: "Anna",
      last_name: "Schmidt",
      short_code: "SCH",
    },
    hours_per_week: 4,
    preferred_block_size: 1,
    pre_buffer_minutes: 0,
    post_buffer_minutes: 0,
    lesson_group_id: null,
    created_at: "2026-04-20T00:00:00Z",
    updated_at: "2026-04-20T00:00:00Z",
  },
];

export const defaultHandlers = [
  http.get(`${BASE}/api/auth/me`, () => HttpResponse.json(adminMe)),
  http.post(`${BASE}/api/auth/login`, async () => HttpResponse.json(null, { status: 204 })),
  http.post(`${BASE}/api/auth/logout`, () => HttpResponse.json(null, { status: 204 })),
  http.post(`${BASE}/api/auth/switch-school`, async ({ request }) => {
    const body = (await request.json()) as { school_id: string };
    const target = adminMe.accessible_schools.find((s) => s.id === body.school_id);
    return HttpResponse.json({
      ...adminMe,
      active_school_id: body.school_id,
      active_school_name: target?.name ?? "Switched School",
    });
  }),
  http.get(`${BASE}/api/auth/admin/users`, () => HttpResponse.json(adminUsers)),
  http.get(`${BASE}/api/auth/admin/audit-log`, () =>
    HttpResponse.json({ items: auditLogRows, total: auditLogRows.length }),
  ),
  http.get(`${BASE}/api/subjects`, () => HttpResponse.json(initialSubjects)),
  http.post(`${BASE}/api/subjects`, async ({ request }) => {
    const body = (await request.json()) as {
      name: string;
      short_name: string;
      color: string;
      prefer_early_period?: number;
      prefer_late_period?: number;
      avoid_first_period?: number;
      avoid_last_period?: number;
    };
    return HttpResponse.json(
      {
        id: "22222222-2222-2222-2222-222222222222",
        name: body.name,
        short_name: body.short_name,
        color: body.color,
        prefer_early_period: body.prefer_early_period ?? 0,
        prefer_late_period: body.prefer_late_period ?? 0,
        avoid_first_period: body.avoid_first_period ?? 0,
        avoid_last_period: body.avoid_last_period ?? 0,
        created_at: "2026-04-17T00:00:00Z",
        updated_at: "2026-04-17T00:00:00Z",
      },
      { status: 201 },
    );
  }),
  http.get(`${BASE}/api/schools`, () => HttpResponse.json(initialSchools)),
  http.post(`${BASE}/api/schools`, async ({ request }) => {
    const body = (await request.json()) as { name: string; short_name?: string | null };
    const created = {
      id: "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee",
      name: body.name,
      short_name: body.short_name ?? null,
      created_at: "2026-05-17T00:00:00Z",
      updated_at: "2026-05-17T00:00:00Z",
    };
    return HttpResponse.json(created, { status: 201 });
  }),
  http.patch(`${BASE}/api/schools/:school_id`, async ({ params, request }) => {
    const body = (await request.json()) as { name?: string; short_name?: string | null };
    const existing = initialSchools.find((s) => s.id === params.school_id);
    if (!existing) return HttpResponse.json({ detail: "not found" }, { status: 404 });
    return HttpResponse.json({
      id: existing.id,
      name: body.name ?? existing.name,
      short_name: body.short_name ?? existing.short_name,
      created_at: "2026-05-17T00:00:00Z",
      updated_at: "2026-05-17T00:00:00Z",
    });
  }),
  http.delete(`${BASE}/api/schools/:school_id`, () => new HttpResponse(null, { status: 204 })),
  http.get(`${BASE}/api/rooms`, () => HttpResponse.json(initialRooms)),
  http.post(`${BASE}/api/rooms`, async ({ request }) => {
    const body = (await request.json()) as {
      name: string;
      short_name: string;
      capacity: number | null;
      is_external?: boolean;
    };
    const id = "dddddddd-dddd-dddd-dddd-dddddddddddd";
    roomSuitabilityByRoomId[id] = [];
    return HttpResponse.json(
      {
        id,
        ...body,
        is_external: body.is_external ?? false,
        created_at: "2026-04-17T00:00:00Z",
        updated_at: "2026-04-17T00:00:00Z",
      },
      { status: 201 },
    );
  }),
  http.get(`${BASE}/api/rooms/:room_id`, ({ params }) => {
    const id = String(params.room_id);
    const base = initialRooms.find((r) => r.id === id);
    if (!base) {
      return HttpResponse.json({ detail: "not found" }, { status: 404 });
    }
    const selectedIds = roomSuitabilityByRoomId[id] ?? [];
    const suitability_subjects = selectedIds
      .map((sid) => initialSubjects.find((s) => s.id === sid))
      .filter((s): s is (typeof initialSubjects)[number] => s !== undefined)
      .map((s) => ({ id: s.id, name: s.name, short_name: s.short_name }));
    const allBlocks = Object.values(timeBlocksBySchemeId).flat();
    const availabilityIds = roomAvailabilityByRoomId[id] ?? [];
    const availability = availabilityIds.flatMap((tbId) => {
      const block = allBlocks.find((b) => b.id === tbId);
      return block
        ? [
            {
              time_block_id: tbId,
              day_of_week: block.day_of_week,
              position: block.position,
            },
          ]
        : [];
    });
    return HttpResponse.json({
      ...base,
      suitability_subjects,
      availability,
    });
  }),
  http.put(`${BASE}/api/rooms/:room_id/suitability`, async ({ request, params }) => {
    const body = (await request.json()) as { subject_ids: string[] };
    const id = String(params.room_id);
    const seen = new Set<string>();
    const unique = body.subject_ids.filter((sid) => {
      if (seen.has(sid)) return false;
      seen.add(sid);
      return true;
    });
    const missing = unique.filter((sid) => !initialSubjects.some((s) => s.id === sid));
    if (missing.length > 0) {
      return HttpResponse.json(
        { detail: { detail: "Some subjects do not exist.", missing_subject_ids: missing } },
        { status: 400 },
      );
    }
    roomSuitabilityByRoomId[id] = unique;
    const base = initialRooms.find((r) => r.id === id) ?? {
      id,
      name: "mutable",
      short_name: "X",
      capacity: null,
      is_external: false,
      created_at: "2026-04-17T00:00:00Z",
      updated_at: "2026-04-17T00:00:00Z",
    };
    const suitability_subjects = unique
      .map((sid) => initialSubjects.find((s) => s.id === sid))
      .filter((s): s is (typeof initialSubjects)[number] => s !== undefined)
      .map((s) => ({ id: s.id, name: s.name, short_name: s.short_name }));
    return HttpResponse.json({
      ...base,
      suitability_subjects,
      availability: [],
    });
  }),
  http.put(`${BASE}/api/rooms/:room_id/availability`, async ({ request, params }) => {
    const id = String(params.room_id);
    const body = (await request.json()) as { time_block_ids: string[] };
    roomAvailabilityByRoomId[id] = [...body.time_block_ids];
    const base = initialRooms.find((r) => r.id === id) ?? initialRooms[0];
    if (!base) return HttpResponse.json({ detail: "not found" }, { status: 404 });
    const allBlocks = Object.values(timeBlocksBySchemeId).flat();
    const availability = body.time_block_ids.flatMap((tbId) => {
      const block = allBlocks.find((b) => b.id === tbId);
      return block
        ? [
            {
              time_block_id: tbId,
              day_of_week: block.day_of_week,
              position: block.position,
            },
          ]
        : [];
    });
    return HttpResponse.json({
      ...base,
      suitability_subjects: [],
      availability,
    });
  }),
  http.get(`${BASE}/api/teachers`, () => HttpResponse.json(initialTeachers)),
  http.post(`${BASE}/api/teachers`, async ({ request }) => {
    const body = (await request.json()) as {
      first_name: string;
      last_name: string;
      short_code: string;
      max_hours_per_week: number;
      reserve_hours_per_week?: number;
    };
    return HttpResponse.json(
      {
        id: "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee",
        reserve_hours_per_week: 0,
        ...body,
        is_active: true,
        created_at: "2026-04-17T00:00:00Z",
        updated_at: "2026-04-17T00:00:00Z",
      },
      { status: 201 },
    );
  }),
  http.get(`${BASE}/api/teachers/:teacher_id`, ({ params }) => {
    const id = String(params.teacher_id);
    const base = initialTeachers.find((t) => t.id === id) ?? initialTeachers[0];
    if (!base) return HttpResponse.json({ detail: "not found" }, { status: 404 });
    const qualIds = teacherQualsByTeacherId[id] ?? [];
    const qualifications = qualIds
      .map((sid) => initialSubjects.find((s) => s.id === sid))
      .filter((s): s is (typeof initialSubjects)[number] => s !== undefined)
      .map((s) => ({ id: s.id, name: s.name, short_name: s.short_name }));
    const allBlocks = Object.values(timeBlocksBySchemeId).flat();
    const availability = (teacherAvailabilityByTeacherId[id] ?? []).flatMap((entry) => {
      const block = allBlocks.find((b) => b.id === entry.time_block_id);
      if (!block) return [];
      return [
        {
          time_block_id: entry.time_block_id,
          day_of_week: block.day_of_week,
          position: block.position,
          status: entry.status,
        },
      ];
    });
    return HttpResponse.json({
      ...base,
      qualifications,
      availability,
    });
  }),
  http.put(`${BASE}/api/teachers/:teacher_id/qualifications`, async ({ request, params }) => {
    const id = String(params.teacher_id);
    const body = (await request.json()) as { subject_ids: string[] };
    teacherQualsByTeacherId[id] = [...body.subject_ids];
    const base = initialTeachers.find((t) => t.id === id) ?? initialTeachers[0];
    if (!base) return HttpResponse.json({ detail: "not found" }, { status: 404 });
    const qualifications = body.subject_ids
      .map((sid) => initialSubjects.find((s) => s.id === sid))
      .filter((s): s is (typeof initialSubjects)[number] => s !== undefined)
      .map((s) => ({ id: s.id, name: s.name, short_name: s.short_name }));
    return HttpResponse.json({
      ...base,
      qualifications,
      availability: [],
    });
  }),
  http.put(`${BASE}/api/teachers/:teacher_id/availability`, async ({ request, params }) => {
    const id = String(params.teacher_id);
    const body = (await request.json()) as {
      entries: Array<{ time_block_id: string; status: string }>;
    };
    teacherAvailabilityByTeacherId[id] = [...body.entries];
    const base = initialTeachers.find((t) => t.id === id) ?? initialTeachers[0];
    if (!base) return HttpResponse.json({ detail: "not found" }, { status: 404 });
    return HttpResponse.json({
      ...base,
      qualifications: [],
      availability: body.entries.map((e) => ({ ...e, day_of_week: 0, position: 1 })),
    });
  }),
  http.get(`${BASE}/api/week-schemes`, () => HttpResponse.json(initialWeekSchemes)),
  http.post(`${BASE}/api/week-schemes`, async ({ request }) => {
    const body = (await request.json()) as { name: string; description?: string | null };
    return HttpResponse.json(
      {
        id: "ffffffff-ffff-ffff-ffff-ffffffffffff",
        name: body.name,
        description: body.description ?? null,
        created_at: "2026-04-17T00:00:00Z",
        updated_at: "2026-04-17T00:00:00Z",
      },
      { status: 201 },
    );
  }),
  http.get(`${BASE}/api/week-schemes/:scheme_id`, ({ params }) => {
    const id = String(params.scheme_id);
    const base = initialWeekSchemes.find((s) => s.id === id);
    if (!base) {
      return HttpResponse.json({ detail: "not found" }, { status: 404 });
    }
    return HttpResponse.json({
      ...base,
      time_blocks: timeBlocksBySchemeId[id] ?? [],
    });
  }),
  http.post(`${BASE}/api/week-schemes/:scheme_id/time-blocks`, async ({ request, params }) => {
    const id = String(params.scheme_id);
    const body = (await request.json()) as {
      day_of_week: number;
      position: number;
      start_time: string;
      end_time: string;
    };
    const bucket = timeBlocksBySchemeId[id] ?? [];
    if (bucket.some((b) => b.day_of_week === body.day_of_week && b.position === body.position)) {
      return HttpResponse.json(
        { detail: "A time block with this day and position already exists in this scheme." },
        { status: 409 },
      );
    }
    const created: TimeBlock = {
      id: `tb-${id}-${bucket.length + 1}`,
      day_of_week: body.day_of_week,
      position: body.position,
      start_time: body.start_time,
      end_time: body.end_time,
    };
    timeBlocksBySchemeId[id] = [...bucket, created];
    return HttpResponse.json(created, { status: 201 });
  }),
  http.patch(
    `${BASE}/api/week-schemes/:scheme_id/time-blocks/:block_id`,
    async ({ request, params }) => {
      const schemeId = String(params.scheme_id);
      const blockId = String(params.block_id);
      const body = (await request.json()) as Partial<{
        day_of_week: number;
        position: number;
        start_time: string;
        end_time: string;
      }>;
      const bucket = timeBlocksBySchemeId[schemeId] ?? [];
      const existing = bucket.find((b) => b.id === blockId);
      if (!existing) {
        return HttpResponse.json({ detail: "not found" }, { status: 404 });
      }
      const next: TimeBlock = {
        ...existing,
        day_of_week: body.day_of_week ?? existing.day_of_week,
        position: body.position ?? existing.position,
        start_time: body.start_time ?? existing.start_time,
        end_time: body.end_time ?? existing.end_time,
      };
      if (
        bucket.some(
          (b) =>
            b.id !== blockId && b.day_of_week === next.day_of_week && b.position === next.position,
        )
      ) {
        return HttpResponse.json(
          { detail: "A time block with this day and position already exists in this scheme." },
          { status: 409 },
        );
      }
      timeBlocksBySchemeId[schemeId] = bucket.map((b) => (b.id === blockId ? next : b));
      return HttpResponse.json(next);
    },
  ),
  http.delete(`${BASE}/api/week-schemes/:scheme_id/time-blocks/:block_id`, ({ params }) => {
    const schemeId = String(params.scheme_id);
    const blockId = String(params.block_id);
    const bucket = timeBlocksBySchemeId[schemeId] ?? [];
    timeBlocksBySchemeId[schemeId] = bucket.filter((b) => b.id !== blockId);
    return HttpResponse.json(null, { status: 204 });
  }),
  http.get(`${BASE}/api/stundentafeln`, () => HttpResponse.json(initialStundentafeln)),
  http.post(`${BASE}/api/stundentafeln`, async ({ request }) => {
    const body = (await request.json()) as {
      name: string;
      grade_level: number;
      school_type?: components["schemas"]["SchoolType"];
    };
    return HttpResponse.json(
      {
        id: "aaaa0000-0000-0000-0000-000000000099",
        name: body.name,
        grade_level: body.grade_level,
        school_type: body.school_type ?? "Grundschule",
        created_at: "2026-04-20T00:00:00Z",
        updated_at: "2026-04-20T00:00:00Z",
      },
      { status: 201 },
    );
  }),
  http.get(`${BASE}/api/stundentafeln/:tafel_id`, ({ params }) => {
    const id = String(params.tafel_id);
    const base = initialStundentafeln.find((s) => s.id === id) ?? initialStundentafeln[0];
    if (!base) {
      return HttpResponse.json({ detail: "not found" }, { status: 404 });
    }
    return HttpResponse.json({
      id: base.id,
      name: base.name,
      grade_level: base.grade_level,
      school_type: base.school_type,
      entries: stundentafelEntriesByTafelId[base.id] ?? [],
      created_at: base.created_at,
      updated_at: base.updated_at,
    });
  }),
  http.patch(`${BASE}/api/stundentafeln/:tafel_id`, async ({ request, params }) => {
    const body = (await request.json()) as {
      name?: string;
      grade_level?: number;
      school_type?: components["schemas"]["SchoolType"] | null;
    };
    const id = String(params.tafel_id);
    const base = initialStundentafeln.find((s) => s.id === id) ?? initialStundentafeln[0];
    if (!base) {
      return HttpResponse.json({ detail: "not found" }, { status: 404 });
    }
    return HttpResponse.json({
      id: base.id,
      name: body.name ?? base.name,
      grade_level: body.grade_level ?? base.grade_level,
      school_type: body.school_type ?? base.school_type,
      created_at: base.created_at,
      updated_at: "2026-04-20T00:00:00Z",
    });
  }),
  http.delete(`${BASE}/api/stundentafeln/:tafel_id`, () =>
    HttpResponse.json(null, { status: 204 }),
  ),
  http.post(`${BASE}/api/stundentafeln/:tafel_id/entries`, async ({ request, params }) => {
    const body = (await request.json()) as {
      subject_id: string;
      hours_per_week: number;
      preferred_block_size: number;
    };
    const tafelId = String(params.tafel_id);
    const subject = initialSubjects.find((s) => s.id === body.subject_id);
    const entry = {
      id: "eeee0000-0000-0000-0000-000000000001",
      subject: subject
        ? { id: subject.id, name: subject.name, short_name: subject.short_name }
        : { id: body.subject_id, name: "Unknown subject", short_name: "??" },
      hours_per_week: body.hours_per_week,
      preferred_block_size: body.preferred_block_size,
    };
    const bucket = stundentafelEntriesByTafelId[tafelId] ?? [];
    stundentafelEntriesByTafelId[tafelId] = [...bucket, entry];
    return HttpResponse.json(entry, { status: 201 });
  }),
  http.patch(
    `${BASE}/api/stundentafeln/:tafel_id/entries/:entry_id`,
    async ({ request, params }) => {
      const body = (await request.json()) as {
        hours_per_week?: number;
        preferred_block_size?: number;
      };
      const tafelId = String(params.tafel_id);
      const entryId = String(params.entry_id);
      const bucket = stundentafelEntriesByTafelId[tafelId] ?? [];
      const existing = bucket.find((e) => e.id === entryId);
      if (!existing) {
        return HttpResponse.json({ detail: "not found" }, { status: 404 });
      }
      const updated = {
        ...existing,
        hours_per_week: body.hours_per_week ?? existing.hours_per_week,
        preferred_block_size: body.preferred_block_size ?? existing.preferred_block_size,
      };
      stundentafelEntriesByTafelId[tafelId] = bucket.map((e) => (e.id === entryId ? updated : e));
      return HttpResponse.json(updated);
    },
  ),
  http.delete(`${BASE}/api/stundentafeln/:tafel_id/entries/:entry_id`, ({ params }) => {
    const tafelId = String(params.tafel_id);
    const entryId = String(params.entry_id);
    const bucket = stundentafelEntriesByTafelId[tafelId] ?? [];
    stundentafelEntriesByTafelId[tafelId] = bucket.filter((e) => e.id !== entryId);
    return HttpResponse.json(null, { status: 204 });
  }),
  http.get(`${BASE}/api/classes`, () => HttpResponse.json(initialSchoolClasses)),
  http.post(`${BASE}/api/classes`, async ({ request }) => {
    const body = (await request.json()) as {
      name: string;
      grade_level: number;
      stundentafel_id: string;
      week_scheme_id: string;
      home_room_id?: string | null;
    };
    return HttpResponse.json(
      {
        id: "77777777-7777-7777-7777-777777777777",
        ...body,
        home_room_id: body.home_room_id ?? null,
        created_at: "2026-04-17T00:00:00Z",
        updated_at: "2026-04-17T00:00:00Z",
      },
      { status: 201 },
    );
  }),
  http.patch(`${BASE}/api/classes/:class_id`, async ({ request, params }) => {
    const body = (await request.json()) as {
      name?: string | null;
      grade_level?: number | null;
      stundentafel_id?: string | null;
      week_scheme_id?: string | null;
      home_room_id?: string | null;
    };
    const id = String(params.class_id);
    const base = initialSchoolClasses.find((c) => c.id === id) ?? initialSchoolClasses[0];
    if (!base) {
      return HttpResponse.json({ detail: "not found" }, { status: 404 });
    }
    return HttpResponse.json({
      ...base,
      id,
      name: body.name ?? base.name,
      grade_level: body.grade_level ?? base.grade_level,
      stundentafel_id: body.stundentafel_id ?? base.stundentafel_id,
      week_scheme_id: body.week_scheme_id ?? base.week_scheme_id,
      home_room_id: body.home_room_id === undefined ? base.home_room_id : body.home_room_id,
      updated_at: "2026-04-20T00:00:00Z",
    });
  }),
  http.post(`${BASE}/api/classes/:class_id/generate-lessons`, ({ params }) => {
    const classId = String(params.class_id);
    const schoolClass = initialSchoolClasses.find((c) => c.id === classId);
    if (!schoolClass) return HttpResponse.json({ detail: "not found" }, { status: 404 });
    const subject = initialSubjects[0];
    if (!subject) return HttpResponse.json([], { status: 201 });
    return HttpResponse.json(
      [
        {
          id: "gen-0000-0000-0000-0000-000000000001",
          school_classes: [{ id: schoolClass.id, name: schoolClass.name }],
          subject: { id: subject.id, name: subject.name, short_name: subject.short_name },
          teacher: null,
          hours_per_week: 4,
          preferred_block_size: 1,
          lesson_group_id: null,
          created_at: "2026-04-20T00:00:00Z",
          updated_at: "2026-04-20T00:00:00Z",
        },
      ],
      { status: 201 },
    );
  }),
  http.get(`${BASE}/api/lessons`, () => HttpResponse.json(initialLessons)),
  http.post(`${BASE}/api/lessons`, async ({ request }) => {
    const body = (await request.json()) as {
      school_class_ids: string[];
      subject_id: string;
      teacher_id: string | null;
      hours_per_week: number;
      preferred_block_size: number;
      pre_buffer_minutes?: number;
      post_buffer_minutes?: number;
    };
    const schoolClasses = body.school_class_ids.map((id) => {
      const match = initialSchoolClasses.find((c) => c.id === id);
      return match ? { id: match.id, name: match.name } : { id, name: "Unknown class" };
    });
    const subject = initialSubjects.find((s) => s.id === body.subject_id);
    const teacher =
      body.teacher_id === null
        ? null
        : (initialTeachers.find((t) => t.id === body.teacher_id) ?? null);
    return HttpResponse.json(
      {
        id: "66666666-6666-6666-6666-666666666666",
        school_classes: schoolClasses,
        subject: subject
          ? { id: subject.id, name: subject.name, short_name: subject.short_name }
          : { id: body.subject_id, name: "Unknown subject", short_name: "??" },
        teacher: teacher
          ? {
              id: teacher.id,
              first_name: teacher.first_name,
              last_name: teacher.last_name,
              short_code: teacher.short_code,
            }
          : null,
        hours_per_week: body.hours_per_week,
        preferred_block_size: body.preferred_block_size,
        pre_buffer_minutes: body.pre_buffer_minutes ?? 0,
        post_buffer_minutes: body.post_buffer_minutes ?? 0,
        lesson_group_id: null,
        created_at: "2026-04-20T00:00:00Z",
        updated_at: "2026-04-20T00:00:00Z",
      },
      { status: 201 },
    );
  }),
  http.patch(`${BASE}/api/lessons/:lesson_id`, async ({ request, params }) => {
    const body = (await request.json()) as {
      teacher_id?: string | null;
      hours_per_week?: number;
      preferred_block_size?: number;
      pre_buffer_minutes?: number;
      post_buffer_minutes?: number;
    };
    const [base] = initialLessons;
    if (!base) {
      return HttpResponse.json({ detail: "seed missing" }, { status: 500 });
    }
    return HttpResponse.json({
      ...base,
      id: String(params.lesson_id),
      hours_per_week: body.hours_per_week ?? base.hours_per_week,
      preferred_block_size: body.preferred_block_size ?? base.preferred_block_size,
      pre_buffer_minutes: body.pre_buffer_minutes ?? base.pre_buffer_minutes,
      post_buffer_minutes: body.post_buffer_minutes ?? base.post_buffer_minutes,
      teacher:
        body.teacher_id === undefined
          ? base.teacher
          : body.teacher_id === null
            ? null
            : (() => {
                const match = initialTeachers.find((t) => t.id === body.teacher_id);
                return match
                  ? {
                      id: match.id,
                      first_name: match.first_name,
                      last_name: match.last_name,
                      short_code: match.short_code,
                    }
                  : null;
              })(),
    });
  }),
  http.delete(`${BASE}/api/lessons/:lesson_id`, () => HttpResponse.json(null, { status: 204 })),
  http.get(`${BASE}/api/classes/:classId/schedule`, ({ params }) => {
    const classId = String(params.classId);
    if (classId === "deadbeef-dead-beef-dead-beefdeadbeef") {
      return HttpResponse.json({ detail: "Class not found" }, { status: 404 });
    }
    return HttpResponse.json({
      placements: scheduleByClassId[classId] ?? [],
      supervision_assignments: [],
      quality_issues: [],
      quality_report: qualityReportByClassId[classId] ?? null,
    });
  }),
  http.get(`${BASE}/api/teachers/:teacher_id/schedule`, ({ params }) => {
    const teacherId = String(params.teacher_id);
    const list = teacherSchedulesByTeacherId[teacherId];
    if (!list) {
      return HttpResponse.json({ detail: "Teacher not found" }, { status: 404 });
    }
    return HttpResponse.json({
      placements: list,
      supervision_assignments: [],
      quality_issues: [],
      quality_report: qualityReportByTeacherId[teacherId] ?? EMPTY_QUALITY_REPORT,
    });
  }),
  http.get(`${BASE}/api/rooms/:room_id/schedule`, ({ params }) => {
    const roomId = String(params.room_id);
    const list = roomSchedulesByRoomId[roomId];
    if (!list) {
      return HttpResponse.json({ detail: "Room not found" }, { status: 404 });
    }
    return HttpResponse.json({
      placements: list,
      supervision_assignments: [],
      quality_issues: [],
      quality_report: null,
    });
  }),
  http.post(`${BASE}/api/classes/:classId/schedule`, ({ params }) => {
    const classId = String(params.classId);
    if (classId === "deadbeef-dead-beef-dead-beefdeadbeef") {
      return HttpResponse.json({ detail: "Class not found" }, { status: 404 });
    }
    const placements = scheduleByClassId[classId] ?? [];
    const violations = violationsByClassId[classId] ?? [];
    return HttpResponse.json({
      placements,
      violations,
      soft_score: 0,
      quality_report: qualityReportByClassId[classId] ?? EMPTY_QUALITY_REPORT,
      was_cancelled: false,
      supervision_assignments: [],
      quality_issues: [],
    });
  }),
  http.post(`${BASE}/api/schedule/all`, () => {
    const summaries = Object.entries(scheduleByClassId).map(([classId, placements]) => ({
      class_id: classId,
      placements_count: placements.length,
      violations_count: (violationsByClassId[classId] ?? []).length,
    }));
    const totalPlacements = summaries.reduce((sum, c) => sum + c.placements_count, 0);
    const totalViolations = summaries.reduce((sum, c) => sum + c.violations_count, 0);
    return HttpResponse.json({
      classes: summaries,
      total_placements: totalPlacements,
      total_violations: totalViolations,
    });
  }),
];

export const server = setupServer(...defaultHandlers);
