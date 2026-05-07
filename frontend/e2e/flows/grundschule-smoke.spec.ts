import { expect, test } from "../fixtures/test";
import { URLS } from "../support/urls";

const BACKEND_URL = "http://localhost:8000";

interface SchoolClassListRow {
  id: string;
  name: string;
}

test.describe("Grundschule smoke", () => {
  test("seed, generate lessons, assign teachers, generate schedule, grid renders", async ({
    page,
    request,
  }) => {
    // The resetBackend auto-fixture has already truncated before this test starts.
    const seedResp = await request.post(`${BACKEND_URL}/__test__/seed-grundschule`);
    expect(seedResp.ok(), await seedResp.text()).toBeTruthy();

    // Navigate to the dashboard, then click into school-classes via the sidebar
    // so the TanStack Query cache for /api/classes populates after the seed.
    await page.goto(URLS.dashboard);
    await page.getByRole("link", { name: "School classes", exact: true }).click();
    await page.reload();

    // The seeded classes appear in the table. Scope the Generate click to the 1a row
    // so we never accidentally click the analogous button on 2a/3a/4a.
    const row1a = page.getByRole("row", { name: /1a/ });
    await expect(row1a).toBeVisible();
    await row1a.getByRole("button", { name: "Generate lessons", exact: true }).click();

    // The confirm dialog's primary button reads exactly "Generate". Use exact: true
    // because "Generate lessons" and "Generate schedule" would otherwise both match.
    await page.getByRole("dialog").getByRole("button", { name: "Generate", exact: true }).click();

    // Sonner toast confirms the lesson-generate mutation resolved.
    await expect(page.getByText(/\d+ lessons? created/i)).toBeVisible();

    // Fetch the 1a class ID so we can deep-link the schedule page and skip the
    // Radix Select class picker entirely.
    const classesResp = await request.get(`${BACKEND_URL}/api/classes`);
    expect(classesResp.ok(), await classesResp.text()).toBeTruthy();
    const classes = (await classesResp.json()) as SchoolClassListRow[];
    const class1a = classes.find((c) => c.name === "1a");
    if (!class1a) {
      throw new Error("seeded class 1a was not found in GET /api/classes");
    }

    await page.goto(`${URLS.schedule}?class=${class1a.id}`);

    // The schedule page renders the empty state until the solver runs.
    await page.getByRole("button", { name: "Generate schedule", exact: true }).click();

    // The schedule POST awaits the LAHC solver (KZ_SOLVE_DEADLINE_MS=2000 in
    // playwright.config.ts), so the grid takes up to ~3 s to mount after the
    // click. Bumping the toBeVisible timeout absorbs the solver latency without
    // racing the empty-state to populated transition.
    await expect(page.locator(".kz-ws-grid")).toBeVisible({ timeout: 10000 });
    await expect(page.locator('[data-variant="period"]').first()).toBeVisible();
    await expect(
      page.locator('[data-variant="period"]').getByText("Deutsch").first(),
    ).toBeVisible();
  });
});
