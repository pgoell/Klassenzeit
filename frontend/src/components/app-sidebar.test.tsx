import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

  it("hides the Admin group for plain admin users (super-admin only after item 10h)", async () => {
    renderSidebar();
    // Settle on the brand text before asserting absence; the default adminMe handler returns role="admin".
    expect(await screen.findByText(/klassenzeit/i)).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /schools/i })).not.toBeInTheDocument();
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

describe("AppSidebar school picker", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders a static label when the user has only one accessible school", async () => {
    renderSidebar();
    expect(await screen.findByText("admin@example.com")).toBeInTheDocument();
    expect(await screen.findByText("Default Schule")).toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  });

  it("renders the select picker when the user has multiple accessible schools", async () => {
    server.use(
      http.get(`${BASE}/api/auth/me`, () =>
        HttpResponse.json({
          ...adminMe,
          accessible_schools: [
            { id: adminMe.school_id, name: adminMe.school_name },
            { id: "00000000-0000-0000-0000-000000000099", name: "Alt Schule" },
          ],
        }),
      ),
    );
    renderSidebar();
    const trigger = await screen.findByRole("combobox", { name: /active school/i });
    expect(trigger).toBeInTheDocument();
  });

  it("posts to the switch endpoint when selecting a different school", async () => {
    server.use(
      http.get(`${BASE}/api/auth/me`, () =>
        HttpResponse.json({
          ...adminMe,
          accessible_schools: [
            { id: adminMe.school_id, name: adminMe.school_name },
            { id: "00000000-0000-0000-0000-000000000099", name: "Alt Schule" },
          ],
        }),
      ),
    );
    let receivedBody: { school_id: string } | undefined;
    server.use(
      http.post(`${BASE}/api/auth/switch-school`, async ({ request }) => {
        receivedBody = (await request.json()) as { school_id: string };
        return HttpResponse.json({
          ...adminMe,
          active_school_id: receivedBody.school_id,
          active_school_name: "Alt Schule",
        });
      }),
    );

    renderSidebar();
    const trigger = await screen.findByRole("combobox", { name: /active school/i });
    await userEvent.click(trigger);
    const altOption = await screen.findByRole("option", { name: /alt schule/i });
    await userEvent.click(altOption);

    await waitFor(() => {
      expect(receivedBody).toEqual({
        school_id: "00000000-0000-0000-0000-000000000099",
      });
    });
  });
});
