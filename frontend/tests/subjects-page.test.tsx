import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { SubjectsPage } from "@/features/subjects/subjects-page";
import i18n from "@/i18n/init";
import { server } from "./msw-handlers";
import { renderWithProviders } from "./render-helpers";

describe("SubjectsPage", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("de");
  });

  it("renders subjects fetched from the API", async () => {
    renderWithProviders(<SubjectsPage />);
    expect(await screen.findByText("Mathematik")).toBeInTheDocument();
    expect(screen.getByText("MA")).toBeInTheDocument();
  });

  it("creates a subject via the dialog and closes on success", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SubjectsPage />);

    await screen.findByText("Mathematik");
    await user.click(screen.getByRole("button", { name: /neues fach/i }));

    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "Deutsch");
    await user.type(within(dialog).getByLabelText(/kürzel/i), "DE");
    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  it("create dialog renders prefer-early-periods and avoid-first-period checkboxes", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SubjectsPage />);
    await screen.findByText("Mathematik");
    await user.click(screen.getByRole("button", { name: /neues fach/i }));
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByRole("checkbox", { name: /frühe stunden bevorzugen/i }),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("checkbox", { name: /erste stunde vermeiden/i }),
    ).toBeInTheDocument();
  });

  it("create dialog renders the avoid-last-period checkbox", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SubjectsPage />);
    await screen.findByText("Mathematik");
    await user.click(screen.getByRole("button", { name: /neues fach/i }));
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByRole("checkbox", { name: /letzte stunde meiden/i }),
    ).toBeInTheDocument();
  });

  it("submitting create dialog with both checkboxes ticked sends both fields", async () => {
    const user = userEvent.setup();
    const requestBody = vi.fn();

    server.use(
      http.post("http://localhost:3000/api/subjects", async ({ request }) => {
        const body = await request.json();
        requestBody(body);
        return HttpResponse.json(
          {
            id: "22222222-2222-2222-2222-222222222222",
            name: "Test",
            short_name: "TS",
            color: "chart-1",
            prefer_early_periods: true,
            avoid_first_period: true,
            created_at: "2026-04-28T00:00:00Z",
            updated_at: "2026-04-28T00:00:00Z",
          },
          { status: 201 },
        );
      }),
    );

    renderWithProviders(<SubjectsPage />);
    await screen.findByText("Mathematik");
    await user.click(screen.getByRole("button", { name: /neues fach/i }));

    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "Test");
    await user.type(within(dialog).getByLabelText(/kürzel/i), "TS");
    await user.click(within(dialog).getByRole("checkbox", { name: /frühe stunden bevorzugen/i }));
    await user.click(within(dialog).getByRole("checkbox", { name: /erste stunde vermeiden/i }));
    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(requestBody).toHaveBeenCalledOnce();
    });
    const firstCall = requestBody.mock.calls[0];
    if (!firstCall) throw new Error("requestBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body.prefer_early_periods).toBe(true);
    expect(body.avoid_first_period).toBe(true);
  });

  it("submitting create dialog with avoid-last-period ticked sends the field", async () => {
    const user = userEvent.setup();
    const requestBody = vi.fn();

    server.use(
      http.post("http://localhost:3000/api/subjects", async ({ request }) => {
        const body = await request.json();
        requestBody(body);
        return HttpResponse.json(
          {
            id: "33333333-3333-3333-3333-333333333333",
            name: "Test",
            short_name: "TS",
            color: "chart-1",
            prefer_early_periods: false,
            avoid_first_period: false,
            avoid_last_period: true,
            created_at: "2026-05-01T00:00:00Z",
            updated_at: "2026-05-01T00:00:00Z",
          },
          { status: 201 },
        );
      }),
    );

    renderWithProviders(<SubjectsPage />);
    await screen.findByText("Mathematik");
    await user.click(screen.getByRole("button", { name: /neues fach/i }));

    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "Test");
    await user.type(within(dialog).getByLabelText(/kürzel/i), "TS");
    await user.click(within(dialog).getByRole("checkbox", { name: /letzte stunde meiden/i }));
    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(requestBody).toHaveBeenCalledOnce();
    });
    const firstCall = requestBody.mock.calls[0];
    if (!firstCall) throw new Error("requestBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body.avoid_last_period).toBe(true);
  });
});
