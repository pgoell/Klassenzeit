import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { I18nextProvider } from "react-i18next";
import { beforeAll, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n/init";
import type { components } from "@/lib/api-types";
import { QualityIssueSidebar } from "./quality-issue-sidebar";

type QualityIssue = components["schemas"]["QualityIssueResponse"];

const CLASS_ID = "00000000-0000-0000-0000-000000000001";
const SUBJECT_ID = "00000000-0000-0000-0000-000000000002";

function wrapWithI18n(ui: React.ReactNode) {
  return <I18nextProvider i18n={i18n}>{ui}</I18nextProvider>;
}

describe("QualityIssueSidebar", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders empty state when no issues", () => {
    render(wrapWithI18n(<QualityIssueSidebar issues={[]} onIssueClick={() => {}} />));
    expect(screen.getByText("No quality issues found.")).toBeInTheDocument();
  });

  it("renders one row per issue, grouped by kind", () => {
    const issues: QualityIssue[] = [
      {
        kind: "room_hop",
        school_class_id: CLASS_ID,
        day_of_week: 1,
        subject_id: SUBJECT_ID,
        detail: { rooms: ["r1", "r2"] },
        cells: [{ day_of_week: 1, position: 1 }],
      },
    ];
    render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={issues}
          onIssueClick={() => {}}
          subjectMap={new Map([[SUBJECT_ID, "Mathe"]])}
        />,
      ),
    );
    // Section header and button both carry "Room hop".
    expect(screen.getAllByText(/Room hop/i).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /Room hop/i })).toBeInTheDocument();
  });

  it("invokes onIssueClick with the issue when a row is clicked", async () => {
    const onIssueClick = vi.fn();
    const issues: QualityIssue[] = [
      {
        kind: "room_hop",
        school_class_id: CLASS_ID,
        day_of_week: 1,
        subject_id: SUBJECT_ID,
        detail: { rooms: ["r1", "r2"] },
        cells: [{ day_of_week: 1, position: 4 }],
      },
    ];
    render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={issues}
          onIssueClick={onIssueClick}
          subjectMap={new Map([[SUBJECT_ID, "Mathe"]])}
        />,
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: /Room hop/i }));
    expect(onIssueClick).toHaveBeenCalledTimes(1);
    expect(onIssueClick).toHaveBeenCalledWith(issues[0]);
  });

  it("disables the button and does not fire onIssueClick when cells are empty", async () => {
    const onIssueClick = vi.fn();
    const issues: QualityIssue[] = [
      {
        kind: "imbalance",
        school_class_id: CLASS_ID,
        detail: { spread: 3, max_spread: 2, daily: [2, 5, 5, 5, 5] },
        cells: [],
      },
    ];
    render(wrapWithI18n(<QualityIssueSidebar issues={issues} onIssueClick={onIssueClick} />));
    const button = screen.getByRole("button", { name: /Imbalanced day load/i });
    expect(button).toBeDisabled();
    await userEvent.click(button);
    expect(onIssueClick).not.toHaveBeenCalled();
  });
});
