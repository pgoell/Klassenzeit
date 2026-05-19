import { screen } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { AuditLogPage } from "@/features/audit-log/audit-log-page";
import i18n from "@/i18n/init";
import { auditLogRows, BASE, resetAuditLogRows, server, superAdminMe } from "./msw-handlers";
import { renderWithProviders } from "./render-helpers";

beforeEach(async () => {
  await i18n.changeLanguage("en");
  resetAuditLogRows();
  server.use(http.get(`${BASE}/api/auth/me`, () => HttpResponse.json(superAdminMe)));
});

describe("AuditLogPage", () => {
  it("renders the title and subtitle", async () => {
    renderWithProviders(<AuditLogPage />);
    expect(await screen.findByRole("heading", { name: /audit log/i })).toBeInTheDocument();
    expect(screen.getByText(/super-admin cross-school writes/i)).toBeInTheDocument();
  });

  it("renders rows from the query", async () => {
    auditLogRows.push({
      id: "11111111-1111-1111-1111-111111111111",
      ts: "2026-05-01T12:00:00Z",
      actor_user_id: "22222222-2222-2222-2222-222222222222",
      actor_user_email: "actor@example.com",
      target_school_id: "33333333-3333-3333-3333-333333333333",
      target_school_name: "School Alpha",
      request_id: null,
      method: "PATCH",
      route_template: "/api/schools/{school_id}",
      response_status: 200,
    });
    renderWithProviders(<AuditLogPage />);
    expect(await screen.findByText("actor@example.com")).toBeInTheDocument();
    expect(screen.getByText("School Alpha")).toBeInTheDocument();
    expect(screen.getByText("/api/schools/{school_id}")).toBeInTheDocument();
  });

  it("renders the empty-state copy when items is empty", async () => {
    renderWithProviders(<AuditLogPage />);
    expect(await screen.findByText(/no audit-log entries/i)).toBeInTheDocument();
  });

  it("handles null target_school_name without crashing", async () => {
    auditLogRows.push({
      id: "11111111-1111-1111-1111-111111111111",
      ts: "2026-05-01T12:00:00Z",
      actor_user_id: null,
      actor_user_email: "deleted-actor@example.com",
      target_school_id: null,
      target_school_name: null,
      request_id: null,
      method: "PATCH",
      route_template: "/api/schools/{school_id}",
      response_status: 200,
    });
    renderWithProviders(<AuditLogPage />);
    expect(await screen.findByText("deleted-actor@example.com")).toBeInTheDocument();
    expect(screen.getAllByText("—").length).toBeGreaterThan(0);
  });

  it("disables Previous on page 1", async () => {
    renderWithProviders(<AuditLogPage />);
    const prev = await screen.findByRole("button", { name: /previous/i });
    expect(prev).toBeDisabled();
  });
});
