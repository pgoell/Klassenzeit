import type { Page } from "@playwright/test";
import { URLS } from "./urls";

export async function loginAs(page: Page, email: string, password: string): Promise<void> {
  await page.goto(URLS.login);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Log in" }).click();
  await page.getByRole("heading", { name: /welcome back/i }).waitFor({ state: "visible" });
}
