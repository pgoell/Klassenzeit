import { expect, test } from "../fixtures/test";
import { URLS } from "../support/urls";

const BACKEND_URL = "http://localhost:8000";

interface SchoolClassListRow {
  id: string;
  name: string;
}

test.describe("Schedule drag-and-drop", () => {
  test("admin drags a placement to an empty slot and the move auto-pins and persists", async ({
    page,
    request,
  }) => {
    // Seed the Grundschule fixture (resetBackend auto-fixture truncated already).
    const seedResp = await request.post(`${BACKEND_URL}/__test__/seed-grundschule`);
    expect(seedResp.ok(), await seedResp.text()).toBeTruthy();

    // Generate lessons for Klasse 1a via the UI so the auto-assign teacher path runs.
    await page.goto(URLS.dashboard);
    await page.getByRole("link", { name: "School classes", exact: true }).click();
    await page.reload();
    const row1a = page.getByRole("row", { name: /1a/ });
    await expect(row1a).toBeVisible();
    await row1a.getByRole("button", { name: "Generate lessons", exact: true }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Generate", exact: true }).click();
    await expect(page.getByText(/\d+ lessons? created/i)).toBeVisible();

    // Look up the 1a class id and deep-link the schedule page.
    const classesResp = await request.get(`${BACKEND_URL}/api/classes`);
    expect(classesResp.ok(), await classesResp.text()).toBeTruthy();
    const classes = (await classesResp.json()) as SchoolClassListRow[];
    const class1a = classes.find((c) => c.name === "1a");
    if (!class1a) {
      throw new Error("seeded class 1a was not found in GET /api/classes");
    }
    const scheduleUrl = `${URLS.schedule}?view=class&class=${class1a.id}`;
    await page.goto(scheduleUrl);

    // Run the per-class solver from the empty-state CTA so a populated grid renders.
    await page.getByRole("button", { name: "Generate schedule", exact: true }).click();
    await expect(page.locator(".kz-ws-grid")).toBeVisible();
    await expect(page.locator('[data-variant="period"]').first()).toBeVisible();

    // Pick the first occupied placement card and the first empty slot in the grid.
    const sourceCard = page.locator('[data-testid^="placement-card-"]').first();
    await expect(sourceCard).toBeVisible();
    const lessonId = await sourceCard.getAttribute("data-lesson-id");
    if (!lessonId) {
      throw new Error("source placement card has no data-lesson-id");
    }

    const emptySlot = page.locator('[data-testid^="empty-slot-"]').first();
    await expect(emptySlot).toBeVisible();
    const targetTimeBlockId = await emptySlot.getAttribute("data-time-block-id");
    if (!targetTimeBlockId) {
      throw new Error("empty slot has no data-time-block-id");
    }

    const sourceBox = await sourceCard.boundingBox();
    const targetBox = await emptySlot.boundingBox();
    if (!sourceBox || !targetBox) {
      throw new Error("could not measure source / target bounding boxes");
    }

    // Manual mouse sequence: dnd-kit's PointerSensor ignores Playwright's
    // synthesised HTML5 dragTo events. The intermediate move after mouse.down
    // is required to clear dnd-kit's default activation distance.
    await page.mouse.move(sourceBox.x + sourceBox.width / 2, sourceBox.y + sourceBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(
      sourceBox.x + sourceBox.width / 2 + 12,
      sourceBox.y + sourceBox.height / 2 + 12,
    );
    await page.mouse.move(targetBox.x + targetBox.width / 2, targetBox.y + targetBox.height / 2, {
      steps: 10,
    });
    await page.mouse.up();

    // The card should now live under the previously-empty slot's time block id.
    const movedCard = page.locator(
      `[data-testid="placement-slot-${targetTimeBlockId}"] [data-lesson-id="${lessonId}"]`,
    );
    await expect(movedCard).toBeVisible();

    // Auto-pin: the visible toggle button now reads "Unpin this lesson".
    await expect(movedCard.getByRole("button", { name: "Unpin this lesson" })).toBeVisible();

    // Reload and re-assert: the move and the pin both persisted.
    await page.goto(scheduleUrl);
    await expect(page.locator(".kz-ws-grid")).toBeVisible();
    const movedAfterReload = page.locator(
      `[data-testid="placement-slot-${targetTimeBlockId}"] [data-lesson-id="${lessonId}"]`,
    );
    await expect(movedAfterReload).toBeVisible();
    await expect(movedAfterReload.getByRole("button", { name: "Unpin this lesson" })).toBeVisible();
  });
});
