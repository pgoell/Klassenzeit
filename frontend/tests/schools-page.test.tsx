import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it } from "vitest";
import { SchoolsPage } from "@/features/schools/schools-page";
import i18n from "@/i18n/init";
import { renderWithProviders } from "./render-helpers";

describe("SchoolsPage", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("de");
  });

  it("renders schools fetched from the API", async () => {
    renderWithProviders(<SchoolsPage />);
    expect(await screen.findByText("Default Schule")).toBeInTheDocument();
    expect(screen.getByText("Zweite Grundschule")).toBeInTheDocument();
  });

  it("creates a school via the dialog and closes on success", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SchoolsPage />);

    await screen.findByText("Default Schule");
    await user.click(screen.getByRole("button", { name: /neue schule/i }));

    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText(/^name$/i), "Dritte Schule");
    await user.type(within(dialog).getByLabelText(/kürzel/i), "DR");
    await user.click(within(dialog).getByRole("button", { name: /^anlegen$/i }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});
