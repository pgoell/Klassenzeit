import { beforeAll, describe, expect, it } from "vitest";
import { AppSidebar } from "@/components/app-sidebar";
import { SidebarProvider } from "@/components/sidebar-provider";
import i18n from "@/i18n/init";
import { renderWithProviders } from "../../tests/render-helpers";

describe("AppSidebar", () => {
  beforeAll(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders the user's school name next to the email", async () => {
    const { findByText } = renderWithProviders(
      <SidebarProvider>
        <AppSidebar />
      </SidebarProvider>,
    );
    expect(await findByText("admin@example.com")).toBeInTheDocument();
    expect(await findByText("Default Schule")).toBeInTheDocument();
  });
});
