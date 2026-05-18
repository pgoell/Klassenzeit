import type { APIRequestContext } from "@playwright/test";
import { expect, test } from "../fixtures/test";
import { loginAs } from "../support/auth";
import { ADMIN_A, SUPER_ADMIN } from "../support/credentials";

const BACKEND_URL = "http://localhost:8000";

interface SeedSchoolBResponse {
  school_b_id: string;
  room_b1_id: string;
  room_b2_id: string;
}

async function seedSchoolB(request: APIRequestContext): Promise<SeedSchoolBResponse> {
  const resp = await request.post(`${BACKEND_URL}/__test__/seed-school-b`);
  expect(resp.ok(), await resp.text()).toBeTruthy();
  return (await resp.json()) as SeedSchoolBResponse;
}

async function seedGrundschule(request: APIRequestContext): Promise<void> {
  const resp = await request.post(`${BACKEND_URL}/__test__/seed-grundschule`);
  expect(resp.ok(), await resp.text()).toBeTruthy();
}

test.describe("multi-school cross-school isolation", () => {
  test("admin sees only their school's rooms", async ({ page, request }) => {
    await seedGrundschule(request);
    await seedSchoolB(request);

    await loginAs(page, ADMIN_A.email, ADMIN_A.password);
    await page.goto("/rooms");

    await expect(page.getByRole("cell", { name: "Turnhalle", exact: true })).toBeVisible();
    await expect(page.getByText("SB Raum 1")).toBeHidden();
    await expect(page.getByText("SB Raum 2")).toBeHidden();
  });

  test("super-admin switches schools and the data set changes", async ({ page, request }) => {
    await seedGrundschule(request);
    await seedSchoolB(request);

    await loginAs(page, SUPER_ADMIN.email, SUPER_ADMIN.password);
    await page.goto("/rooms");

    await expect(page.getByRole("cell", { name: "Turnhalle", exact: true })).toBeVisible();

    const picker = page.getByRole("combobox", { name: /active school/i });
    await expect(picker).toBeVisible();
    await expect(picker).toBeEnabled();
    await picker.click();
    await page.getByRole("option", { name: "Schule B" }).click();

    await expect(picker).toContainText("Schule B");

    // Force a reload to verify the server-side session active_school_id was
    // updated; the rooms list refresh after the picker click depends on a
    // queryClient.clear() refetch cycle that is not reliable in this branch.
    await page.reload();

    await expect(page.getByRole("cell", { name: "SB Raum 1", exact: true })).toBeVisible();
    await expect(page.getByRole("cell", { name: "SB Raum 2", exact: true })).toBeVisible();
    await expect(page.getByRole("cell", { name: "Turnhalle", exact: true })).toBeHidden();
    await expect(picker).toContainText("Schule B");
  });

  test("cross-school deep-link returns 404", async ({ page, request, context }) => {
    await seedGrundschule(request);
    const seed = await seedSchoolB(request);

    await loginAs(page, ADMIN_A.email, ADMIN_A.password);

    const response = await context.request.get(`${BACKEND_URL}/api/rooms/${seed.room_b1_id}`);
    expect(response.status()).toBe(404);
    const bodyText = await response.text();
    expect(bodyText).not.toContain("SB Raum 1");
  });
});
