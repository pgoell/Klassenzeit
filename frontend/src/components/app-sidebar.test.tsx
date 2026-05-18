import { screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { beforeAll, describe, expect, it } from "vitest";
import { AppSidebar } from "@/components/app-sidebar";
import { SidebarProvider } from "@/components/sidebar-provider";
import i18n from "@/i18n/init";
import { adminMe, server, superAdminMe } from "../../tests/msw-handlers";
import { renderWithProviders } from "../../tests/render-helpers";

const BASE = "http://localhost:3000";

function renderSidebar() {
  return renderWithProviders(
    <SidebarProvider>
      <AppSidebar />
    </SidebarProvider>,
  );
}

describe("AppSidebar", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders the user's school name next to the email", async () => {
    const { findByText } = renderSidebar();
    expect(await findByText("admin@example.com")).toBeInTheDocument();
    expect(await findByText("Default Schule")).toBeInTheDocument();
  });

  it("renders the Admin group with Schools for admin users", async () => {
    renderSidebar();
    await waitFor(() => {
      expect(screen.getByRole("link", { name: /schools/i })).toBeInTheDocument();
    });
  });

  it("hides the Admin group for non-admin users", async () => {
    server.use(
      http.get(`${BASE}/api/auth/me`, () => HttpResponse.json({ ...adminMe, role: "user" })),
    );
    renderSidebar();
    // Allow the sidebar to settle with the overridden me payload.
    await waitFor(() => {
      // Any element from the rendered sidebar; "Klassenzeit" is the brand text.
      expect(screen.getByText(/klassenzeit/i)).toBeInTheDocument();
    });
    expect(screen.queryByRole("link", { name: /schools/i })).not.toBeInTheDocument();
  });

  it("renders the Super Admin badge for super-admin users", async () => {
    server.use(http.get(`${BASE}/api/auth/me`, () => HttpResponse.json(superAdminMe)));
    renderSidebar();
    expect(await screen.findByText(/super admin/i)).toBeInTheDocument();
  });

  it("does not render the Super Admin badge for plain admin users", async () => {
    renderSidebar();
    // Wait for the sidebar to settle before asserting absence.
    await screen.findByText("admin@example.com");
    expect(screen.queryByText(/super admin/i)).not.toBeInTheDocument();
  });

  it("renders the Admin nav group for super-admin users", async () => {
    server.use(http.get(`${BASE}/api/auth/me`, () => HttpResponse.json(superAdminMe)));
    renderSidebar();
    await waitFor(() => {
      expect(screen.getByRole("link", { name: /schools/i })).toBeInTheDocument();
    });
  });
});
