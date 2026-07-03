import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  loadLeagueSelectionData: vi.fn(),
  selectTeam: vi.fn(),
  navigate: vi.fn(),
  setGameState: vi.fn(),
  setGameActive: vi.fn(),
}));

vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>("react-router-dom");
  return {
    ...actual,
    useNavigate: () => mocks.navigate,
  };
});

vi.mock("react-i18next", () => ({
  initReactI18next: { type: "3rdParty", init: vi.fn() },
  useTranslation: () => ({
    i18n: { language: "en" },
    t: (_key: string, fallback?: string) => fallback ?? _key,
  }),
}));

vi.mock("@/store/gameStore", () => ({
  useGameStore: () => ({
    setGameState: mocks.setGameState,
    setGameActive: mocks.setGameActive,
  }),
}));

vi.mock("@/ui-v2/_legacy/components/teamSelection/teamSelection.helpers", () => ({
  loadLeagueSelectionData: mocks.loadLeagueSelectionData,
  selectTeam: mocks.selectTeam,
  getTeamLogoPath: () => "",
  getReputationLabel: () => ({ label: "Media", variant: "secondary" }),
  formatFinance: (value: number) => `€${value}`,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("TeamSelectionV2", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.loadLeagueSelectionData.mockResolvedValue({
      competitions: [
        {
          id: "lec",
          name: "LEC",
          region: "Europe",
          logo: null,
          tier: 1,
          legacy: false,
          active: true,
          team_count: 1,
          teams: [
            {
              id: "lec-g2",
              name: "G2 Esports",
              short_name: "G2",
              logo_url: null,
              country: "Germany",
              finance: 1000,
              reputation: 750,
              colors: null,
              ovr: null,
              player_count: 5,
            },
          ],
        },
      ],
    });
  });

  it("shows the backend error when team confirmation fails", async () => {
    mocks.selectTeam.mockRejectedValue(
      "No active competition manifests available; cannot select a team safely",
    );

    const TeamSelectionV2 = (await import("./TeamSelectionV2")).default;

    render(
      <MemoryRouter>
        <TeamSelectionV2 />
      </MemoryRouter>,
    );

    fireEvent.click(await screen.findByRole("button", { name: /LEC/i }));
    fireEvent.click(await screen.findByRole("button", { name: /G2 Esports/i }));
    fireEvent.click(screen.getByRole("button", { name: /Confirmar/i }));

    await waitFor(() => {
      expect(screen.getByText("Could not select team")).toBeInTheDocument();
    });
    expect(
      screen.getByText("No active competition manifests available; cannot select a team safely"),
    ).toBeInTheDocument();
    expect(mocks.navigate).not.toHaveBeenCalledWith("/dashboard");
  });
});
