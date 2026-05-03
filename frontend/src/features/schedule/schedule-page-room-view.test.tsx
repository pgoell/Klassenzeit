import { screen } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n/init";
import { roomSchedulesByRoomId } from "../../../tests/msw-handlers";
import { renderWithProviders } from "../../../tests/render-helpers";
import { SchedulePageRoomView } from "./schedule-page-room-view";

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

beforeEach(() => {
  for (const k of Object.keys(roomSchedulesByRoomId)) {
    delete roomSchedulesByRoomId[k];
  }
});

describe("SchedulePageRoomView", () => {
  it("renders the placeholder body when no room is selected", async () => {
    renderWithProviders(<SchedulePageRoomView />);
    expect(await screen.findByText(/select a room above/i)).toBeInTheDocument();
  });
});
