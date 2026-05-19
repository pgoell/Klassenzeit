import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { AuditLogPage } from "@/features/audit-log/audit-log-page";
import i18n from "@/i18n/init";
import {
  auditLogDetailById,
  auditLogRows,
  BASE,
  resetAuditLogDetail,
  resetAuditLogRows,
  server,
  superAdminMe,
} from "./msw-handlers";
import { renderWithProviders } from "./render-helpers";

beforeEach(async () => {
  await i18n.changeLanguage("en");
  resetAuditLogRows();
  resetAuditLogDetail();
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

  it("renders a Details button per row", async () => {
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
    expect(await screen.findByRole("button", { name: /^details$/i })).toBeInTheDocument();
  });

  it("opens the detail dialog with request id, path params, and request body", async () => {
    const id = "11111111-1111-1111-1111-111111111111";
    auditLogRows.push({
      id,
      ts: "2026-05-01T12:00:00Z",
      actor_user_id: null,
      actor_user_email: "actor@example.com",
      target_school_id: null,
      target_school_name: "School Alpha",
      request_id: "req-xyz",
      method: "PATCH",
      route_template: "/api/schools/{school_id}",
      response_status: 200,
    });
    auditLogDetailById[id] = {
      id,
      ts: "2026-05-01T12:00:00Z",
      actor_user_id: null,
      actor_user_email: "actor@example.com",
      target_school_id: null,
      target_school_name: "School Alpha",
      request_id: "req-xyz",
      method: "PATCH",
      route_template: "/api/schools/{school_id}",
      response_status: 200,
      path_params: { school_id: "deleted" },
      request_body: { name: "Renamed" },
      request_body_truncated: false,
    };
    renderWithProviders(<AuditLogPage />);
    await userEvent.click(await screen.findByRole("button", { name: /^details$/i }));
    expect(await screen.findByText("req-xyz")).toBeInTheDocument();
    expect(screen.getByText(/"school_id": "deleted"/)).toBeInTheDocument();
    expect(screen.getByText(/"name": "Renamed"/)).toBeInTheDocument();
  });

  it("renders the truncated badge when request_body_truncated is true", async () => {
    const id = "44444444-4444-4444-4444-444444444444";
    auditLogRows.push({
      id,
      ts: "2026-05-01T12:00:00Z",
      actor_user_id: null,
      actor_user_email: "actor@example.com",
      target_school_id: null,
      target_school_name: null,
      request_id: null,
      method: "PATCH",
      route_template: "/api/x/{y}",
      response_status: 200,
    });
    auditLogDetailById[id] = {
      id,
      ts: "2026-05-01T12:00:00Z",
      actor_user_id: null,
      actor_user_email: "actor@example.com",
      target_school_id: null,
      target_school_name: null,
      request_id: null,
      method: "PATCH",
      route_template: "/api/x/{y}",
      response_status: 200,
      path_params: { y: "z" },
      request_body: { name: "x" },
      request_body_truncated: true,
    };
    renderWithProviders(<AuditLogPage />);
    await userEvent.click(await screen.findByRole("button", { name: /^details$/i }));
    expect(await screen.findByText(/truncated/i)).toBeInTheDocument();
  });

  it("renders the no-body placeholder when request_body is null", async () => {
    const id = "55555555-5555-5555-5555-555555555555";
    auditLogRows.push({
      id,
      ts: "2026-05-01T12:00:00Z",
      actor_user_id: null,
      actor_user_email: "actor@example.com",
      target_school_id: null,
      target_school_name: null,
      request_id: null,
      method: "DELETE",
      route_template: "/api/schools/{school_id}",
      response_status: 204,
    });
    auditLogDetailById[id] = {
      id,
      ts: "2026-05-01T12:00:00Z",
      actor_user_id: null,
      actor_user_email: "actor@example.com",
      target_school_id: null,
      target_school_name: null,
      request_id: null,
      method: "DELETE",
      route_template: "/api/schools/{school_id}",
      response_status: 204,
      path_params: { school_id: "abc" },
      request_body: null,
      request_body_truncated: false,
    };
    renderWithProviders(<AuditLogPage />);
    await userEvent.click(await screen.findByRole("button", { name: /^details$/i }));
    expect(await screen.findByText(/no body captured/i)).toBeInTheDocument();
  });
});
