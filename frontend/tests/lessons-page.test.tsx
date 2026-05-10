import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { LessonsPage } from "@/features/lessons/lessons-page";
import i18n from "@/i18n/init";
import { initialLessons, initialSchoolClasses, server } from "./msw-handlers";
import { renderWithProviders } from "./render-helpers";

const SECOND_CLASS_ID = "88888888-8888-8888-8888-888888888889";
const BASE = "http://localhost:3000";

describe("LessonsPage", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("de");
  });

  beforeEach(() => {
    // Per-test seed: setup.ts's `afterEach(server.resetHandlers)` wipes any
    // overrides between specs, so the multi-class fixtures must be re-applied
    // on every spec.
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
      http.get(`${BASE}/api/lessons`, () =>
        HttpResponse.json(
          initialLessons.map((l) => ({
            ...l,
            school_classes: [
              { id: "88888888-8888-8888-8888-888888888888", name: "1a" },
              { id: SECOND_CLASS_ID, name: "1b" },
            ],
          })),
        ),
      ),
    );
  });

  it("renders lessons fetched from the API", async () => {
    renderWithProviders(<LessonsPage />);
    expect(await screen.findByText(/1a, 1b/)).toBeInTheDocument();
    expect(screen.getByText(/mathematik/i)).toBeInTheDocument();
    expect(screen.getByText("SCH")).toBeInTheDocument();
  });

  it("creates a lesson via the dialog and closes on success", async () => {
    const user = userEvent.setup();
    renderWithProviders(<LessonsPage />);

    await screen.findByText(/1a, 1b/);
    await user.click(screen.getByRole("button", { name: /neuer unterricht/i }));

    const dialog = await screen.findByRole("dialog");

    // Tick a class checkbox
    const firstClassCheckbox = await within(dialog).findByRole("checkbox", { name: /^1a$/ });
    await user.click(firstClassCheckbox);

    // Subject select
    await user.click(within(dialog).getByRole("combobox", { name: /fach/i }));
    await user.click(await screen.findByRole("option", { name: /mathematik/i }));

    // Teacher: leave the pin switch off so the solver picks a teacher (teacher_id null).

    // Hours: keep the default 1
    // Block size: keep the default single period

    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});
