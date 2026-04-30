import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { SchoolClassesPage } from "@/features/school-classes/school-classes-page";
import i18n from "@/i18n/init";
import { server } from "./msw-handlers";
import { renderWithProviders } from "./render-helpers";

describe("SchoolClassesPage", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("de");
  });

  it("renders school classes fetched from the API", async () => {
    renderWithProviders(<SchoolClassesPage />);
    expect(await screen.findByText("1a")).toBeInTheDocument();
    expect(screen.getByText("Grundschule Klasse 1")).toBeInTheDocument();
    expect(screen.getByText("Standardwoche")).toBeInTheDocument();
  });

  it("creates a school class via the dialog and closes on success", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SchoolClassesPage />);

    await screen.findByText("1a");
    await user.click(screen.getByRole("button", { name: /neue klasse/i }));

    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "1b");

    // Open the curriculum select and pick the seeded entry.
    await user.click(within(dialog).getByRole("combobox", { name: /stundentafel/i }));
    await user.click(await screen.findByRole("option", { name: /grundschule klasse 1/i }));

    // Open the week scheme select and pick the seeded entry.
    await user.click(within(dialog).getByRole("combobox", { name: /wochenschema/i }));
    await user.click(await screen.findByRole("option", { name: /standardwoche/i }));

    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});

describe("SchoolClassesPage Generate-lessons toast", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("de");
  });

  beforeEach(() => {
    vi.spyOn(window, "alert").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("shows a success toast with the interpolated lesson count", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SchoolClassesPage />);
    await screen.findByText("1a");
    const row = screen.getByText("1a").closest("tr");
    if (!row) throw new Error("row for 1a not found");
    await user.click(within(row).getByRole("button", { name: /unterricht erzeugen/i }));

    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /^erzeugen$/i }));

    expect(await screen.findByText(/1 Stunde erzeugt/i)).toBeInTheDocument();
    expect(window.alert).not.toHaveBeenCalled();
  });

  it("shows an info toast when the backend generated no lessons", async () => {
    server.use(
      http.post("http://localhost:3000/api/classes/:class_id/generate-lessons", () =>
        HttpResponse.json([], { status: 201 }),
      ),
    );
    const user = userEvent.setup();
    renderWithProviders(<SchoolClassesPage />);
    await screen.findByText("1a");
    const row = screen.getByText("1a").closest("tr");
    if (!row) throw new Error("row for 1a not found");
    await user.click(within(row).getByRole("button", { name: /unterricht erzeugen/i }));

    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /^erzeugen$/i }));

    expect(await screen.findByText(/kein neuer unterricht erzeugt/i)).toBeInTheDocument();
    expect(window.alert).not.toHaveBeenCalled();
  });
});

describe("SchoolClassesPage home-room dropdown", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("de");
  });

  it("renders the home-room dropdown when creating a school class", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SchoolClassesPage />);
    await screen.findByText("1a");
    await user.click(screen.getByRole("button", { name: /neue klasse/i }));
    const dialog = await screen.findByRole("dialog");
    const trigger = within(dialog).getByRole("combobox", { name: /klassenraum/i });
    await user.click(trigger);
    expect(await screen.findByRole("option", { name: /klasse 1a/i })).toBeInTheDocument();
  });

  it("submits home_room_id when a Klassenraum is selected", async () => {
    let capturedBody: Record<string, unknown> | null = null;
    server.use(
      http.post("http://localhost:3000/api/classes", async ({ request }) => {
        capturedBody = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(
          {
            id: "77777777-7777-7777-7777-777777777777",
            ...capturedBody,
            created_at: "2026-04-17T00:00:00Z",
            updated_at: "2026-04-17T00:00:00Z",
          },
          { status: 201 },
        );
      }),
    );

    const user = userEvent.setup();
    renderWithProviders(<SchoolClassesPage />);
    await screen.findByText("1a");
    await user.click(screen.getByRole("button", { name: /neue klasse/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "1c");
    await user.click(within(dialog).getByRole("combobox", { name: /stundentafel/i }));
    await user.click(await screen.findByRole("option", { name: /grundschule klasse 1/i }));
    await user.click(within(dialog).getByRole("combobox", { name: /wochenschema/i }));
    await user.click(await screen.findByRole("option", { name: /standardwoche/i }));
    await user.click(within(dialog).getByRole("combobox", { name: /klassenraum/i }));
    await user.click(await screen.findByRole("option", { name: /klasse 1a/i }));
    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(capturedBody).not.toBeNull();
    });
    expect(capturedBody).toMatchObject({
      home_room_id: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaab",
    });
  });

  it("submits home_room_id as null when 'Kein Klassenraum' is selected", async () => {
    let capturedBody: Record<string, unknown> | null = null;
    server.use(
      http.post("http://localhost:3000/api/classes", async ({ request }) => {
        capturedBody = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(
          {
            id: "77777777-7777-7777-7777-777777777777",
            ...capturedBody,
            created_at: "2026-04-17T00:00:00Z",
            updated_at: "2026-04-17T00:00:00Z",
          },
          { status: 201 },
        );
      }),
    );

    const user = userEvent.setup();
    renderWithProviders(<SchoolClassesPage />);
    await screen.findByText("1a");
    await user.click(screen.getByRole("button", { name: /neue klasse/i }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "1d");
    await user.click(within(dialog).getByRole("combobox", { name: /stundentafel/i }));
    await user.click(await screen.findByRole("option", { name: /grundschule klasse 1/i }));
    await user.click(within(dialog).getByRole("combobox", { name: /wochenschema/i }));
    await user.click(await screen.findByRole("option", { name: /standardwoche/i }));
    // The placeholder option (Kein Klassenraum) is selected by default; submit without changing it.
    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(capturedBody).not.toBeNull();
    });
    expect(capturedBody).toMatchObject({
      home_room_id: null,
    });
  });
});
