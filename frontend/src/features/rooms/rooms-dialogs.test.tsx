import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import type { ReactNode } from "react";
import { afterEach, beforeAll, beforeEach, describe, expect, test } from "vitest";
import i18n from "@/i18n/init";
import { roomSuitabilityByRoomId, server } from "../../../tests/msw-handlers";
import { RoomFormDialog } from "./rooms-dialogs";

const BASE = "http://localhost:3000";

function wrapRoomDialog(children: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("RoomFormDialog create flow", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  beforeEach(() => {
    for (const k of Object.keys(roomSuitabilityByRoomId)) roomSuitabilityByRoomId[k] = [];
  });

  afterEach(() => {
    for (const k of Object.keys(roomSuitabilityByRoomId)) roomSuitabilityByRoomId[k] = [];
  });

  test("does not render a suitability mode selector", async () => {
    render(wrapRoomDialog(<RoomFormDialog open onOpenChange={() => {}} submitLabel="Create" />));
    expect(screen.queryByText(/mode/i)).not.toBeInTheDocument();
  });

  test("submits is_external=true when External room checkbox is checked", async () => {
    const captured: Array<Record<string, unknown>> = [];
    server.use(
      http.post(`${BASE}/api/rooms`, async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        captured.push(body);
        return HttpResponse.json(
          {
            id: "dddddddd-dddd-dddd-dddd-dddddddddddd",
            name: body.name,
            short_name: body.short_name,
            capacity: body.capacity ?? null,
            is_external: body.is_external ?? false,
            created_at: "2026-04-17T00:00:00Z",
            updated_at: "2026-04-17T00:00:00Z",
          },
          { status: 201 },
        );
      }),
    );
    render(wrapRoomDialog(<RoomFormDialog open onOpenChange={() => {}} submitLabel="Create" />));
    await userEvent.type(screen.getByLabelText(/^name$/i), "Schwimmbad");
    await userEvent.type(screen.getByLabelText(/short name/i), "SB");
    await userEvent.click(screen.getByLabelText(/external room/i));
    await userEvent.click(screen.getByRole("button", { name: /^create$/i }));
    await waitFor(() => expect(captured.length).toBe(1));
    expect(captured[0]).toEqual(expect.objectContaining({ is_external: true }));
  });

  test("can submit with a selected subject and calls PUT suitability", async () => {
    render(wrapRoomDialog(<RoomFormDialog open onOpenChange={() => {}} submitLabel="Create" />));
    await userEvent.type(screen.getByLabelText(/^name$/i), "Gym");
    await userEvent.type(screen.getByLabelText(/short name/i), "GM");
    const mathChip = await screen.findByRole("button", { name: /Mathematik/ });
    await userEvent.click(mathChip);
    await userEvent.click(screen.getByRole("button", { name: /create/i }));
    await waitFor(() =>
      expect(roomSuitabilityByRoomId["dddddddd-dddd-dddd-dddd-dddddddddddd"]).toEqual([
        "11111111-1111-1111-1111-111111111111",
      ]),
    );
  });
});
