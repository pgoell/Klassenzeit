import { screen } from "@testing-library/react";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n/init";
import { teacherSchedulesByTeacherId } from "../../../tests/msw-handlers";
import { renderWithProviders } from "../../../tests/render-helpers";
import { SchedulePageTeacherView } from "./schedule-page-teacher-view";

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

beforeEach(() => {
  for (const k of Object.keys(teacherSchedulesByTeacherId)) {
    delete teacherSchedulesByTeacherId[k];
  }
});

describe("SchedulePageTeacherView", () => {
  it("renders the placeholder body when no teacher is selected", async () => {
    renderWithProviders(<SchedulePageTeacherView />);
    expect(await screen.findByText(/select a teacher above/i)).toBeInTheDocument();
  });
});
