import { useCallback, useEffect, useMemo, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { Eye, Gamepad2 } from "lucide-react";

import { useGameStore } from "@/store/gameStore";
import type { GameStateData, PlayerSelectionOptions } from "@/store/gameStore";
import { useSettingsStore } from "@/store/settingsStore";
import { useBugReportStore } from "@/store/bugReportStore";
import { useAdvanceTime, type MatchModeType } from "@/hooks/useAdvanceTime";
import { resolveTeamLogo } from "@/lib/teams/teamLogos";
import { isAcademyTeam } from "@/store/academySelectors";
import TeamProfileV2 from "@/ui-v2/pages/TeamProfileV2";
import { assetUrl } from "@/lib/assetUrl";
import { DEFAULT_MANAGER_ICON_PATH } from "@/lib/common/managerAvatars";

import {
  formatDateFull,
  isSeasonComplete as isLeagueSeasonComplete,
} from "@/lib/common/helpers";
import DashboardWorkspaceContent from "@/ui-v2/_legacy/components/dashboard/DashboardWorkspaceContent";
import DashboardOverlays from "@/ui-v2/_legacy/components/dashboard/DashboardOverlays";
import FiredModal from "@/ui-v2/_legacy/components/dashboard/FiredModal";
import {
  createDashboardProfileNavigationState,
  goBackDashboardProfile,
  hasDashboardProfileHistory,
  navigateDashboardProfiles,
  selectDashboardPlayer,
  selectDashboardTeam,
  selectDashboardStaff,
  type DashboardNavigateContext,
} from "@/lib/dashboard/profileNavigation";
import { createDashboardTabContentModel } from "@/lib/dashboard/tabContentModel";
import {
  getDashboardAlerts,
  getManagerTeamName,
  getTodayMatchFixture,
  getUnreadMessagesCount,
} from "@/lib/dashboard/helpers";
import type { DashboardMatchModeMeta } from "@/ui-v2/_legacy/components/dashboard/DashboardHeader";

import { DashboardSidebarV2 } from "./DashboardSidebarV2";
import { DashboardHeaderV2 } from "./DashboardHeaderV2";
import { HomeTabV2 } from "./tabs/HomeTabV2";
import { InboxTabV2 } from "./tabs/InboxTabV2";
import { ScheduleTabV2 } from "./tabs/ScheduleTabV2";
import { SquadTabV2 } from "./tabs/SquadTabV2";
import { TacticsTabV2 } from "./tabs/TacticsTabV2";
import { TrainingTabV2 } from "./tabs/TrainingTabV2";
import { PlayersTabV2 } from "./tabs/PlayersTabV2";
import { TeamsTabV2 } from "./tabs/TeamsTabV2";
import { StaffTabV2 } from "./tabs/StaffTabV2";
import { FinancesTabV2 } from "./tabs/FinancesTabV2";
import { ScrimsTabV2 } from "./tabs/ScrimsTabV2";
import { SoloqTabV2 } from "./tabs/SoloqTabV2";
import { ScoutingTabV2 } from "./tabs/ScoutingTabV2";
import { TransfersTabV2 } from "./tabs/TransfersTabV2";
import { NewsTabV2 } from "./tabs/NewsTabV2";
import { SocialTabV2 } from "./tabs/SocialTabV2";

import { ManagerTabV2 } from "./tabs/ManagerTabV2";
import { YouthTabV2 } from "./tabs/YouthTabV2";
import { CompetitionsTabV2 } from "./tabs/CompetitionsTabV2";
import { MarketTabV2 } from "./tabs/MarketTabV2";
import { ChampionsWorldTabV2 } from "./tabs/ChampionsWorldTabV2";
import { MetaTabV2 } from "./tabs/MetaTabV2";
import ChampionPageV2 from "@/ui-v2/pages/ChampionPageV2";
import PlayerProfileV2 from "@/ui-v2/pages/PlayerProfileV2";
import StaffProfileV2 from "@/ui-v2/pages/StaffProfileV2";

const TAB_TRANSLATION_KEYS: Record<string, string> = {
  Home: "dashboard.home",
  Inbox: "dashboard.inbox",
  Manager: "dashboard.manager",
  Squad: "dashboard.squad",
  Tactics: "dashboard.tactics",
  Training: "dashboard.training",
  Scrims: "dashboard.scrims",
  Soloq: "dashboard.soloq",
  Meta: "dashboard.meta",
  Staff: "dashboard.staff",
  Finances: "dashboard.finances",
  Competitions: "dashboard.competitions",
  Tournaments: "dashboard.tournaments",
  Market: "dashboard.market",
  Transfers: "dashboard.transfers",
  Players: "dashboard.players",
  Teams: "dashboard.teams",
  WorldStaff: "dashboard.worldStaff",

  ChampionsWorld: "dashboard.champions_world",
  Schedule: "dashboard.schedule",
  News: "dashboard.news",
  Social: "dashboard.social",
  Scouting: "dashboard.scouting",
  Youth: "dashboard.youthAcademy",
};

const PATH_TAB_MAP: Record<string, string> = {
  "/finanzas": "Finances",
  "/finances": "Finances",
  "/competiciones": "Competitions",
  "/competitions": "Competitions",
};

/// Maps internal dashboard tab names to Discord Rich Presence state keys.
/// Each distinct activity gets its own key for richer presence data.
const TAB_TO_DISCORD_KEY: Record<string, string> = {
  Home: "dashboard",
  Inbox: "inbox",
  News: "news",
  Social: "social",
  Manager: "dashboard",
  Squad: "squad",
  Tactics: "tactics",
  Training: "training",
  Scrims: "scrims",
  Players: "players",
  Teams: "teams",
  Staff: "staff",
  WorldStaff: "staff",
  Scouting: "scouting",
  Youth: "youth",
  Transfers: "transfers",
  Market: "market",
  Finances: "finances",
  Competitions: "competitions",
  Tournaments: "competitions",
  Schedule: "competitions",
  Meta: "dashboard",
  ChampionsWorld: "dashboard",
};

export default function DashboardV2() {
  const navigate = useNavigate();
  const location = useLocation();
  const { t } = useTranslation();
  const {
    hasActiveGame,
    managerName,
    gameState,
    setGameState,
    setGameActive,
    clearGame,
    markClean,
  } = useGameStore();
  const [probedNoGame, setProbedNoGame] = useState(false);
  const { settings, loaded: settingsLoaded, loadSettings } = useSettingsStore();

  const [isSaving, setIsSaving] = useState(false);
  const [saveFlash, setSaveFlash] = useState(false);
  const [profileNavigation, setProfileNavigation] = useState(() =>
    createDashboardProfileNavigationState(PATH_TAB_MAP[location.pathname] ?? "Home"),
  );
  const [showExitConfirm, setShowExitConfirm] = useState(false);
  const [isExitingToMenu, setIsExitingToMenu] = useState(false);
  const [showCloseConfirm, setShowCloseConfirm] = useState(false);
  const [viewingChampionKey, setViewingChampionKey] = useState<string | null>(null);

  useEffect(() => {
    if (!settingsLoaded) loadSettings();
  }, [settingsLoaded, loadSettings]);

  // Block browser back from leaving the dashboard
  useEffect(() => {
    const handlePopState = () => {
      navigate("/dashboard", { replace: true });
    };
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, [navigate]);

  useEffect(() => {
    const tab = PATH_TAB_MAP[location.pathname];
    if (!tab) {
      return;
    }
    setProfileNavigation((s) =>
      s.activeTab === tab ? s : navigateDashboardProfiles(s, tab),
    );
  }, [location.pathname]);

  // Update Discord Rich Presence when switching dashboard tabs.
  useEffect(() => {
    const activeTab = profileNavigation.activeTab;
    const stateKey = TAB_TO_DISCORD_KEY[activeTab] ?? "dashboard";
    invoke("update_discord_presence", { stateKey }).catch(() => {
      // Silently ignore — Discord may not be available.
    });
  }, [profileNavigation.activeTab]);

  // Update bug-report context so reports know the actual dashboard tab.
  const setCurrentDashboardTab = useBugReportStore((s) => s.setCurrentDashboardTab);
  useEffect(() => {
    setCurrentDashboardTab(profileNavigation.activeTab);
  }, [profileNavigation.activeTab, setCurrentDashboardTab]);

  useEffect(() => {
    if (hasActiveGame) {
      invoke<GameStateData>("get_active_game")
        .then(setGameState)
        .catch((err) => console.error("Failed to fetch game state:", err));
      return;
    }
    invoke<GameStateData | null>("get_active_game")
      .then((state) => {
        if (state) {
          const name =
            state.manager.nickname?.trim() ||
            `${state.manager.first_name} ${state.manager.last_name}`;
          setGameActive(true, name);
          setGameState(state);
        } else {
          setProbedNoGame(true);
        }
      })
      .catch(() => setProbedNoGame(true));
  }, [hasActiveGame, setGameState, setGameActive]);

  const [championLoadError, setChampionLoadError] = useState(false);

  // Load champions once game state is available
  useEffect(() => {
    if (!gameState) return;
    if (gameState.champions) return;

    const loadChampions = async () => {
      try {
        const champions = await invoke<unknown[]>("get_champions");
        useGameStore.getState().setGameState({ ...gameState, champions } as GameStateData);
        setChampionLoadError(false);
      } catch (err) {
        console.error("[DashboardV2] Failed to load champions:", err);
        setChampionLoadError(true);
      }
    };
    loadChampions();
  }, [gameState]);

  const isUnemployed = gameState?.manager.team_id === null;
  // World (MUNDO) sidebar badges: count the real competitive world only,
  // excluding seeded youth-academy teams and their generated players (which the
  // Players/Teams tabs already filter out). Keeps the badges consistent with
  // the lists and with the imported catalog totals.
  const worldCounts = useMemo(() => {
    if (!gameState) return { players: 0, teams: 0, staff: 0 };
    const academyTeamIds = new Set(
      gameState.teams.filter(isAcademyTeam).map((team) => team.id),
    );
    return {
      players: gameState.players.filter(
        (player) => !player.team_id || !academyTeamIds.has(player.team_id),
      ).length,
      teams: gameState.teams.filter((team) => !isAcademyTeam(team)).length,
      staff: gameState.staff.length,
    };
  }, [gameState]);
  const todayMatchFixture = gameState ? getTodayMatchFixture(gameState) : null;
  const hasMatchToday = todayMatchFixture !== null;
  const activeLeague = gameState?.user_competition_id
    ? gameState.leagues.find(
        (league) => league.competition_id === gameState.user_competition_id,
      ) ?? gameState.leagues[0]
    : gameState?.leagues[0];
  const seasonComplete = isLeagueSeasonComplete(activeLeague);

  const {
    isAdvancing,
    showMatchConfirm,
    setShowMatchConfirm,
    matchMode,
    blockerModal,
    setBlockerModal,
    autoDelegationNotice,
    handleContinue,
    handleConfirmMatch,
    handleSkipToMatchDay,
    handleSkipToNextDay,
  } = useAdvanceTime(
    setGameState,
    hasMatchToday,
    settings.default_match_mode,
    settings.scrim_review_mode,
    settingsLoaded,
    isUnemployed ?? false,
  );

  const handleSave = useCallback(async () => {
    setIsSaving(true);
    try {
      await invoke("save_game");
      markClean();
      setSaveFlash(true);
      setTimeout(() => setSaveFlash(false), 2000);
    } catch (err) {
      console.error("Failed to save:", err);
    } finally {
      setIsSaving(false);
    }
  }, [markClean]);

  const handleExitToMenu = async () => {
    if (isExitingToMenu) return;
    setIsExitingToMenu(true);
    try {
      await invoke("exit_to_menu");
    } catch (err) {
      console.error("Failed to exit:", err);
    } finally {
      clearGame();
      navigate("/");
    }
  };

  const handleExitWithoutSave = () => {
    clearGame();
    navigate("/");
  };

  const handleCloseQuit = async (save: boolean) => {
    setShowCloseConfirm(false);
    if (save) {
      try {
        await invoke("save_game");
        markClean();
      } catch (err) {
        console.error("Auto-save on close failed:", err);
      }
    }
  };

  const handleNavClick = (tab: string) => {
    setViewingChampionKey(null);
    setProfileNavigation((s) => navigateDashboardProfiles(s, tab));
  };

  const handleNavigate = (tab: string, context?: DashboardNavigateContext) => {
    setViewingChampionKey(null);
    setProfileNavigation((s) => navigateDashboardProfiles(s, tab, context));
  };

  const handleBack = () => setProfileNavigation((s) => goBackDashboardProfile(s));

  const selectPlayer = (id: string, options?: PlayerSelectionOptions) =>
    setProfileNavigation((s) => selectDashboardPlayer(s, id, options));

  const selectTeam = (id: string) =>
    setProfileNavigation((s) => selectDashboardTeam(s, id));

  const selectStaff = (id: string) =>
    setProfileNavigation((s) => selectDashboardStaff(s, id));

  const MODE_META: Record<MatchModeType, DashboardMatchModeMeta> = useMemo(
    () => ({
      live: {
        label: t("continueMenu.goToField"),
        icon: <Gamepad2 className="w-4 h-4" />,
        desc: t("continueMenu.goToFieldDesc"),
        buttonColorClass: "from-primary-500 to-primary-600",
        dropdownColorClass: "from-primary-600 to-primary-700",
      },
      spectator: {
        label: t("continueMenu.watchSpectator"),
        icon: <Eye className="w-4 h-4" />,
        desc: t("continueMenu.watchSpectatorDesc"),
        buttonColorClass: "from-indigo-500 to-indigo-600",
        dropdownColorClass: "from-indigo-600 to-indigo-700",
      },
      delegate: {
        label: t("continueMenu.watchSpectator"),
        icon: <Eye className="w-4 h-4" />,
        desc: t("continueMenu.watchSpectatorDesc"),
        buttonColorClass: "from-indigo-500 to-indigo-600",
        dropdownColorClass: "from-indigo-600 to-indigo-700",
      },
    }),
    [t],
  );
  const currentModeMeta = MODE_META[matchMode];

  const currentDate = gameState
    ? formatDateFull(gameState.clock.current_date, settings.language)
    : "";
  const unreadMessagesCount = gameState ? getUnreadMessagesCount(gameState) : 0;
  const myTeamName = gameState ? getManagerTeamName(gameState) : null;
  const myTeam = gameState?.teams.find((t) => t.id === gameState?.manager.team_id);
  const liveManagerName = gameState
    ? gameState.manager.nickname?.trim() ||
      `${gameState.manager.first_name} ${gameState.manager.last_name}`
    : managerName;
  const managerAvatar = useMemo(() => gameState?.manager?.avatar_path ? assetUrl(gameState.manager.avatar_path) : assetUrl(DEFAULT_MANAGER_ICON_PATH), [gameState?.manager?.avatar_path]);
  const teamLogo = useMemo(() => resolveTeamLogo(myTeamName, myTeam?.logo_url), [myTeamName, myTeam?.logo_url]);
  const hasProfileHistory = hasDashboardProfileHistory(profileNavigation);
  const activeTabLabel = TAB_TRANSLATION_KEYS[profileNavigation.activeTab]
    ? t(TAB_TRANSLATION_KEYS[profileNavigation.activeTab], { defaultValue: profileNavigation.activeTab })
    : profileNavigation.activeTab;

  const dashboardAlerts = gameState
    ? getDashboardAlerts(gameState, hasMatchToday, t)
    : [];
  if (autoDelegationNotice) {
    dashboardAlerts.unshift({
      id: "scrim_auto_delegate_notice",
      text: autoDelegationNotice,
      tab: "Scrims",
      severity: "info",
    });
  }

  const dashboardTabContentModel = gameState
    ? createDashboardTabContentModel({
        activeTab: profileNavigation.activeTab,
        gameState,
        seasonComplete,
        visitedOnboardingTabs: new Set<string>(),
        initialMessageId: profileNavigation.initialMessageId,
        handlers: {
          onSelectPlayer: selectPlayer,
          onSelectTeam: selectTeam,
          onGameUpdate: setGameState,
          onNavigate: handleNavigate,
          onViewChampion: (k) => setViewingChampionKey(k),
        },
      })
    : null;

  if (!hasActiveGame && probedNoGame) {
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-4 bg-background text-foreground">
        <div className="text-lg font-medium">{t("dashboard.noActiveGame")}</div>
        <p className="text-sm text-muted-foreground">
          {t("dashboard.noActiveGameDesc")}
        </p>
      </div>
    );
  }

  if (!gameState || !dashboardTabContentModel) {
    return (
      <div className="flex h-screen items-center justify-center bg-background text-muted-foreground">
        <div className="size-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 overflow-hidden bg-background text-foreground">
      <DashboardSidebarV2
        activeTab={profileNavigation.activeTab}
        onNavClick={handleNavClick}
        unreadMessagesCount={unreadMessagesCount}
        managerFullName={`${gameState?.manager.first_name ?? ""} ${gameState?.manager.last_name ?? ""}`.trim() || null}
        managerName={liveManagerName}
        managerAvatar={managerAvatar}
        teamName={myTeamName}
        teamLogo={teamLogo}
        isUnemployed={isUnemployed ?? false}
        playerCount={worldCounts.players}
        teamCount={worldCounts.teams}
        staffCount={worldCounts.staff}
        onNavigateSettings={() => navigate("/settings", { state: { from: "/dashboard" } })}
        onExitClick={() => !isExitingToMenu && setShowExitConfirm(true)}
      />

      <DashboardOverlays
        blockerModal={blockerModal}
        currentModeMeta={currentModeMeta}
        handleConfirmMatch={handleConfirmMatch}
        handleExitToMenu={handleExitToMenu}
        handleExitWithoutSave={handleExitWithoutSave}
        handleNavigate={handleNavigate}
        handleCloseQuit={handleCloseQuit}
        isExitingToMenu={isExitingToMenu}
        matchMode={matchMode}
        setBlockerModal={setBlockerModal}
        setShowCloseConfirm={setShowCloseConfirm}
        setShowExitConfirm={setShowExitConfirm}
        setShowMatchConfirm={setShowMatchConfirm}
        showCloseConfirm={showCloseConfirm}
        showExitConfirm={showExitConfirm}
        showMatchConfirm={showMatchConfirm}
        teams={gameState.teams}
        todayMatchFixture={todayMatchFixture}
      />
      <FiredModal />

      <main className="flex flex-1 flex-col overflow-hidden scrollbar-v2">
        <DashboardHeaderV2
          activeTabLabel={activeTabLabel}
          currentDate={currentDate}
          hasProfileHistory={hasProfileHistory}
          isAdvancing={isAdvancing}
          isSaving={isSaving}
          saveFlash={saveFlash}
          hasMatchToday={hasMatchToday}
          dayPhase={gameState?.day_phase ?? "Morning"}
          alerts={dashboardAlerts}
          onBack={handleBack}
          onSave={handleSave}
          onContinue={handleContinue}
          onSkipToMatchDay={handleSkipToMatchDay}
          onSkipToNextDay={handleSkipToNextDay}
          onNavigate={handleNavigate}
        />

        {profileNavigation.activeTab === "Home" &&
        !viewingChampionKey &&
        !profileNavigation.selectedPlayerId &&
        !profileNavigation.selectedTeamId &&
        !seasonComplete ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <HomeTabV2 gameState={gameState} onNavigate={handleNavigate} onSelectPlayer={selectPlayer} />
          </div>
        ) : profileNavigation.activeTab === "Inbox" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-hidden">
            <InboxTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
              initialMessageId={profileNavigation.initialMessageId ?? null}
              onNavigate={handleNavigate}
            />
          </div>
        ) : profileNavigation.activeTab === "Schedule" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <ScheduleTabV2 gameState={gameState} onSelectTeam={selectTeam} />
          </div>
        ) : profileNavigation.activeTab === "Squad" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <SquadTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
              onSelectPlayer={selectPlayer}
            />
          </div>
        ) : profileNavigation.activeTab === "Training" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <TrainingTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
              onSelectPlayer={selectPlayer}
            />
          </div>
        ) : profileNavigation.activeTab === "Tactics" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <TacticsTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
            />
          </div>
        ) : profileNavigation.activeTab === "Players" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <PlayersTabV2
              gameState={gameState}
              onSelectPlayer={selectPlayer}
              onSelectTeam={selectTeam}
            />
          </div>
        ) : profileNavigation.activeTab === "Teams" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <TeamsTabV2
              gameState={gameState}
              onSelectTeam={selectTeam}
            />
          </div>
        ) : profileNavigation.activeTab === "Staff" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId &&
          !profileNavigation.selectedStaffId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <StaffTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
              onSelectStaff={selectStaff}
            />
          </div>
        ) : profileNavigation.activeTab === "WorldStaff" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId &&
          !profileNavigation.selectedStaffId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <StaffTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
              mode="world"
              onSelectStaff={selectStaff}
            />
          </div>
        ) : profileNavigation.activeTab === "Finances" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <FinancesTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
              onSelectPlayer={selectPlayer}
            />
          </div>
        ) : profileNavigation.activeTab === "Scrims" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <ScrimsTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
            />
          </div>
        ) : profileNavigation.activeTab === "Soloq" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <SoloqTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
              onSelectPlayer={selectPlayer}
            />
          </div>
        ) : profileNavigation.activeTab === "Scouting" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <ScoutingTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
              onSelectPlayer={selectPlayer}
              onNavigate={handleNavigate}
            />
          </div>
        ) : profileNavigation.activeTab === "Transfers" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <TransfersTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
              onSelectPlayer={selectPlayer}
              onSelectTeam={selectTeam}
            />
          </div>
        ) : profileNavigation.activeTab === "News" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <NewsTabV2 gameState={gameState} />
          </div>
        ) : profileNavigation.activeTab === "Social" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <SocialTabV2
              gameState={gameState}
              onGameUpdate={setGameState}
            />
          </div>
        ) : profileNavigation.activeTab === "Manager" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <ManagerTabV2 gameState={gameState} />
          </div>
        ) : profileNavigation.activeTab === "Youth" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <YouthTabV2
              gameState={gameState}
              onSelectPlayer={selectPlayer}
              onSelectTeam={selectTeam}
              onGameUpdate={setGameState}
            />
          </div>
        ) : profileNavigation.activeTab === "Competitions" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <CompetitionsTabV2
              gameState={gameState}
              onSelectTeam={selectTeam}
            />
          </div>
        ) : profileNavigation.activeTab === "Market" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <MarketTabV2 gameState={gameState} />
          </div>
        ) : profileNavigation.activeTab === "ChampionsWorld" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <ChampionsWorldTabV2
              champions={gameState.champions}
              onViewChampion={(k) => setViewingChampionKey(k)}
            />
          </div>
        ) : profileNavigation.activeTab === "Meta" &&
          !viewingChampionKey &&
          !profileNavigation.selectedPlayerId &&
          !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-y-auto scrollbar-v2">
            <MetaTabV2
              gameState={gameState}
              onViewChampion={(k) => setViewingChampionKey(k)}
              onGameUpdate={setGameState}
            />
          </div>
        ) : profileNavigation.selectedPlayerId && !viewingChampionKey && !profileNavigation.selectedTeamId ? (
          <div className="flex-1 overflow-hidden">
            <PlayerProfileV2
              gameState={gameState}
              playerId={profileNavigation.selectedPlayerId}
              onClose={() =>
                setProfileNavigation((s) => goBackDashboardProfile(s))
              }
              onGameUpdate={setGameState}
              onSelectPlayer={selectPlayer}
              onSelectTeam={selectTeam}
              onViewChampion={(k) => setViewingChampionKey(k)}
            />
          </div>
        ) : profileNavigation.selectedTeamId && !viewingChampionKey && !profileNavigation.selectedPlayerId ? (
          <div className="flex-1 overflow-hidden">
            <TeamProfileV2
              gameState={gameState}
              teamId={profileNavigation.selectedTeamId}
              onClose={handleBack}
              onSelectPlayer={selectPlayer}
            />
          </div>
        ) : profileNavigation.selectedStaffId && !viewingChampionKey ? (
          <div className="flex-1 overflow-hidden">
            <StaffProfileV2
              gameState={gameState}
              staffId={profileNavigation.selectedStaffId}
              onClose={handleBack}
              onGameUpdate={setGameState}
              onSelectTeam={selectTeam}
            />
          </div>
        ) : viewingChampionKey ? (
          <div className="flex-1 overflow-hidden">
            <ChampionPageV2
              championKey={viewingChampionKey}
              onClose={() => setViewingChampionKey(null)}
            />
          </div>
        ) : (
          <DashboardWorkspaceContent
            dashboardAlerts={[]}
            gameState={gameState}
            profileNavigation={profileNavigation}
            dashboardTabContentModel={dashboardTabContentModel}
            onBack={handleBack}
            onNavigate={handleNavigate}
            onSelectPlayer={selectPlayer}
            onSelectTeam={selectTeam}
            onGameUpdate={setGameState}
            isUnemployed={isUnemployed ?? false}
            viewingChampionKey={viewingChampionKey}
            onCloseChampion={() => setViewingChampionKey(null)}
            onViewChampion={(k) => setViewingChampionKey(k)}
          />
        )}
      </main>
    </div>
  );
}



