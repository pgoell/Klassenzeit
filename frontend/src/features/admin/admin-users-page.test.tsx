import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeAll, describe, expect, it } from "vitest";
import i18n from "@/i18n/init";
import { adminUsers, BASE, server } from "../../../tests/msw-handlers";
import { AdminUsersPage } from "./admin-users-page";

function wrapAdminUsersPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <AdminUsersPage />
    </QueryClientProvider>,
  );
}

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

describe("AdminUsersPage", () => {
  it("renders the user list with role, active state, last login", async () => {
    wrapAdminUsersPage();
    expect(await screen.findByText("admin@example.com")).toBeInTheDocument();
    expect(screen.getByText("superadmin@example.com")).toBeInTheDocument();
    expect(screen.getAllByText(/yes/i).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/never/i).length).toBeGreaterThan(0);
  });

  it("opens the manage-schools dialog for the chosen row", async () => {
    const user = userEvent.setup();
    wrapAdminUsersPage();
    await screen.findByText("admin@example.com");
    const manageButtons = screen.getAllByRole("button", { name: /manage schools/i });
    const btn = manageButtons[0];
    if (!btn) throw new Error("expected at least one manage-schools button");
    await user.click(btn);
    await waitFor(() => {
      expect(screen.getByRole("dialog")).toBeInTheDocument();
    });
    expect(screen.getByText(/Manage schools for admin@example.com/i)).toBeInTheDocument();
  });

  it("shows the load error when the API returns 403", async () => {
    server.use(
      http.get(`${BASE}/api/auth/admin/users`, () =>
        HttpResponse.json({ detail: "forbidden" }, { status: 403 }),
      ),
    );
    wrapAdminUsersPage();
    expect(await screen.findByText(/could not load users/i)).toBeInTheDocument();
    expect(adminUsers.length).toBeGreaterThan(0);
  });
});
