import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { I18nextProvider } from "react-i18next";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { Toaster } from "@/components/ui/sonner";
import i18n from "@/i18n/init";
import { timeBlocksBySchemeId } from "../../../tests/msw-handlers";
import { TimeBlocksGrid } from "./time-blocks-grid";

const schemeId = "cccccccc-cccc-cccc-cccc-cccccccccccc";

function wrapTimeBlocksGrid() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <I18nextProvider i18n={i18n}>
        <TimeBlocksGrid schemeId={schemeId} />
        <Toaster />
      </I18nextProvider>
    </QueryClientProvider>,
  );
}

describe("TimeBlocksGrid", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  beforeEach(() => {
    timeBlocksBySchemeId[schemeId] = [];
  });

  it("renders filled cells with start and end on two lines and empty cells with an add affordance", async () => {
    timeBlocksBySchemeId[schemeId] = [
      { id: "a", day_of_week: 0, position: 1, start_time: "08:00:00", end_time: "08:45:00" },
      { id: "b", day_of_week: 1, position: 1, start_time: "08:00:00", end_time: "08:45:00" },
    ];
    wrapTimeBlocksGrid();
    expect(await screen.findAllByText("08:00")).toHaveLength(2);
    expect(await screen.findAllByText("08:45")).toHaveLength(2);
    const emptyButtons = await screen.findAllByRole("button", {
      name: /add time block on .* at period \d+/i,
    });
    expect(emptyButtons.length).toBeGreaterThanOrEqual(38);
  });

  it("opens TimeBlockFormDialog in create mode when an empty cell is clicked", async () => {
    const user = userEvent.setup();
    wrapTimeBlocksGrid();
    const emptyButton = await screen.findByRole("button", {
      name: /add time block on tuesday at period 3/i,
    });
    await user.click(emptyButton);
    const dialog = await screen.findByRole("dialog", { name: /add time block/i });
    expect(within(dialog).getByRole("combobox", { name: /day/i })).toHaveTextContent(/tuesday/i);
    expect(within(dialog).getByLabelText(/period/i)).toHaveValue(3);
  });

  it("opens TimeBlockFormDialog in edit mode when a filled cell is clicked", async () => {
    timeBlocksBySchemeId[schemeId] = [
      { id: "a", day_of_week: 2, position: 2, start_time: "09:00:00", end_time: "09:45:00" },
    ];
    const user = userEvent.setup();
    wrapTimeBlocksGrid();
    const filledButton = await screen.findByRole("button", {
      name: /edit time block on wednesday at period 2/i,
    });
    await user.click(filledButton);
    const dialog = await screen.findByRole("dialog", { name: /edit time block/i });
    expect(within(dialog).getByLabelText(/start/i)).toHaveValue("09:00");
    expect(within(dialog).getByLabelText(/end/i)).toHaveValue("09:45");
  });

  it("pre-fills create mode with start and end from any existing block at the same position", async () => {
    timeBlocksBySchemeId[schemeId] = [
      { id: "a", day_of_week: 0, position: 3, start_time: "10:15:00", end_time: "11:00:00" },
    ];
    const user = userEvent.setup();
    wrapTimeBlocksGrid();
    const emptyButton = await screen.findByRole("button", {
      name: /add time block on tuesday at period 3/i,
    });
    await user.click(emptyButton);
    const dialog = await screen.findByRole("dialog", { name: /add time block/i });
    expect(within(dialog).getByLabelText(/start/i)).toHaveValue("10:15");
    expect(within(dialog).getByLabelText(/end/i)).toHaveValue("11:00");
  });

  it("opens the delete confirm from the edit dialog footer Delete button", async () => {
    timeBlocksBySchemeId[schemeId] = [
      { id: "a", day_of_week: 0, position: 1, start_time: "08:00:00", end_time: "08:45:00" },
    ];
    const user = userEvent.setup();
    wrapTimeBlocksGrid();
    const filledButton = await screen.findByRole("button", {
      name: /edit time block on monday at period 1/i,
    });
    await user.click(filledButton);
    const editDialog = await screen.findByRole("dialog", { name: /edit time block/i });
    await user.click(within(editDialog).getByRole("button", { name: /^delete$/i }));
    // Intent: at minimum a delete-confirm dialog must appear. The implementation may close the
    // underlying edit dialog or leave it stacked underneath; either is acceptable, so we only assert
    // the delete dialog exists (mirrors frontend/CLAUDE.md's "Stacked Radix Dialogs count as
    // multiple role='dialog' nodes" guidance).
    await waitFor(() =>
      expect(
        screen.getAllByRole("dialog").some((d) => /delete time block/i.test(d.textContent ?? "")),
      ).toBe(true),
    );
  });
});
