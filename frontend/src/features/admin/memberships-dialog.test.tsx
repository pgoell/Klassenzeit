import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { Toaster } from "@/components/ui/sonner";
import i18n from "@/i18n/init";
import { BASE, initialSchools, server, userMembershipsByUserId } from "../../../tests/msw-handlers";
import { MembershipsDialog } from "./memberships-dialog";
import type { AdminUser } from "./users-hooks";

const USER: AdminUser = {
  id: "10000000-0000-0000-0000-000000000010",
  email: "carla@example.com",
  role: "admin",
  is_active: true,
  last_login_at: null,
  school_id: "00000000-0000-0000-0000-000000000001",
  school_name: "Default Schule",
};

const SECOND_SCHOOL = initialSchools[1];
if (!SECOND_SCHOOL) throw new Error("seed missing: initialSchools[1]");

function wrapMembershipsDialog() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <Toaster />
      <MembershipsDialog user={USER} onClose={() => {}} />
    </QueryClientProvider>,
  );
}

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

describe("MembershipsDialog", () => {
  beforeEach(() => {
    userMembershipsByUserId[USER.id] = [];
  });

  it("renders the user's home school as a read-only row", async () => {
    wrapMembershipsDialog();
    expect(await screen.findByText(/Default Schule/i)).toBeInTheDocument();
    expect(screen.getByText(/home school/i)).toBeInTheDocument();
  });

  it("renders existing memberships from the API", async () => {
    userMembershipsByUserId[USER.id] = [
      { school_id: SECOND_SCHOOL.id, school_name: SECOND_SCHOOL.name },
    ];
    wrapMembershipsDialog();
    expect(await screen.findByText(SECOND_SCHOOL.name)).toBeInTheDocument();
  });

  it("grants a new membership", async () => {
    const user = userEvent.setup();
    wrapMembershipsDialog();
    await screen.findByText(/home school/i);
    await user.click(screen.getByRole("combobox", { name: /pick a school/i }));
    await user.click(await screen.findByRole("option", { name: SECOND_SCHOOL.name }));
    await user.click(screen.getByRole("button", { name: /^grant$/i }));
    expect(await screen.findByText(/granted access to/i)).toBeInTheDocument();
    expect(await screen.findByText(SECOND_SCHOOL.name)).toBeInTheDocument();
  });

  it("surfaces a code-specific toast on 409 membership_exists", async () => {
    server.use(
      http.post(`${BASE}/api/auth/admin/users/:user_id/memberships`, () =>
        HttpResponse.json({ detail: { code: "membership_exists" } }, { status: 409 }),
      ),
    );
    const user = userEvent.setup();
    wrapMembershipsDialog();
    await user.click(screen.getByRole("combobox", { name: /pick a school/i }));
    await user.click(await screen.findByRole("option", { name: SECOND_SCHOOL.name }));
    await user.click(screen.getByRole("button", { name: /^grant$/i }));
    expect(await screen.findByText(/already has access/i)).toBeInTheDocument();
  });

  it("surfaces a code-specific toast on 409 membership_redundant_home_school", async () => {
    server.use(
      http.post(`${BASE}/api/auth/admin/users/:user_id/memberships`, () =>
        HttpResponse.json(
          { detail: { code: "membership_redundant_home_school" } },
          { status: 409 },
        ),
      ),
    );
    const user = userEvent.setup();
    wrapMembershipsDialog();
    await user.click(screen.getByRole("combobox", { name: /pick a school/i }));
    await user.click(await screen.findByRole("option", { name: SECOND_SCHOOL.name }));
    await user.click(screen.getByRole("button", { name: /^grant$/i }));
    expect(await screen.findByText(/already the home school/i)).toBeInTheDocument();
  });

  it("revokes a membership through the confirm dialog", async () => {
    userMembershipsByUserId[USER.id] = [
      { school_id: SECOND_SCHOOL.id, school_name: SECOND_SCHOOL.name },
    ];
    const user = userEvent.setup();
    wrapMembershipsDialog();
    const row = await screen.findByText(SECOND_SCHOOL.name);
    const removeButton = within(row.closest("li") as HTMLElement).getByRole("button", {
      name: /remove/i,
    });
    await user.click(removeButton);
    const confirmDialog = await screen.findByRole("dialog", {
      name: /remove school membership/i,
    });
    await user.click(within(confirmDialog).getByRole("button", { name: /^remove$/i }));
    await waitFor(() => {
      expect(screen.queryByText(SECOND_SCHOOL.name)).not.toBeInTheDocument();
    });
    expect(screen.getByText(/sessions invalidated/i)).toBeInTheDocument();
  });

  it("cancels the revoke flow without firing DELETE", async () => {
    userMembershipsByUserId[USER.id] = [
      { school_id: SECOND_SCHOOL.id, school_name: SECOND_SCHOOL.name },
    ];
    const user = userEvent.setup();
    wrapMembershipsDialog();
    const row = await screen.findByText(SECOND_SCHOOL.name);
    const removeButton = within(row.closest("li") as HTMLElement).getByRole("button", {
      name: /remove/i,
    });
    await user.click(removeButton);
    const confirmDialog = await screen.findByRole("dialog", {
      name: /remove school membership/i,
    });
    await user.click(within(confirmDialog).getByRole("button", { name: /^cancel$/i }));
    expect(screen.getByText(SECOND_SCHOOL.name)).toBeInTheDocument();
  });
});
