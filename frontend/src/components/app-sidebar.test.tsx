import { screen, waitFor } from "@testing-library/react";
import { HttpResponse, http } from "msw";
import { beforeAll, describe, expect, it } from "vitest";
import { AppSidebar } from "@/components/app-sidebar";
import { SidebarProvider } from "@/components/sidebar-provider";
import i18n from "@/i18n/init";
import { adminMe, server } from "../../tests/msw-handlers";
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
});
