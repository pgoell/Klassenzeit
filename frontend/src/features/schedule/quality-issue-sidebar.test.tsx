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

type QualityReport = components["schemas"]["QualityReportResponse"];

function emptyQualityReport(overrides: Partial<QualityReport> = {}): QualityReport {
  return {
    hard_violations: 0,
    unplaced_hours: 0,
    class_gap_hours: 0,
    class_gap_hours_by_class: {},
    teacher_gap_hours: 0,
    teacher_gap_hours_by_teacher: {},
    class_day_balance_cost: 0,
    class_day_balance_cost_by_class: {},
    home_room_misses: 0,
    home_room_misses_by_class: {},
    prefer_early_units: 0,
    avoid_first_units: 0,
    avoid_last_units: 0,
    prefer_late_units: 0,
    prefer_class_teacher_misses: 0,
    weighted_score: 0,
    worst_per_class_spread: 0,
    worst_per_class_interior_gaps: 0,
    soft_pin_misses: 0,
    supervision_spread_raw: 0,
    ...overrides,
  };
}

describe("QualityIssueSidebar metrics section", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders the metrics section above issues when attribution is non-zero", () => {
    const qualityReport = emptyQualityReport({
      class_gap_hours_by_class: { [CLASS_ID]: 3 },
      home_room_misses_by_class: { [CLASS_ID]: 2 },
    });
    render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={[]}
          onIssueClick={() => {}}
          qualityReport={qualityReport}
          scope={{ kind: "class", classId: CLASS_ID }}
        />,
      ),
    );
    expect(screen.getByText("Quality metrics")).toBeInTheDocument();
    expect(screen.getByText("3 class gap hours")).toBeInTheDocument();
    expect(screen.getByText("2 home-room misses")).toBeInTheDocument();
  });

  it("omits a row when its attribution value is 0 for this class", () => {
    const qualityReport = emptyQualityReport({
      class_gap_hours_by_class: { [CLASS_ID]: 1 },
      home_room_misses_by_class: {},
    });
    render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={[]}
          onIssueClick={() => {}}
          qualityReport={qualityReport}
          scope={{ kind: "class", classId: CLASS_ID }}
        />,
      ),
    );
    expect(screen.getByText("1 class gap hour")).toBeInTheDocument();
    expect(screen.queryByText(/home-room miss/i)).not.toBeInTheDocument();
  });

  it("omits the entire metrics section when all attribution values are 0 for this class", () => {
    const qualityReport = emptyQualityReport({
      class_gap_hours_by_class: {},
      home_room_misses_by_class: {},
    });
    render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={[]}
          onIssueClick={() => {}}
          qualityReport={qualityReport}
          scope={{ kind: "class", classId: CLASS_ID }}
        />,
      ),
    );
    expect(screen.queryByText("Quality metrics")).not.toBeInTheDocument();
  });

  it("pluralizes classGapHours via i18n (one vs other)", () => {
    const oneReport = emptyQualityReport({ class_gap_hours_by_class: { [CLASS_ID]: 1 } });
    const { rerender } = render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={[]}
          onIssueClick={() => {}}
          qualityReport={oneReport}
          scope={{ kind: "class", classId: CLASS_ID }}
        />,
      ),
    );
    expect(screen.getByText("1 class gap hour")).toBeInTheDocument();
    const twoReport = emptyQualityReport({ class_gap_hours_by_class: { [CLASS_ID]: 2 } });
    rerender(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={[]}
          onIssueClick={() => {}}
          qualityReport={twoReport}
          scope={{ kind: "class", classId: CLASS_ID }}
        />,
      ),
    );
    expect(screen.getByText("2 class gap hours")).toBeInTheDocument();
  });

  it("treats null qualityReport as no metrics section", () => {
    render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={[]}
          onIssueClick={() => {}}
          qualityReport={null}
          scope={{ kind: "class", classId: CLASS_ID }}
        />,
      ),
    );
    expect(screen.queryByText("Quality metrics")).not.toBeInTheDocument();
  });
});

const TEACHER_ID = "00000000-0000-0000-0000-000000000003";

describe("QualityIssueSidebar metrics section (teacher scope)", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders teacher_gap_hours via the teacher scope", () => {
    const qualityReport = emptyQualityReport({
      teacher_gap_hours_by_teacher: { [TEACHER_ID]: 4 },
    });
    render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={[]}
          onIssueClick={() => {}}
          qualityReport={qualityReport}
          scope={{ kind: "teacher", teacherId: TEACHER_ID }}
        />,
      ),
    );
    expect(screen.getByText("Quality metrics")).toBeInTheDocument();
    expect(screen.getByText("4 teacher gap hours")).toBeInTheDocument();
  });

  it("omits the section when teacher has no gap hours", () => {
    const qualityReport = emptyQualityReport({
      teacher_gap_hours_by_teacher: {},
    });
    render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={[]}
          onIssueClick={() => {}}
          qualityReport={qualityReport}
          scope={{ kind: "teacher", teacherId: TEACHER_ID }}
        />,
      ),
    );
    expect(screen.queryByText("Quality metrics")).not.toBeInTheDocument();
  });

  it("pluralizes teacherGapHours via i18n (one vs other)", () => {
    const oneReport = emptyQualityReport({
      teacher_gap_hours_by_teacher: { [TEACHER_ID]: 1 },
    });
    render(
      wrapWithI18n(
        <QualityIssueSidebar
          issues={[]}
          onIssueClick={() => {}}
          qualityReport={oneReport}
          scope={{ kind: "teacher", teacherId: TEACHER_ID }}
        />,
      ),
    );
    expect(screen.getByText("1 teacher gap hour")).toBeInTheDocument();
  });
});
