import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeAll, describe, expect, it } from "vitest";
import i18n from "@/i18n/init";
import { StundentafelFormDialog } from "./stundentafeln-dialogs";

function wrapStundentafelDialog(children: ReactNode) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

describe("StundentafelFormDialog", () => {
  it("renders the school-type Select with five options, defaults to Grundschule, and submits the picked value", async () => {
    const user = userEvent.setup();
    render(wrapStundentafelDialog(<StundentafelFormDialog open onOpenChange={() => {}} />));

    const trigger = await screen.findByRole("combobox", { name: /school type/i });
    expect(trigger).toHaveTextContent(/grundschule/i);

    await user.click(trigger);
    expect(await screen.findByRole("option", { name: /grundschule/i })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: /hauptschule/i })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: /realschule/i })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: /gymnasium/i })).toBeInTheDocument();
    expect(await screen.findByRole("option", { name: /gesamtschule/i })).toBeInTheDocument();

    await user.click(await screen.findByRole("option", { name: /gymnasium/i }));

    const nameInput = screen.getByLabelText(/name/i);
    fireEvent.change(nameInput, { target: { value: "Gymnasium 5" } });
    const gradeInput = screen.getByLabelText(/grade/i);
    fireEvent.change(gradeInput, { target: { value: "5" } });

    await user.click(screen.getByRole("button", { name: /create/i }));

    await waitFor(() => {
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
  });

  it("rejects grade_level above 13 with the Zod max-13 message", async () => {
    render(wrapStundentafelDialog(<StundentafelFormDialog open onOpenChange={() => {}} />));

    const nameInput = screen.getByLabelText(/name/i);
    fireEvent.change(nameInput, { target: { value: "Out of bounds" } });
    const gradeInput = screen.getByLabelText(/grade/i);
    fireEvent.change(gradeInput, { target: { value: "14" } });

    const form = nameInput.closest("form");
    if (!form) throw new Error("form not found");
    fireEvent.submit(form);

    await waitFor(() => {
      const grade = screen.getByLabelText(/grade/i);
      expect(grade).toHaveAttribute("aria-invalid", "true");
    });
    expect(screen.getByText(/Too big|13/i)).toBeInTheDocument();
  });
});
