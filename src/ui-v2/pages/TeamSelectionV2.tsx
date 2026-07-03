import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, Loader2 } from "lucide-react";

import type { GameStateData, LeagueSelectionData } from "@/store/gameStore";
import { useGameStore } from "@/store/gameStore";
import { LeaguePickerV2 } from "@/ui-v2/_legacy/components/teamSelection/LeaguePickerV2";
import { TeamGridV2 } from "@/ui-v2/_legacy/components/teamSelection/TeamGridV2";
import { loadLeagueSelectionData, selectTeam } from "@/ui-v2/_legacy/components/teamSelection/teamSelection.helpers";
import { resolvePlayerLolRole } from "@/lib/players/lolIdentity";
import { calculateLolOvr } from "@/lib/players/lolPlayerStats";

type Screen = "loading" | "error" | "league" | "teams";

function formatActionError(error: unknown): string {
  if (error instanceof Error) {
    return [error.message, error.stack].filter(Boolean).join("\n\n");
  }

  if (typeof error === "string") {
    return error;
  }

  try {
    return JSON.stringify(error, null, 2);
  } catch {
    return String(error);
  }
}

export default function TeamSelectionV2() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const { setGameState, setGameActive } = useGameStore();

  const [screen, setScreen] = useState<Screen>("loading");
  const [error, setError] = useState<string | null>(null);
  const [leagueData, setLeagueData] = useState<LeagueSelectionData | null>(null);
  const [selectedCompetitionId, setSelectedCompetitionId] = useState<string | null>(null);
  const [selectedTeamId, setSelectedTeamId] = useState<string | null>(null);
  const [isConfirming, setIsConfirming] = useState(false);
  const [confirmError, setConfirmError] = useState<string | null>(null);

  useEffect(() => {
    loadLeagueSelectionData()
      .then((data) => {
        console.log("[LeagueDebug] RAW from backend:", data.competitions.map(c => ({ id: c.id, name: c.name, legacy: c.legacy, tier: c.tier, team_count: c.team_count })));
        setLeagueData(data);
        setScreen(data.competitions.length > 0 ? "league" : "error");
      })
      .catch((err) => {
        console.error("Failed to load league data:", err);
        setError(String(err));
        setScreen("error");
      });
  }, []);

  const activeCompetitions = useMemo(() => {
    const filtered = (leagueData?.competitions ?? []).filter((c) => c.active);
    console.log("[LeagueDebug] FILTERED:", filtered.map(c => ({ id: c.id, name: c.name, legacy: c.legacy })));
    return filtered;
  }, [leagueData]);

  const handleLeagueSelect = (id: string) => {
    setSelectedCompetitionId(id);
    setSelectedTeamId(null);
    setConfirmError(null);
    setScreen("teams");
  };

  const handleBackToLeagues = () => {
    setSelectedCompetitionId(null);
    setSelectedTeamId(null);
    setConfirmError(null);
    setScreen("league");
  };

  const handleBackToMenu = () => navigate("/");

  const handleConfirm = async () => {
    if (!selectedTeamId || isConfirming) {
      if (!selectedTeamId) setConfirmError("Select a team before confirming.");
      return;
    }
    setIsConfirming(true);
    setConfirmError(null);
    try {
      let updatedGame = await selectTeam(selectedTeamId, i18n.language);
      const myTeam = updatedGame.teams.find((t: any) => t.id === selectedTeamId);
      if (myTeam) {
        const roster = updatedGame.players.filter((p: any) => p.team_id === myTeam.id);
        const roles = ["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"] as const;
        const used = new Set<string>();
        const lineup = roles.map((role) => {
          const candidates = roster
            .filter((p: any) => !used.has(p.id) && resolvePlayerLolRole(p) === role)
            .sort((a: any, b: any) => calculateLolOvr(b) - calculateLolOvr(a));
          const best = candidates[0];
          if (best) used.add(best.id);
          return best?.id ?? "";
        });
        if (lineup.every(Boolean)) {
          try {
            updatedGame = await invoke<GameStateData>("set_active_lineup", { playerIds: lineup });
          } catch (e) {
            console.error("[TeamSelection] set_active_lineup failed:", e);
          }
        }
      }
      setGameState(updatedGame);
      const mgr = updatedGame.manager;
      const displayName = mgr.nickname?.trim() || `${mgr.first_name} ${mgr.last_name}`;
      setGameActive(true, displayName);
      navigate("/dashboard");
    } catch (err) {
      console.error("Failed to select team:", err);
      setConfirmError(formatActionError(err));
    } finally {
      setIsConfirming(false);
    }
  };

  if (screen === "loading") {
    return (
      <div className="flex h-full items-center justify-center bg-background">
        <div className="text-center">
          <Loader2 className="mx-auto mb-3 size-8 animate-spin text-primary" />
          <p className="text-sm text-muted-foreground">{t("worldSelect.creatingWorld")}</p>
        </div>
      </div>
    );
  }

  if (screen === "error") {
    return (
      <div className="flex h-full items-center justify-center bg-background">
        <div className="mx-auto max-w-md p-8 text-center">
          <p className="mb-4 text-sm text-red-400">{error || t("teamSelect.noLeaguesDesc")}</p>
          <button type="button" onClick={handleBackToMenu}
            className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground"
          >
            {t("common.backToMenu", "Back to menu")}
          </button>
        </div>
      </div>
    );
  }

  const selectedCompetition = activeCompetitions.find((c) => c.id === selectedCompetitionId);
  const isLeagueScreen = screen === "league" && leagueData;

  const handleRootKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      if (isLeagueScreen) handleBackToMenu();
      else handleBackToLeagues();
    }
  };

  return (
    <div className="flex h-full flex-col bg-background" onKeyDown={handleRootKeyDown} tabIndex={-1}>
      <header className="relative flex h-14 shrink-0 items-center border-b border-border bg-gradient-to-r from-primary/5 to-transparent px-6">
        <button type="button" onClick={isLeagueScreen ? handleBackToMenu : handleBackToLeagues}
          className="flex size-9 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        >
          <ArrowLeft className="size-4" />
        </button>
        <div className="ml-3 flex flex-1 items-center justify-between">
          <div>
            <h1 className="font-heading text-lg font-black uppercase tracking-widest text-foreground">
              {isLeagueScreen ? t("teamSelect.selectLeague", "Select League") : selectedCompetition?.name ?? ""}
            </h1>
            <p className="text-xs text-muted-foreground/70">
              {isLeagueScreen ? t("teamSelect.selectLeagueSubtitle", "Choose a competition") : t("teamSelect.selectTeam", "Elige un equipo")}
            </p>
          </div>
          {!isLeagueScreen && selectedTeamId && (
            <button type="button" disabled={isConfirming} onClick={handleConfirm}
              className="flex h-8 items-center gap-2 rounded-lg bg-primary px-4 text-xs font-bold uppercase tracking-wider text-primary-foreground shadow-lg shadow-primary/20 transition-all hover:bg-primary/90 hover:shadow-xl hover:shadow-primary/30 disabled:pointer-events-none disabled:opacity-50"
            >
              {isConfirming && <Loader2 className="size-4 animate-spin" />}
              {t("teamSelect.confirm", "Confirmar")}
            </button>
          )}
        </div>
      </header>

      {isLeagueScreen ? (
        <LeaguePickerV2 competitions={activeCompetitions} onSelect={handleLeagueSelect} />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col">
          {confirmError && (
            <div className="mx-6 mt-4 rounded-lg border border-destructive/40 bg-destructive/10 p-4 text-sm text-destructive md:mx-8">
              <p className="font-heading text-sm font-bold uppercase tracking-wide">
                {t("teamSelect.selectTeamErrorTitle", "Could not select team")}
              </p>
              <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-words rounded-md bg-background/70 p-3 font-mono text-xs leading-relaxed text-destructive">
                {confirmError}
              </pre>
            </div>
          )}
          <TeamGridV2 teams={selectedCompetition?.teams ?? []} onSelectTeam={(id) => {
            setSelectedTeamId(id);
            setConfirmError(null);
          }} selectedTeamId={selectedTeamId} />
        </div>
      )}
    </div>
  );
}
