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

  it("create dialog renders prefer-early-period and avoid-first-period weight inputs", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SubjectsPage />);
    await screen.findByText("Mathematik");
    await user.click(screen.getByRole("button", { name: /neues fach/i }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText(/frühe stunden bevorzugen/i)).toBeInTheDocument();
    expect(within(dialog).getByLabelText(/erste stunde meiden/i)).toBeInTheDocument();
  });

  it("create dialog renders the avoid-last-period weight input", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SubjectsPage />);
    await screen.findByText("Mathematik");
    await user.click(screen.getByRole("button", { name: /neues fach/i }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText(/letzte stunde meiden/i)).toBeInTheDocument();
  });

  it("submitting create dialog with non-zero weights sends the integer fields", async () => {
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
            prefer_early_period: 2,
            avoid_first_period: 3,
            avoid_last_period: 0,
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
    const earlyInput = within(dialog).getByLabelText(/frühe stunden bevorzugen/i);
    await user.clear(earlyInput);
    await user.type(earlyInput, "2");
    const avoidFirstInput = within(dialog).getByLabelText(/erste stunde meiden/i);
    await user.clear(avoidFirstInput);
    await user.type(avoidFirstInput, "3");
    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(requestBody).toHaveBeenCalledOnce();
    });
    const firstCall = requestBody.mock.calls[0];
    if (!firstCall) throw new Error("requestBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body.prefer_early_period).toBe(2);
    expect(body.avoid_first_period).toBe(3);
  });

  it("submitting create dialog with avoid-last-period weight sends the field", async () => {
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
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 4,
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
    const avoidLastInput = within(dialog).getByLabelText(/letzte stunde meiden/i);
    await user.clear(avoidLastInput);
    await user.type(avoidLastInput, "4");
    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(requestBody).toHaveBeenCalledOnce();
    });
    const firstCall = requestBody.mock.calls[0];
    if (!firstCall) throw new Error("requestBody was not called");
    const body = firstCall[0] as Record<string, unknown>;
    expect(body.avoid_last_period).toBe(4);
  });
});
