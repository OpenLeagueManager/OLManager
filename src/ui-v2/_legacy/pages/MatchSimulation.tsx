import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { useGameStore, GameStateData, LolTacticsData } from "@/store/gameStore";
import { useSettingsStore } from "@/store/settingsStore";
import {
  MatchSnapshot,
  MatchEvent,
  MatchDayStage,
} from "@/ui-v2/_legacy/components/match/types";
import { mapRuntimeEventsToMatchEvents, mergeRuntimeEventsIntoSnapshot } from "@/ui-v2/_legacy/components/match/matchRuntimeEvents";
import { resolveMatchFixture } from "@/ui-v2/_legacy/components/match/helpers";
import PreMatchSetup from "@/ui-v2/_legacy/components/match/PreMatchSetup";
import ChampionDraft, {
  authorizedRelationshipChampionIdsForViewer,
} from "@/ui-v2/_legacy/components/match/ChampionDraft";
import type { ChampionDraftResultPayload } from "@/ui-v2/_legacy/components/match/ChampionDraft";
import LolMatchLive from "@/ui-v2/_legacy/components/match/LolMatchLive";
import type { ChampionSelectionByPlayer } from "@/ui-v2/_legacy/components/match/LolMatchLive";
import MatchTacticsStage from "@/ui-v2/_legacy/components/match/MatchTacticsStage";
import LolResultScreen from "@/ui-v2/_legacy/components/match/LolResultScreen";
import DraftResultScreenV2 from "@/ui-v2/components/DraftResultScreenV2";
import PressConference from "@/ui-v2/_legacy/components/match/PressConference";
import {
  simulateDraftMatchResult,
  type DraftEvaluationsBySide,
  type DraftPlayerResult,
  type DraftMatchResult,
  type DraftStateEvaluation,
} from "@/ui-v2/_legacy/components/match/draftResultSimulator";
import {
  lolSimV2RunToCompletion,
} from "@/ui-v2/_legacy/components/match/lol-prototype/backend/tauri-client";
import {
  createDefaultObjectivesState,
  createEmptyNeutralTimersState,
  type LolSimV1MatchReportInput,
  type LolSimV1PolicyConfig,
  type LolSimV1RuntimeState,
} from "@/ui-v2/_legacy/components/match/lol-prototype/backend/contract-v1";
import { computeRoleModifiers, ROLE_ORDER, type DraftRole } from "@/lib/teams/lolTactics";
import { getLolStaffEffectsForTeam } from "@/lib/teams/lolStaffEffects";
import { buildLolScrimPrepPayload } from "@/lib/scrims/lolScrimPrep";
import { mapRuntimeWinnerToCanonicalScores } from "@/ui-v2/_legacy/pages/MatchSimulation.resultMapping";

// ---------------------------------------------------------------------------
// Multi-stage Match Day Orchestrator
// ---------------------------------------------------------------------------

interface MatchRouteState {
  fixtureIndex?: number;
  mode?: string;
  snapshot?: MatchSnapshot;
}

interface FinishLiveMatchResponse {
  game: GameStateData;
  round_summary?: unknown;
}

interface StoredFixtureDraftResult {
  snapshot: MatchSnapshot;
  controlledSide: "blue" | "red";
  result: DraftMatchResult;
  draftSessionId?: string;
  seriesGames?: StoredSeriesGameResult[];
  seriesLength?: 1 | 3 | 5;
  seriesGameIndex?: number;
  userSeriesWins?: number;
  opponentSeriesWins?: number;
  homeSeriesWins?: number;
  awaySeriesWins?: number;
  seriesUsedChampionIds?: string[];
}

interface StoredSeriesGameResult {
  gameIndex: number;
  result: DraftMatchResult;
  winnerSide?: "blue" | "red";
}

const DEFAULT_LOL_TACTICS: LolTacticsData = {
  strong_side: "Bot",
  game_timing: "Mid",
  jungle_style: "Enabler",
  jungle_pathing: "TopToBot",
  fight_plan: "FrontToBack",
  support_roaming: "Lane",
};

function attachLolTacticsToSnapshot(
  snapshot: MatchSnapshot,
  gameState: GameStateData,
  championSelections?: ChampionSelectionByPlayer | null,
): MatchSnapshot {
  const homeTeam = gameState.teams.find((team) => team.id === snapshot.home_team.id);
  const awayTeam = gameState.teams.find((team) => team.id === snapshot.away_team.id);

  const engineRoleToDraftRole = (role: string): DraftRole | null => {
    const normalized = role.toLowerCase();
    if (normalized === "top") return "TOP";
    if (normalized === "jungle") return "JUNGLE";
    if (normalized === "mid") return "MID";
    if (normalized === "adc") return "ADC";
    if (normalized === "support") return "SUPPORT";
    return null;
  };

  const buildImpactByPlayer = (
    players: MatchSnapshot["home_team"]["players"],
    tactics: LolTacticsData,
  ): Record<string, { modifier: number; variance: number }> => {
    const roleModifiers = computeRoleModifiers(tactics);
    const byRole = new Map<DraftRole, MatchSnapshot["home_team"]["players"][number]>();

    players.forEach((player) => {
      const role = engineRoleToDraftRole(player.role ?? "");
      if (!role || byRole.has(role)) return;
      byRole.set(role, player);
    });

    const impact: Record<string, { modifier: number; variance: number }> = {};
    ROLE_ORDER.forEach((role) => {
      const player = byRole.get(role);
      if (!player) return;
      const modifier = roleModifiers[role] ?? 0;
      const variance = Math.max(0.5, Math.abs(modifier) * 0.6 + 0.6);
      impact[player.id] = { modifier, variance };
    });

    return impact;
  };

  const homeTactics = homeTeam?.lol_tactics ?? DEFAULT_LOL_TACTICS;
  const awayTactics = awayTeam?.lol_tactics ?? DEFAULT_LOL_TACTICS;
  const homeStaffEffects = getLolStaffEffectsForTeam(gameState, homeTeam?.id ?? snapshot.home_team.id);
  const awayStaffEffects = getLolStaffEffectsForTeam(gameState, awayTeam?.id ?? snapshot.away_team.id);
  const roleImpactByPlayer = {
    home: buildImpactByPlayer(snapshot.home_team.players, homeTactics),
    away: buildImpactByPlayer(snapshot.away_team.players, awayTactics),
  };

  return {
    ...snapshot,
    // extra payload consumed by Rust sim v2
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    lol_tactics: {
      home: homeTactics,
      away: awayTactics,
    },
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    lol_role_impact_by_player: roleImpactByPlayer,
    // extra payload consumed by Rust sim v2: conservative preparation/execution signal
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    lol_staff_effects: {
      home: homeStaffEffects,
      away: awayStaffEffects,
    },
    // extra payload consumed by Rust sim v2: small scrim-derived execution signal
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    lol_scrim_prep: buildLolScrimPrepPayload(gameState, snapshot, championSelections),
  } as MatchSnapshot;
}

function buildLolMatchReport(runtime: LolSimV1RuntimeState): LolSimV1MatchReportInput {
  const safeStats = {
    blue: {
      kills: runtime.stats?.blue?.kills ?? 0,
      towers: runtime.stats?.blue?.towers ?? 0,
      dragons: runtime.stats?.blue?.dragons ?? 0,
      barons: runtime.stats?.blue?.barons ?? 0,
      gold: runtime.stats?.blue?.gold ?? 0,
    },
    red: {
      kills: runtime.stats?.red?.kills ?? 0,
      towers: runtime.stats?.red?.towers ?? 0,
      dragons: runtime.stats?.red?.dragons ?? 0,
      barons: runtime.stats?.red?.barons ?? 0,
      gold: runtime.stats?.red?.gold ?? 0,
    },
  };

  return {
    winner: runtime.winner,
    timeSec: runtime.timeSec ?? 0,
    events: (runtime.events ?? []).map((event) => ({
      t: event.t,
      text: event.text,
      type: event.type,
    })),
    stats: {
      blue: {
        kills: safeStats.blue.kills,
        deaths: safeStats.red.kills,
        gold: safeStats.blue.gold,
        towers: safeStats.blue.towers,
        dragons: safeStats.blue.dragons,
        barons: safeStats.blue.barons,
      },
      red: {
        kills: safeStats.red.kills,
        deaths: safeStats.blue.kills,
        gold: safeStats.red.gold,
        towers: safeStats.red.towers,
        dragons: safeStats.red.dragons,
        barons: safeStats.red.barons,
      },
    },
    champions: (runtime.champions ?? []).map((champion) => ({
      id: champion.id,
      name: champion.name,
      team: champion.team,
      role: champion.role,
      kills: champion.kills,
      deaths: champion.deaths,
      assists: champion.assists,
      cs: champion.cs,
      gold: champion.gold,
      spentGold: champion.spentGold,
    })),
  };
}

function normalizeDraftPayload(
  payload: ChampionDraftResultPayload | null,
  selections: ChampionSelectionByPlayer | null,
  snapshot: MatchSnapshot | null,
): ChampionDraftResultPayload | null {
  if (
    payload?.blue?.picks &&
    payload?.red?.picks &&
    Array.isArray(payload.history)
  ) {
    return payload;
  }

  if (!selections || !snapshot) return null;

  const roles = ["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"] as const;

  const buildPicks = (
    players: MatchSnapshot["home_team"]["players"],
    championsByPlayer: Record<string, string>,
    rolesByPlayer: Record<string, "TOP" | "JUNGLE" | "MID" | "ADC" | "SUPPORT">,
  ) => {
    const taken = new Set<string>();
    return roles.map((role) => {
      const byRole = players.find((player) => rolesByPlayer[player.id] === role && !taken.has(player.id));
      const fallback = players.find((player) => !taken.has(player.id));
      const selectedPlayer = byRole ?? fallback;
      if (selectedPlayer) {
        taken.add(selectedPlayer.id);
      }
      return {
        role,
        championId: selectedPlayer ? championsByPlayer[selectedPlayer.id] ?? `generic-${role.toLowerCase()}` : `generic-${role.toLowerCase()}`,
      };
    });
  };

  const baseScore = { mastery: 0, synergy: 0, counter: 0, comfort: 0, preparation: 0, total: 0 };

  return {
    blue: {
      picks: buildPicks(
        snapshot.home_team.players,
        selections.home,
        selections.homeRoles,
      ),
      bans: [],
      score: baseScore,
    },
    red: {
      picks: buildPicks(
        snapshot.away_team.players,
        selections.away,
        selections.awayRoles,
      ),
      bans: [],
      score: baseScore,
    },
    history: [],
  };
}

export function draftPicksByPlayerRole(
  players: Array<{ id: string; role?: string }>,
  picks: Array<{ role: "TOP" | "JUNGLE" | "MID" | "ADC" | "SUPPORT"; championId: string }>,
): Array<{ playerId: string; championId: string }> {
  const roleForPlayer = (role: string | undefined): string => {
    const normalized = (role ?? "").toUpperCase();
    if (normalized.includes("JUNG")) return "JUNGLE";
    if (normalized.includes("ADC") || normalized.includes("BOT")) return "ADC";
    if (normalized.includes("SUP")) return "SUPPORT";
    if (normalized.includes("TOP")) return "TOP";
    return "MID";
  };
  const used = new Set<string>();
  return picks.flatMap((pick) => {
    const player = players.find((candidate) => !used.has(candidate.id) && roleForPlayer(candidate.role) === pick.role)
      ?? players.find((candidate) => !used.has(candidate.id));
    if (!player) return [];
    used.add(player.id);
    return [{ playerId: player.id, championId: pick.championId }];
  });
}

function draftEvaluationInputs(
  snapshot: MatchSnapshot,
  payload: ChampionDraftResultPayload,
): Array<{ playerId: string; championId: string; effectiveRole: "TOP" | "JUNGLE" | "MID" | "ADC" | "SUPPORT" }> {
  const inputsForSide = (
    players: MatchSnapshot["home_team"]["players"],
    picks: ChampionDraftResultPayload["blue"]["picks"],
  ) => {
    const picksByPlayer = draftPicksByPlayerRole(players, picks);
    return picksByPlayer.map(({ playerId, championId }) => ({
      playerId,
      championId,
      effectiveRole: picks.find((pick) => pick.championId === championId)?.role ?? "SUPPORT",
    }));
  };
  return [
    ...inputsForSide(snapshot.home_team.players, payload.blue.picks),
    ...inputsForSide(snapshot.away_team.players, payload.red.picks),
  ];
}

function parseRuntimeEventSide(text: string | undefined): "blue" | "red" | null {
  const upper = (text ?? "").toUpperCase();
  if (upper.includes("BLUE")) return "blue";
  if (upper.includes("RED")) return "red";
  return null;
}

function runtimeEventToDraftType(event: { type?: string; text?: string }):
  | "first_blood"
  | "voidgrubs"
  | "dragon"
  | "dragon_soul"
  | "elder"
  | "herald"
  | "baron"
  | "turret"
  | "inhibitor"
  | "nexus_turret"
  | "nexus"
  | null {
  const text = (event.text ?? "").toLowerCase();
  const eventType = (event.type ?? "").toLowerCase();
  const isMeaningfulText = /\b(killed|destroyed|secured|deployed)\b/.test(text);
  if (!isMeaningfulText && !["kill", "tower", "dragon", "baron", "nexus"].includes(eventType)) return null;

  if (text.includes("first blood")) return "first_blood";
  if (text.includes("voidgrub")) return "voidgrubs";
  if (text.includes("dragon soul") || text.includes(" soul")) return "dragon_soul";
  if (text.includes("elder")) return "elder";
  if (text.includes("baron")) return "baron";
  if (text.includes("herald")) return "herald";
  if (text.includes("inhib")) return "inhibitor";
  if (text.includes("nexus") && (text.includes("turret") || text.includes("tower"))) return "nexus_turret";
  if (text.includes("nexus")) return "nexus";
  if (text.includes("tower") || text.includes("turret")) return "turret";
  if (text.includes("dragon")) return "dragon";

  switch (eventType) {
    case "kill":
      return "first_blood";
    case "tower":
      return "turret";
    case "dragon":
      return "dragon";
    case "baron":
      return "baron";
    case "nexus":
      return "nexus";
    default:
      return null;
  }
}

function draftTypeLabel(type: NonNullable<ReturnType<typeof runtimeEventToDraftType>>): string {
  switch (type) {
    case "first_blood":
      return "First Blood";
    case "voidgrubs":
      return "Voidgrubs x3";
    case "dragon":
      return "Dragon";
    case "dragon_soul":
      return "Dragon Soul";
    case "elder":
      return "Elder Dragon";
    case "herald":
      return "Herald";
    case "baron":
      return "Baron";
    case "turret":
      return "Turret";
    case "inhibitor":
      return "Inhibitor";
    case "nexus_turret":
      return "Nexus Turret";
    case "nexus":
      return "Nexus";
  }
}

function runtimeRoleToDraftRole(role: string | undefined): DraftPlayerResult["role"] {
  switch ((role ?? "").toUpperCase()) {
    case "JGL":
    case "JUNGLE":
      return "JUNGLE";
    case "SUP":
    case "SUPPORT":
      return "SUPPORT";
    case "ADC":
      return "ADC";
    case "TOP":
      return "TOP";
    case "MID":
    default:
      return "MID";
  }
}

function buildDraftResultFromRuntime(params: {
  runtime: LolSimV1RuntimeState;
  snapshot: MatchSnapshot;
  championSelections: ChampionSelectionByPlayer | null;
}): DraftMatchResult {
  const { runtime, snapshot, championSelections } = params;
  const durationMinutes = Math.max(1, Math.floor((runtime.timeSec ?? 0) / 60));

  const blueKills = runtime.stats?.blue?.kills ?? 0;
  const redKills = runtime.stats?.red?.kills ?? 0;

  const champions = runtime.champions ?? [];
  const selectionByPlayerId = {
    ...(championSelections?.home ?? {}),
    ...(championSelections?.away ?? {}),
  };
  const playerResults: DraftPlayerResult[] = champions.map((champion) => ({
    side: champion.team === "red" ? "red" : "blue",
    playerId: champion.id,
    playerName: champion.name,
    role: runtimeRoleToDraftRole(champion.role),
    championId: selectionByPlayerId[champion.id] ?? null,
    kills: champion.kills ?? 0,
    deaths: champion.deaths ?? 0,
    assists: champion.assists ?? 0,
    gold: champion.gold ?? 0,
    rating: Number(
      Math.max(
        1,
        Math.min(
          10,
          5.5 +
            (champion.kills ?? 0) * 0.45 +
            (champion.assists ?? 0) * 0.12 -
            (champion.deaths ?? 0) * 0.35 +
            ((champion.team === "red" ? "red" : "blue") ===
            (runtime.winner === "red" ? "red" : "blue")
              ? 0.8
              : -0.4),
        ),
      ).toFixed(1),
    ),
  }));

  const fallbackRows: DraftPlayerResult[] = [
    ...snapshot.home_team.players.map((player, idx) => ({
      side: "blue" as const,
      playerId: player.id,
      playerName: player.name,
      role: (["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"] as const)[idx] ?? "MID",
      championId: selectionByPlayerId[player.id] ?? null,
      kills: 0,
      deaths: 0,
      assists: 0,
      gold: 0,
      rating: 5,
    })),
    ...snapshot.away_team.players.map((player, idx) => ({
      side: "red" as const,
      playerId: player.id,
      playerName: player.name,
      role: (["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"] as const)[idx] ?? "MID",
      championId: selectionByPlayerId[player.id] ?? null,
      kills: 0,
      deaths: 0,
      assists: 0,
      gold: 0,
      rating: 5,
    })),
  ];

  const resolvedRows = playerResults.length > 0 ? playerResults : fallbackRows;
  const mvp = [...resolvedRows].sort((a, b) => b.rating - a.rating)[0] ?? fallbackRows[0];

  const blueGold = runtime.stats?.blue?.gold ?? 0;
  const redGold = runtime.stats?.red?.gold ?? 0;
  const timelineEvents = (runtime.events ?? [])
    .map((event) => {
      const side = parseRuntimeEventSide(event.text);
      if (!side) return null;
      const type = runtimeEventToDraftType(event);
      if (!type) return null;
      return {
        minute: Math.max(0, Math.floor((event.t ?? 0) / 60)),
        side,
        type,
        label: draftTypeLabel(type),
      };
    })
    .filter((item): item is NonNullable<typeof item> => item !== null)
    .slice(-28);

  const finalDelta = blueGold - redGold;
  const runtimeGoldTimeline = (runtime.goldDiffTimeline ?? [])
    .filter((point) => Number.isFinite(point.minute) && Number.isFinite(point.diff))
    .map((point) => ({ minute: Math.max(0, Math.floor(point.minute)), diff: Math.round(point.diff) }));
  const timelinePoints = Math.max(8, Math.min(24, Math.floor(durationMinutes / 2) + 1));
  const timelineMinutes = Array.from({ length: timelinePoints }, (_, idx) =>
    Math.floor((idx / Math.max(1, timelinePoints - 1)) * durationMinutes),
  );
  if (timelineMinutes[timelineMinutes.length - 1] !== durationMinutes) {
    timelineMinutes.push(durationMinutes);
  }

  const eventImpact = (type: NonNullable<ReturnType<typeof runtimeEventToDraftType>>, side: "blue" | "red") => {
    const sign = side === "blue" ? 1 : -1;
    switch (type) {
      case "first_blood":
        return sign * 450;
      case "voidgrubs":
        return sign * 320;
      case "dragon":
        return sign * 220;
      case "dragon_soul":
        return sign * 700;
      case "elder":
        return sign * 1100;
      case "herald":
        return sign * 380;
      case "baron":
        return sign * 900;
      case "turret":
        return sign * 650;
      case "inhibitor":
        return sign * 850;
      case "nexus_turret":
        return sign * 1050;
      case "nexus":
        return sign * 2000;
      default:
        return 0;
    }
  };

  const rawTimeline = timelineMinutes.map((minute, idx) => {
    if (idx === 0) return 0;
    const progress = Math.max(0, Math.min(1, minute / Math.max(1, durationMinutes)));
    const base = finalDelta * Math.pow(progress, 1.08);
    const events = timelineEvents
      .filter((event) => event.minute <= minute)
      .reduce((sum, event) => sum + eventImpact(event.type, event.side), 0);
    const jitter = Math.round((Math.sin(minute + idx) * 0.5 + 0.5) * 120 * (1 - progress));
    return base + events * 0.35 + jitter;
  });

  const rawFirst = rawTimeline[0] ?? 0;
  const rawLast = rawTimeline[rawTimeline.length - 1] ?? 0;
  const scale = Math.abs(rawLast - rawFirst) < 1 ? 1 : (finalDelta / (rawLast - rawFirst));
  const goldDiffTimeline = runtimeGoldTimeline.length >= 2
    ? runtimeGoldTimeline
    : timelineMinutes.map((minute, idx) => ({
      minute,
      diff: Math.round((rawTimeline[idx] - rawFirst) * scale),
    }));

  const winnerSide = runtime.winner === "red" ? "red" : "blue";

  return {
    winnerSide,
    durationMinutes,
    blueKills,
    redKills,
    mvp,
    playerResults: resolvedRows,
    goldDiffTimeline,
    timelineEvents,
    objectives: {
      blue: {
        voidgrubs: 0,
        dragons: runtime.stats?.blue?.dragons ?? 0,
        dragonSoul: false,
        elderDragons: 0,
        heralds: 0,
        barons: runtime.stats?.blue?.barons ?? 0,
        towers: runtime.stats?.blue?.towers ?? 0,
        inhibitors: 0,
      },
      red: {
        voidgrubs: 0,
        dragons: runtime.stats?.red?.dragons ?? 0,
        dragonSoul: false,
        elderDragons: 0,
        heralds: 0,
        barons: runtime.stats?.red?.barons ?? 0,
        towers: runtime.stats?.red?.towers ?? 0,
        inhibitors: 0,
      },
    },
    power: {
      blue: 50,
      red: 50,
      diff: 0,
      autoWin: false,
      winProbBlue: 50,
    },
  };
}

const PARALLEL_SIM_MAX_TICKS = 3600;
const PARALLEL_SIM_DT_SEC = 0.2;
const PARALLEL_SIM_SPEED = 12;
const DRAFT_RUNTIME_SESSION_ID = `${Date.now()}-${Math.random().toString(36).slice(2)}`;

function persistFixtureDraftResult(
  fixtureId: string,
  payload: StoredFixtureDraftResult,
) {
  if (typeof window === "undefined") return;

  try {
    window.localStorage.setItem(
      `fixture-draft-result:${fixtureId}`,
      JSON.stringify({
        ...payload,
        draftSessionId: DRAFT_RUNTIME_SESSION_ID,
      }),
    );
  } catch (error) {
    console.warn("[MatchSimulation] fixtureResult:saveFailed", {
      error,
      fixtureId,
    });
  }
}

function readStoredFixtureDraftResult(fixtureId: string): StoredFixtureDraftResult | null {
  if (typeof window === "undefined") return null;

  try {
    const raw = window.localStorage.getItem(`fixture-draft-result:${fixtureId}`);
    if (!raw) return null;
    return JSON.parse(raw) as StoredFixtureDraftResult;
  } catch {
    return null;
  }
}

function clearStoredFixtureDraftResult(fixtureId: string) {
  if (typeof window === "undefined") return;

  try {
    window.localStorage.removeItem(`fixture-draft-result:${fixtureId}`);
  } catch (error) {
    console.warn("[MatchSimulation] fixtureResult:clearFailed", {
      error,
      fixtureId,
    });
  }
}

function getSeriesSessionKey(fixtureId: string): string {
  return `fixture-draft-session-active:${fixtureId}`;
}

export function hasActiveSeriesSession(fixtureId: string): boolean {
  if (typeof window === "undefined") return false;

  try {
    return window.sessionStorage.getItem(getSeriesSessionKey(fixtureId)) === "1";
  } catch {
    return false;
  }
}

function markActiveSeriesSession(fixtureId: string) {
  if (typeof window === "undefined") return;

  try {
    window.sessionStorage.setItem(getSeriesSessionKey(fixtureId), "1");
  } catch {
    // no-op
  }
}

function clearActiveSeriesSession(fixtureId: string) {
  if (typeof window === "undefined") return;

  try {
    window.sessionStorage.removeItem(getSeriesSessionKey(fixtureId));
  } catch {
    // no-op
  }
}

function isCurrentRuntimeDraftSession(fixtureId: string): boolean {
  if (typeof window === "undefined") return false;

  try {
    return window.sessionStorage.getItem(getSeriesSessionKey(fixtureId)) === DRAFT_RUNTIME_SESSION_ID;
  } catch {
    return false;
  }
}

function readSeriesWins(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  return Math.max(0, Math.floor(value));
}

function getTargetSeriesWins(seriesLength: 1 | 3 | 5): number {
  return seriesLength === 1 ? 1 : seriesLength === 3 ? 2 : 3;
}

function hasTeamReachedSeriesTarget(
  seriesLength: 1 | 3 | 5,
  homeWins: number,
  awayWins: number,
): boolean {
  const targetSeriesWins = getTargetSeriesWins(seriesLength);
  return homeWins >= targetSeriesWins || awayWins >= targetSeriesWins;
}

function hasSeriesGamesSupportingScore(
  seriesLength: 1 | 3 | 5,
  homeWins: number,
  awayWins: number,
  games: StoredSeriesGameResult[],
): boolean {
  if (seriesLength <= 1) return true;

  const totalWins = homeWins + awayWins;
  const decidedGameCount = normalizeStoredSeriesGames(games).filter(
    (entry) => entry.winnerSide === "blue" || entry.winnerSide === "red",
  ).length;

  return totalWins > 0 && totalWins <= seriesLength && decidedGameCount >= totalWins;
}

function hasEnoughSeriesGamesForScore(
  seriesLength: 1 | 3 | 5,
  homeWins: number,
  awayWins: number,
  games: StoredSeriesGameResult[],
): boolean {
  const totalWins = homeWins + awayWins;

  return (
    totalWins >= getTargetSeriesWins(seriesLength) &&
    hasSeriesGamesSupportingScore(seriesLength, homeWins, awayWins, games)
  );
}

function isSupportedSeriesComplete(
  seriesLength: 1 | 3 | 5,
  homeWins: number,
  awayWins: number,
  games: StoredSeriesGameResult[],
): boolean {
  return (
    seriesLength <= 1 ||
    (hasTeamReachedSeriesTarget(seriesLength, homeWins, awayWins) &&
      hasEnoughSeriesGamesForScore(seriesLength, homeWins, awayWins, games))
  );
}

function getNextSeriesGameIndex(games: StoredSeriesGameResult[]): number {
  const normalizedGames = normalizeStoredSeriesGames(games);
  const latestGameIndex = Math.max(0, ...normalizedGames.map((entry) => entry.gameIndex));
  return latestGameIndex + 1;
}

function normalizeStoredSeriesGames(value: unknown): StoredSeriesGameResult[] {
  if (!Array.isArray(value)) return [];

  const byGameIndex = new Map<number, StoredSeriesGameResult>();

  value.forEach((entry) => {
    if (!entry || typeof entry !== "object") return;

    const candidate = entry as Partial<StoredSeriesGameResult>;
    const gameIndex =
      typeof candidate.gameIndex === "number" && Number.isFinite(candidate.gameIndex)
        ? Math.max(1, Math.floor(candidate.gameIndex))
        : null;

    if (gameIndex === null || !candidate.result || typeof candidate.result !== "object") return;

    const winnerSide =
      candidate.winnerSide === "blue" || candidate.winnerSide === "red"
        ? candidate.winnerSide
        : candidate.result.winnerSide;

    byGameIndex.set(gameIndex, {
      gameIndex,
      result: candidate.result,
      winnerSide,
    });
  });

  return Array.from(byGameIndex.values()).sort((left, right) => left.gameIndex - right.gameIndex);
}

export default function MatchSimulation() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const routeState = (location.state as MatchRouteState | null) ?? null;
  const matchMode = routeState?.mode || "live";
  const effectiveMatchMode = matchMode === "delegate" ? "spectator" : matchMode;
  const { gameState, setGameState } = useGameStore();
  const { settings } = useSettingsStore();
  const simPolicy = useMemo<LolSimV1PolicyConfig>(() => ({
    hybridOpenTradeConfidenceHigh: settings.lol_hybrid_open_trade_confidence_high,
    hybridDisengageConfidenceLow: settings.lol_hybrid_disengage_confidence_low,
  }), [settings.lol_hybrid_disengage_confidence_low, settings.lol_hybrid_open_trade_confidence_high]);
  const [snapshot, setSnapshot] = useState<MatchSnapshot | null>(
    routeState?.snapshot ?? null,
  );
  const [stage, setStage] = useState<MatchDayStage>("prematch");
  const [importantEvents, setImportantEvents] = useState<MatchEvent[]>([]);
  const [finalRuntimeState, setFinalRuntimeState] = useState<LolSimV1RuntimeState | null>(null);
  const [draftPayload, setDraftPayload] = useState<ChampionDraftResultPayload | null>(null);
  const [draftEvaluations, setDraftEvaluations] = useState<DraftEvaluationsBySide | null>(null);
  const [draftResultSimulation, setDraftResultSimulation] = useState<DraftMatchResult | null>(null);
  const [finalDraftResult, setFinalDraftResult] = useState<DraftMatchResult | null>(null);
  const [championSelections, setChampionSelections] = useState<ChampionSelectionByPlayer | null>(null);
  const [seriesGameIndex, setSeriesGameIndex] = useState(0);
  const [seriesHomeWins, setSeriesHomeWins] = useState(0);
  const [seriesAwayWins, setSeriesAwayWins] = useState(0);
  const [seriesGames, setSeriesGames] = useState<StoredSeriesGameResult[]>([]);
  const [seriesUsedChampionIds, setSeriesUsedChampionIds] = useState<string[]>([]);
  const [userSide, setUserSide] = useState<"Home" | "Away" | null>(null);
  const [isSpectator, setIsSpectator] = useState(effectiveMatchMode === "spectator");
  const [hasFinalizedMatch, setHasFinalizedMatch] = useState(false);
  const [isSimulating, setIsSimulating] = useState(false);
  const [simulationFeedback, setSimulationFeedback] = useState<string | null>(null);
  const delegateAutoAdvanceKeyRef = useRef<string | null>(null);
  const delegateAutoSimulateKeyRef = useRef<string | null>(null);
  const delegateSimulateInFlightRef = useRef(false);

  useEffect(() => {
    console.info("[MatchSimulation] mount", {
      fixtureIndex: routeState?.fixtureIndex,
      hasGameState: !!gameState,
      hasRouteSnapshot: !!routeState?.snapshot,
      matchMode,
    });
  }, [gameState, matchMode, routeState?.fixtureIndex, routeState?.snapshot]);

  // Determine user side from game state
  useEffect(() => {
    if (!gameState || !snapshot) return;
    const utid = gameState.manager.team_id;
    if (!utid) {
      console.warn("[MatchSimulation] resolveSide: no manager team_id, forcing spectator");
      setIsSpectator(true);
      return;
    }
    const isHome = snapshot.home_team.id === utid;
    const isAway = snapshot.away_team.id === utid;
    if (isHome) setUserSide("Home");
    else if (isAway) setUserSide("Away");
    else {
      console.warn("[MatchSimulation] resolveSide: team_id mismatch", {
        managerTeamId: utid,
        homeTeamId: snapshot.home_team.id,
        awayTeamId: snapshot.away_team.id,
      });
      setIsSpectator(true);
    }

    // If mode is spectator, force spectator regardless of team
    if (effectiveMatchMode === "spectator") setIsSpectator(true);

    console.info("[MatchSimulation] resolveSide", {
      awayTeamId: snapshot.away_team.id,
      homeTeamId: snapshot.home_team.id,
      matchMode,
      managerTeamId: utid,
      resolvedUserSide: isHome ? "Home" : isAway ? "Away" : null,
      isSpectator: !isHome && !isAway,
    });
  }, [effectiveMatchMode, gameState, snapshot?.home_team.id, snapshot?.away_team.id]);

  useEffect(() => {
    console.info("[MatchSimulation] stage", {
      hasSnapshot: !!snapshot,
      isSpectator,
      stage,
      userSide,
    });
  }, [isSpectator, snapshot, stage, userSide]);

  // Fetch initial snapshot
  useEffect(() => {
    let isCancelled = false;

    const fetchSnapshot = async () => {
      console.info("[MatchSimulation] fetchSnapshot:start", {
        fixtureIndex: routeState?.fixtureIndex,
        hasRouteSnapshot: !!routeState?.snapshot,
        matchMode,
      });
      try {
        const snap = await invoke<MatchSnapshot>("get_match_snapshot");
        console.info("[MatchSimulation] fetchSnapshot:success", {
          awayPlayers: snap.away_team.players.length,
          awayTeam: snap.away_team.name,
          homePlayers: snap.home_team.players.length,
          homeTeam: snap.home_team.name,
          phase: snap.phase,
        });
        console.debug("[MatchSimulation] fetchSnapshot:full", JSON.parse(JSON.stringify(snap)));
        if (!isCancelled) {
          setSnapshot(snap);
        }
        return;
      } catch (snapshotError) {
        console.warn("[MatchSimulation] fetchSnapshot:failed", snapshotError);
        if (typeof routeState?.fixtureIndex !== "number") {
          console.error("Failed to get match snapshot:", snapshotError);
          navigate("/dashboard");
          return;
        }

        try {
          console.info("[MatchSimulation] restoreLiveMatch:start", {
            fixtureIndex: routeState.fixtureIndex,
            matchMode,
          });
          const restoredSnapshot = await invoke<MatchSnapshot>(
            "start_live_match",
            {
              allowsExtraTime: false,
              fixtureIndex: routeState.fixtureIndex,
              mode: effectiveMatchMode,
            },
          );

          console.info("[MatchSimulation] restoreLiveMatch:success", {
            awayPlayers: restoredSnapshot.away_team.players.length,
            awayTeam: restoredSnapshot.away_team.name,
            homePlayers: restoredSnapshot.home_team.players.length,
            homeTeam: restoredSnapshot.home_team.name,
            phase: restoredSnapshot.phase,
          });

          if (!isCancelled) {
            setSnapshot(restoredSnapshot);
          }
        } catch (restoreError) {
          console.error("Failed to restore live match session:", restoreError);
          navigate("/dashboard");
        }
      }
    };

    fetchSnapshot();

    return () => {
      isCancelled = true;
    };
  }, [effectiveMatchMode, navigate, routeState?.fixtureIndex]);

  const currentFixture =
    gameState && snapshot
      ? resolveMatchFixture(gameState, snapshot, routeState?.fixtureIndex)
      : null;

  useEffect(() => {
    if (!currentFixture?.id) {
      setSeriesGames([]);
      return;
    }

    const stored = readStoredFixtureDraftResult(currentFixture.id);
    setSeriesGames(normalizeStoredSeriesGames(stored?.seriesGames));
  }, [currentFixture?.id]);

  const playoffFixtures =
    gameState?.leagues?.[0]?.fixtures.filter(
      (fixture) => fixture.match_type === "Playoffs",
    ) ?? [];

  const playoffFinalMatchday =
    playoffFixtures.length > 0
      ? Math.max(...playoffFixtures.map((fixture) => fixture.matchday))
      : null;

  const preseasonFriendlyFixtures =
    gameState?.leagues?.[0]?.fixtures
      .filter((fixture) => fixture.match_type === "Friendly" && fixture.matchday === 0)
      .sort((left, right) =>
        left.date.localeCompare(right.date) ||
        left.matchday - right.matchday ||
        left.id.localeCompare(right.id),
      ) ?? [];

  const firstFriendlyFixtureId = preseasonFriendlyFixtures[0]?.id ?? null;
  const secondFriendlyFixtureId = preseasonFriendlyFixtures[1]?.id ?? null;

  const playoffStartMatchday =
    playoffFixtures.length > 0
      ? Math.min(...playoffFixtures.map((fixture) => fixture.matchday))
      : null;

  const explicitSeriesLength = (() => {
    const raw = currentFixture?.best_of;
    if (raw === 1 || raw === 3 || raw === 5) return raw;
    return null;
  })();

  const seriesLength: 1 | 3 | 5 =
    explicitSeriesLength ??
    (currentFixture?.match_type === "Friendly"
      ? currentFixture.id === firstFriendlyFixtureId
        ? 3
        : currentFixture.id === secondFriendlyFixtureId
          ? 5
          : 1
      : currentFixture?.match_type !== "Playoffs"
        ? 1
      : playoffStartMatchday !== null &&
          currentFixture.matchday === playoffStartMatchday + 1
        ? 5
        : playoffFinalMatchday !== null &&
            currentFixture.matchday >= playoffFinalMatchday
        ? 5
        : 3);

  useEffect(() => {
    if (!currentFixture?.id || seriesLength <= 1) {
      return;
    }

    const stored = readStoredFixtureDraftResult(currentFixture.id);
    if (!stored) {
      markActiveSeriesSession(currentFixture.id);
      try {
        window.sessionStorage.setItem(getSeriesSessionKey(currentFixture.id), DRAFT_RUNTIME_SESSION_ID);
      } catch {
        // no-op
      }
      return;
    }

    const homeWins = readSeriesWins(stored.homeSeriesWins);
    const awayWins = readSeriesWins(stored.awaySeriesWins);
    const storedSeriesGames = normalizeStoredSeriesGames(stored.seriesGames);
    const isSeriesComplete = isSupportedSeriesComplete(seriesLength, homeWins, awayWins, storedSeriesGames);

    if (isSeriesComplete) {
      markActiveSeriesSession(currentFixture.id);
      try {
        window.sessionStorage.setItem(getSeriesSessionKey(currentFixture.id), DRAFT_RUNTIME_SESSION_ID);
      } catch {
        // no-op
      }
      return;
    }

    const fixtureHomeWins = readSeriesWins(currentFixture.result?.home_wins) || readSeriesWins(currentFixture.result?.home_goals);
    const fixtureAwayWins = readSeriesWins(currentFixture.result?.away_wins) || readSeriesWins(currentFixture.result?.away_goals);
    const storedMatchesFixtureScore = fixtureHomeWins === homeWins && fixtureAwayWins === awayWins;
    if (
      currentFixture.status !== "Scheduled" &&
      storedMatchesFixtureScore &&
      hasSeriesGamesSupportingScore(seriesLength, homeWins, awayWins, storedSeriesGames)
    ) {
      markActiveSeriesSession(currentFixture.id);
      try {
        window.sessionStorage.setItem(getSeriesSessionKey(currentFixture.id), DRAFT_RUNTIME_SESSION_ID);
      } catch {
        // no-op
      }
      return;
    }

    if (stored.draftSessionId !== DRAFT_RUNTIME_SESSION_ID) {
      clearStoredFixtureDraftResult(currentFixture.id);
      clearActiveSeriesSession(currentFixture.id);
      return;
    }

    // Never resume an incomplete BO-series from persisted storage.
    // We keep only completed series results.
    clearStoredFixtureDraftResult(currentFixture.id);
    clearActiveSeriesSession(currentFixture.id);
  }, [
    currentFixture?.id,
    currentFixture?.result?.away_goals,
    currentFixture?.result?.away_wins,
    currentFixture?.result?.home_goals,
    currentFixture?.result?.home_wins,
    currentFixture?.status,
    seriesLength,
  ]);

  useEffect(() => {
    if (!currentFixture?.id) {
      setSeriesGameIndex(0);
      setSeriesHomeWins(0);
      setSeriesAwayWins(0);
      setSeriesUsedChampionIds([]);
      return;
    }

    const stored = readStoredFixtureDraftResult(currentFixture.id);
    const fixtureIsScheduled = currentFixture.status === "Scheduled";
    const resumeFromFixtureResult =
      seriesLength <= 1 ||
      (isCurrentRuntimeDraftSession(currentFixture.id) && !fixtureIsScheduled);
    const fromResultHome = resumeFromFixtureResult
      ? readSeriesWins(currentFixture.result?.home_wins) || readSeriesWins(currentFixture.result?.home_goals)
      : 0;
    const fromResultAway = resumeFromFixtureResult
      ? readSeriesWins(currentFixture.result?.away_wins) || readSeriesWins(currentFixture.result?.away_goals)
      : 0;
    const storedHomeWins = readSeriesWins(stored?.homeSeriesWins);
    const storedAwayWins = readSeriesWins(stored?.awaySeriesWins);
    const storedSeriesGames = normalizeStoredSeriesGames(stored?.seriesGames);
    const storedSeriesIsComplete =
      seriesLength > 1 && isSupportedSeriesComplete(seriesLength, storedHomeWins, storedAwayWins, storedSeriesGames);
    const persistedWinsEnabled = resumeFromFixtureResult || storedSeriesIsComplete;
    const homeWins = persistedWinsEnabled ? Math.max(fromResultHome, storedHomeWins) : 0;
    const awayWins = persistedWinsEnabled ? Math.max(fromResultAway, storedAwayWins) : 0;
    const canReuseStoredState = seriesLength > 1 && resumeFromFixtureResult;

    setSeriesHomeWins(homeWins);
    setSeriesAwayWins(awayWins);
    setSeriesGameIndex(homeWins + awayWins);
    setSeriesUsedChampionIds(
      canReuseStoredState && Array.isArray(stored?.seriesUsedChampionIds)
        ? stored.seriesUsedChampionIds
        : [],
    );
  }, [
    currentFixture?.id,
    currentFixture?.result?.away_goals,
    currentFixture?.result?.away_wins,
    currentFixture?.result?.home_goals,
    currentFixture?.result?.home_wins,
    seriesLength,
  ]);

  const normalizeTeamKey = (value: string): string =>
    value.trim().toLowerCase().replace(/[^a-z0-9]/g, "");

  const managerTeamId = gameState?.manager.team_id ?? null;
  const managerTeamName =
    gameState?.teams.find((team) => team.id === managerTeamId)?.name ?? null;

  const defaultControlledDraftSide: "blue" | "red" = (() => {
    if (!snapshot) return "blue";
    if (managerTeamId && managerTeamId === snapshot.away_team.id) return "red";
    if (managerTeamId && managerTeamId === snapshot.home_team.id) return "blue";

    if (managerTeamName) {
      const managerNameKey = normalizeTeamKey(managerTeamName);
      if (managerNameKey === normalizeTeamKey(snapshot.away_team.name)) return "red";
      if (managerNameKey === normalizeTeamKey(snapshot.home_team.name)) return "blue";
    }

    if (userSide === "Away") return "red";
    return "blue";
  })();

  const [userSelectedSide, setUserSelectedSide] = useState<"blue" | "red">(
    defaultControlledDraftSide,
  );

  useEffect(() => {
    setUserSelectedSide(defaultControlledDraftSide);
  }, [defaultControlledDraftSide]);

  const swapSnapshotSides = useCallback((snap: MatchSnapshot): MatchSnapshot => {
    return {
      ...snap,
      home_team: snap.away_team,
      away_team: snap.home_team,
      home_bench: snap.away_bench,
      away_bench: snap.home_bench,
    };
  }, []);

  const activeSnapshot = useMemo(() => {
    if (!snapshot || !managerTeamId) return snapshot;

    const isUserHome = managerTeamId === snapshot.home_team.id;
    const shouldBeBlue = userSelectedSide === "blue";

    if ((isUserHome && shouldBeBlue) || (!isUserHome && !shouldBeBlue)) {
      return snapshot;
    }

    return swapSnapshotSides(snapshot);
  }, [managerTeamId, snapshot, swapSnapshotSides, userSelectedSide]);

  const blueTeamId = useMemo(() => {
    if (!snapshot) return null;
    if (!managerTeamId) return snapshot.home_team.id;

    const isUserHome = managerTeamId === snapshot.home_team.id;
    const shouldBeBlue = userSelectedSide === "blue";
    const notSwapped = (isUserHome && shouldBeBlue) || (!isUserHome && !shouldBeBlue);
    return notSwapped ? snapshot.home_team.id : snapshot.away_team.id;
  }, [managerTeamId, snapshot, userSelectedSide]);

  const renderSnapshot = activeSnapshot ?? snapshot;
  const renderSnapshotWithTactics = useMemo(() => {
    if (!renderSnapshot || !gameState) return renderSnapshot;
    return attachLolTacticsToSnapshot(renderSnapshot, gameState, championSelections);
  }, [championSelections, gameState, renderSnapshot]);

  const userSeriesWins =
    managerTeamId && currentFixture
      ? managerTeamId === currentFixture.home_team_id
        ? seriesHomeWins
        : managerTeamId === currentFixture.away_team_id
          ? seriesAwayWins
          : 0
      : 0;

  const opponentSeriesWins =
    managerTeamId && currentFixture
      ? managerTeamId === currentFixture.home_team_id
        ? seriesAwayWins
        : managerTeamId === currentFixture.away_team_id
          ? seriesHomeWins
          : 0
      : 0;

  const blueSeriesWins =
    currentFixture && renderSnapshotWithTactics
      ? renderSnapshotWithTactics.home_team.id === currentFixture.home_team_id
        ? seriesHomeWins
        : seriesAwayWins
      : seriesHomeWins;

  const redSeriesWins =
    currentFixture && renderSnapshotWithTactics
      ? renderSnapshotWithTactics.away_team.id === currentFixture.away_team_id
        ? seriesAwayWins
        : seriesHomeWins
      : seriesAwayWins;

  const isSeriesComplete = isSupportedSeriesComplete(seriesLength, seriesHomeWins, seriesAwayWins, seriesGames);
  const canOpenPressConference = seriesLength <= 1 || isSeriesComplete;

  // Callbacks for stage transitions
  const handleStartMatch = useCallback(() => {
    console.info("[MatchSimulation] handleStartMatch");
    setStage("draft");
  }, []);

  const handleDraftComplete = useCallback(async (_payload: ChampionDraftResultPayload) => {
    console.info("[MatchSimulation] handleDraftComplete");
    let payload = _payload;
    setDraftEvaluations(null);
    if (activeSnapshot) {
      const roles = ["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"] as const;
      const inferRole = (position: string): typeof roles[number] => {
        const p = position.toLowerCase();
        if (p.includes("top")) return "TOP";
        if (p.includes("jung")) return "JUNGLE";
        if (p.includes("mid")) return "MID";
        if (p.includes("adc") || p.includes("bot") || p.includes("carry")) return "ADC";
        return "SUPPORT";
      };

      const mapSide = (
        players: MatchSnapshot["home_team"]["players"],
        picks: ChampionDraftResultPayload["blue"]["picks"],
      ): {
        champions: Record<string, string>;
        roles: Record<string, "TOP" | "JUNGLE" | "MID" | "ADC" | "SUPPORT">;
      } => {
        const champions: Record<string, string> = {};
        const roleByPlayer: Record<string, "TOP" | "JUNGLE" | "MID" | "ADC" | "SUPPORT"> = {};
        const roleOrder: Record<typeof roles[number], number> = {
          TOP: 0,
          JUNGLE: 1,
          MID: 2,
          ADC: 3,
          SUPPORT: 4,
        };
        const usedPlayerIds = new Set<string>();
        for (const role of roles) {
          const pick = picks.find((entry) => entry.role === role);
          if (!pick) continue;

          const exact = players.find(
            (entry) => !usedPlayerIds.has(entry.id) && inferRole(entry.role ?? "") === role,
          );
          const slot = players[roleOrder[role]];
          const slotCandidate = slot && !usedPlayerIds.has(slot.id) ? slot : null;
          const fallback = players.find((entry) => !usedPlayerIds.has(entry.id)) ?? players[0];
          const player = exact ?? slotCandidate ?? fallback;

          if (player) {
            usedPlayerIds.add(player.id);
            champions[player.id] = pick.championId;
            roleByPlayer[player.id] = role;
          }
        }
        return { champions, roles: roleByPlayer };
      };

      const homeDraft = mapSide(activeSnapshot.home_team.players, payload.blue.picks);
      const awayDraft = mapSide(activeSnapshot.away_team.players, payload.red.picks);

      setChampionSelections({
        home: homeDraft.champions,
        away: awayDraft.champions,
        homeRoles: homeDraft.roles,
        awayRoles: awayDraft.roles,
      });
    }
    if (activeSnapshot) {
      try {
        const inputs = draftEvaluationInputs(activeSnapshot, payload);
        const viewerTeamId = gameState?.manager.team_id ?? activeSnapshot.home_team.id;
        const viewerSide = viewerTeamId === activeSnapshot.home_team.id ? "blue" : "red";
        const canonical = await invoke<DraftStateEvaluation>("evaluate_draft_state", {
          input: {
            viewerTeamId,
            blue: { teamId: activeSnapshot.home_team.id, picks: inputs.slice(0, payload.blue.picks.length) },
            red: { teamId: activeSnapshot.away_team.id, picks: inputs.slice(payload.blue.picks.length) },
            knowledge: {
              authorizedRelationshipChampionIds: authorizedRelationshipChampionIdsForViewer(viewerSide, payload),
            },
          },
        });
        payload = { ...payload, canonical };
        const viewerIsBlue = viewerTeamId === activeSnapshot.home_team.id;
        const visibleEvaluations = canonical.pick_evaluations.flatMap((evaluation) =>
          typeof evaluation.total === "number" && typeof evaluation.engine_modifier === "number"
            ? [{ ...evaluation, meta_power: evaluation.meta_power ?? 60, total: evaluation.total, engine_modifier: evaluation.engine_modifier }]
            : [],
        );
        setDraftEvaluations(viewerIsBlue
          ? { blue: visibleEvaluations, red: [] }
          : { blue: [], red: visibleEvaluations });
      } catch (error) {
        console.error("[MatchSimulation] canonicalDraftEvaluation:failed", error);
      }
    }
    setDraftPayload(payload);
    setStage("tactics");
  }, [activeSnapshot, gameState?.manager.team_id]);

  const handleContinueFromTactics = useCallback(() => {
    console.info("[MatchSimulation] handleContinueFromTactics");
    setStage("first_half");
  }, []);

  const finalizeMatch = useCallback(async (lolReport?: LolSimV1MatchReportInput): Promise<boolean> => {
    if (hasFinalizedMatch) {
      return true;
    }

    try {
      console.info("[MatchSimulation] finalizeMatch:start");
      const response =
        await invoke<FinishLiveMatchResponse>("finish_live_match", { lolReport });
      console.info("[MatchSimulation] finalizeMatch:success", {
        hasRoundSummary: !!response.round_summary,
        hasUpdatedGame: !!response.game,
      });
      setGameState(response.game);
      const fixtureId = currentFixture?.id;
      const draft = normalizeDraftPayload(draftPayload, championSelections, renderSnapshotWithTactics ?? snapshot ?? null);
      const draftSnapshot = renderSnapshotWithTactics ?? snapshot;
      if (fixtureId && draft && draftSnapshot && lolReport?.winner) {
        const winnerTeamId = lolReport.winner === "blue"
          ? draftSnapshot.home_team.id
          : draftSnapshot.away_team.id;
        const picks = [
          ...draftPicksByPlayerRole(draftSnapshot.home_team.players, draft.blue.picks),
          ...draftPicksByPlayerRole(draftSnapshot.away_team.players, draft.red.picks),
        ];
        const updated = await invoke<GameStateData>("record_fixture_champion_picks", {
          fixtureId,
          winnerTeamId,
          picks,
          bans: [...draft.blue.bans, ...draft.red.bans],
        });
        setGameState(updated);
      }
      setHasFinalizedMatch(true);
      return true;
    } catch (err) {
      console.error("Failed to finish match:", err);
      return false;
    }
  }, [championSelections, currentFixture?.id, draftPayload, hasFinalizedMatch, renderSnapshotWithTactics, setGameState, snapshot]);

  const handleFullTime = useCallback((finalRuntimeState: LolSimV1RuntimeState, meta?: { source: "live" | "skip"; precomputedResult?: DraftMatchResult }) => {
    console.info("[MatchSimulation] handleFullTime");
    const source = meta?.source ?? "live";
    setFinalRuntimeState(finalRuntimeState);
    const mappedEvents = mapRuntimeEventsToMatchEvents(
      finalRuntimeState.events,
      blueTeamId ?? undefined,
      snapshot?.home_team.id,
    );
    setImportantEvents(mappedEvents);

    const safeDraftPayload = normalizeDraftPayload(draftPayload, championSelections, renderSnapshotWithTactics ?? null);

    const snapshotForResult = renderSnapshotWithTactics ?? snapshot;
    if (!snapshotForResult) {
      if (seriesLength > 1) {
        console.warn("[MatchSimulation] handleFullTime:blockedSeriesFinalizeWithoutSnapshot", { seriesLength });
        setStage("draft_result");
        return;
      }

      void (async () => {
        const finalized = await finalizeMatch(buildLolMatchReport(finalRuntimeState));
        if (finalized) {
          setStage("draft_result");
        }
      })();
      return;
    }

    // Determine the single source of truth for the post-match result before any
    // snapshot/screen/persistence/press updates. Live uses the runtime result;
    // skip/delegate reuse the deterministic draft simulator result. When a result
    // was already computed (delegate mode) we pass it through so we never simulate
    // twice with different seeds.
    const runtimeBasedResult = buildDraftResultFromRuntime({
      runtime: finalRuntimeState,
      snapshot: snapshotForResult,
      championSelections,
    });

    let finalResult: DraftMatchResult = runtimeBasedResult;

    if (meta?.precomputedResult) {
      finalResult = meta.precomputedResult;
      setDraftResultSimulation(meta.precomputedResult);
    } else if (source === "skip" && safeDraftPayload && renderSnapshotWithTactics && gameState) {
      try {
        const simulated = simulateDraftMatchResult({
          snapshot: renderSnapshotWithTactics,
          gameState,
          draft: safeDraftPayload,
          draftEvaluations: draftEvaluations ?? undefined,
          seedSalt: `${currentFixture?.id ?? "fixture"}-g${seriesGameIndex + 1}`,
        });
        finalResult = simulated;
        setDraftResultSimulation(simulated);
      } catch (error) {
        console.error("[MatchSimulation] draftResultFallback:failed", error);
        setDraftResultSimulation(null);
      }
    } else {
      setDraftResultSimulation(null);
    }

    // Preserve the runtime/simulated result so the press conference can use the
    // actual blue/red winner in live mode (where draftResultSimulation is null).
    setFinalDraftResult(finalResult);

    // Keep the canonical fixture home/away snapshot intact. `snapshotForResult` can be
    // side-swapped to render the user's selected LoL side, and storing that swapped
    // shape as the base snapshot corrupts subsequent home/away series win tracking.
    const merged = mergeRuntimeEventsIntoSnapshot(
      snapshot ?? snapshotForResult,
      finalRuntimeState.events,
      blueTeamId ?? undefined,
    );
    const canonicalScores = mapRuntimeWinnerToCanonicalScores({
      canonicalSnapshot: merged,
      snapshotForResult,
      winnerSide: finalResult.winnerSide,
    });
    setSnapshot({
      ...merged,
      ...canonicalScores,
    });

    const targetSeriesWins = getTargetSeriesWins(seriesLength);
    let homeSeriesWins = seriesHomeWins;
    let awaySeriesWins = seriesAwayWins;
    let userSeriesWins = 0;
    let opponentSeriesWins = 0;
    let seriesComplete = seriesLength === 1;
    let nextSeriesUsedChampionIds = seriesLength > 1 ? seriesUsedChampionIds : [];

    if (currentFixture?.id) {
      const stored = readStoredFixtureDraftResult(currentFixture.id);
      const storedHomeWins = readSeriesWins(stored?.homeSeriesWins);
      const storedAwayWins = readSeriesWins(stored?.awaySeriesWins);
      const existingHomeWins = Math.min(
        targetSeriesWins,
        Math.max(storedHomeWins, seriesHomeWins),
      );
      const existingAwayWins = Math.min(
        targetSeriesWins,
        Math.max(storedAwayWins, seriesAwayWins),
      );

      const winnerTeamId =
        finalResult.winnerSide === "blue"
          ? snapshotForResult.home_team.id
          : snapshotForResult.away_team.id;

      homeSeriesWins = Math.min(
        targetSeriesWins,
        winnerTeamId === currentFixture.home_team_id
          ? existingHomeWins + 1
          : existingHomeWins,
      );
      awaySeriesWins = Math.min(
        targetSeriesWins,
        winnerTeamId === currentFixture.away_team_id
          ? existingAwayWins + 1
          : existingAwayWins,
      );

      const pickedThisMap = [
        ...(draftPayload?.blue.picks ?? []).map((pick) => pick.championId),
        ...(draftPayload?.red.picks ?? []).map((pick) => pick.championId),
      ];
      nextSeriesUsedChampionIds = seriesLength > 1
        ? Array.from(new Set<string>([
          ...(stored?.seriesUsedChampionIds ?? []),
          ...seriesUsedChampionIds,
          ...pickedThisMap,
        ]))
        : [];

      const managerTeamId = gameState?.manager.team_id ?? null;
      userSeriesWins =
        managerTeamId === currentFixture.home_team_id
          ? homeSeriesWins
          : managerTeamId === currentFixture.away_team_id
            ? awaySeriesWins
            : 0;
      opponentSeriesWins =
        managerTeamId === currentFixture.home_team_id
          ? awaySeriesWins
          : managerTeamId === currentFixture.away_team_id
            ? homeSeriesWins
            : 0;

      setSeriesHomeWins(homeSeriesWins);
      setSeriesAwayWins(awaySeriesWins);
      setSeriesUsedChampionIds(nextSeriesUsedChampionIds);

      const nextSeriesGamesByIndex = new Map<number, StoredSeriesGameResult>();
      normalizeStoredSeriesGames(stored?.seriesGames).forEach((entry) => {
        nextSeriesGamesByIndex.set(entry.gameIndex, entry);
      });
      seriesGames.forEach((entry) => {
        nextSeriesGamesByIndex.set(entry.gameIndex, entry);
      });
      const currentSeriesGameIndex = getNextSeriesGameIndex(Array.from(nextSeriesGamesByIndex.values()));
      nextSeriesGamesByIndex.set(currentSeriesGameIndex, {
        gameIndex: currentSeriesGameIndex,
        result: finalResult,
        winnerSide: finalResult.winnerSide,
      });
      const nextSeriesGames = Array.from(nextSeriesGamesByIndex.values()).sort(
        (left, right) => left.gameIndex - right.gameIndex,
      );
      setSeriesGames(nextSeriesGames);
      setSeriesGameIndex(currentSeriesGameIndex);

      persistFixtureDraftResult(currentFixture.id, {
        snapshot: snapshotForResult,
        controlledSide: userSelectedSide,
        result: finalResult,
        seriesGames: nextSeriesGames,
        seriesLength,
        seriesGameIndex: currentSeriesGameIndex,
        userSeriesWins,
        opponentSeriesWins,
        homeSeriesWins,
        awaySeriesWins,
        seriesUsedChampionIds: nextSeriesUsedChampionIds,
      });

      seriesComplete = isSupportedSeriesComplete(seriesLength, homeSeriesWins, awaySeriesWins, nextSeriesGames);
      if (seriesComplete && currentFixture?.id) {
        clearActiveSeriesSession(currentFixture.id);
      }
    }

    const runtimeForFinalize = {
      ...finalRuntimeState,
      winner: finalResult.winnerSide,
      timeSec: finalResult.durationMinutes * 60,
      stats: {
        ...(finalRuntimeState.stats ?? {
          blue: { kills: 0, towers: 0, dragons: 0, barons: 0, gold: 0 },
          red: { kills: 0, towers: 0, dragons: 0, barons: 0, gold: 0 },
        }),
        blue: {
          ...(finalRuntimeState.stats?.blue ?? { kills: 0, towers: 0, dragons: 0, barons: 0, gold: 0 }),
          kills: finalResult.blueKills,
        },
        red: {
          ...(finalRuntimeState.stats?.red ?? { kills: 0, towers: 0, dragons: 0, barons: 0, gold: 0 }),
          kills: finalResult.redKills,
        },
      },
    };

    if (seriesComplete) {
      void (async () => {
        const finalized = await finalizeMatch(buildLolMatchReport(runtimeForFinalize));
        if (finalized) {
          setStage("draft_result");
        }
      })();
      return;
    }

    setStage("draft_result");
  }, [
    blueTeamId,
    championSelections,
    currentFixture?.id,
    currentFixture?.away_team_id,
    currentFixture?.home_team_id,
    currentFixture?.result?.away_goals,
    currentFixture?.result?.away_wins,
    currentFixture?.result?.home_goals,
    currentFixture?.result?.home_wins,
    draftEvaluations,
    draftPayload,
    finalizeMatch,
    gameState,
    renderSnapshotWithTactics,
    seriesAwayWins,
    seriesGameIndex,
    seriesGames,
    seriesHomeWins,
    seriesLength,
    seriesUsedChampionIds,
    snapshot,
    userSelectedSide,
  ]);

  const handleSimulateFromTactics = async (): Promise<void> => {
    if (!renderSnapshotWithTactics || isSimulating) {
      return;
    }

    setIsSimulating(true);
    setSimulationFeedback(
      t("match.simulatingFromTactics", { defaultValue: "Simulando la partida..." }),
    );

    const championMapByPlayerId: Record<string, string> = {
      ...(championSelections?.home ?? {}),
      ...(championSelections?.away ?? {}),
    };

    if (matchMode === "delegate") {
      try {
        const safeDraftPayload = normalizeDraftPayload(
          draftPayload,
          championSelections,
          renderSnapshotWithTactics ?? null,
        );
        if (!safeDraftPayload || !gameState) {
          throw new Error("Missing draft payload for delegate simulation");
        }

        const simulated = simulateDraftMatchResult({
          snapshot: renderSnapshotWithTactics,
          gameState,
          draft: safeDraftPayload,
          draftEvaluations: draftEvaluations ?? undefined,
          seedSalt: `${currentFixture?.id ?? "fixture"}-g${seriesGameIndex + 1}-delegate`,
        });

        setDraftResultSimulation(simulated);

        const predictiveState: LolSimV1RuntimeState = {
          timeSec: simulated.durationMinutes * 60,
          running: false,
          winner: simulated.winnerSide,
          showWalls: false,
          champions: [],
          minions: [],
          structures: [],
          objectives: createDefaultObjectivesState(),
          neutralTimers: createEmptyNeutralTimersState(),
          stats: {
            blue: { kills: simulated.blueKills, towers: 0, dragons: 0, barons: 0, gold: 0 },
            red: { kills: simulated.redKills, towers: 0, dragons: 0, barons: 0, gold: 0 },
          },
          events: [],
          speed: PARALLEL_SIM_SPEED,
        };

        handleFullTime(predictiveState, { source: "skip", precomputedResult: simulated });
      } catch (error) {
        console.error("[MatchSimulation] delegateSimulateFromTactics:failed", error);
        setSimulationFeedback(
          t("match.simulateFailed", {
            defaultValue: "No se pudo simular la partida. Volvé a intentarlo.",
          }),
        );
      } finally {
        setIsSimulating(false);
      }
      return;
    }

    try {
      const response = await lolSimV2RunToCompletion({
        seed: `post-draft-simulate-${Date.now()}`,
        aiMode: "hybrid",
        policy: simPolicy,
        snapshot: renderSnapshotWithTactics,
        championByPlayerId: championMapByPlayerId,
        championProfilesById: {},
        dtSec: PARALLEL_SIM_DT_SEC,
        speed: PARALLEL_SIM_SPEED,
        maxTicks: PARALLEL_SIM_MAX_TICKS,
      });

      const predictiveState: LolSimV1RuntimeState = {
        timeSec: response.elapsedSimulatedSec ?? 0,
        running: false,
        winner: response.winner ?? null,
        showWalls: false,
        champions: [],
        minions: [],
        structures: [],
        objectives: createDefaultObjectivesState(),
        neutralTimers: createEmptyNeutralTimersState(),
        stats: {
          blue: { kills: 0, towers: 0, dragons: 0, barons: 0, gold: 0 },
          red: { kills: 0, towers: 0, dragons: 0, barons: 0, gold: 0 },
        },
        events: [],
        speed: PARALLEL_SIM_SPEED,
      };

      handleFullTime(predictiveState, { source: "skip" });
    } catch (error) {
      console.error("[MatchSimulation] simulateFromTactics:failed", error);
      setSimulationFeedback(
        t("match.simulateFailed", {
          defaultValue: "No se pudo simular la partida. Volvé a intentarlo.",
        }),
      );
    } finally {
      setIsSimulating(false);
    }
  };

  useEffect(() => {
    if (matchMode !== "delegate") return;
    if (stage !== "tactics") return;
    if (isSimulating || delegateSimulateInFlightRef.current) return;
    if (!renderSnapshotWithTactics) return;

    const key = `${currentFixture?.id ?? "fixture"}-${seriesGameIndex}-${seriesHomeWins}-${seriesAwayWins}`;
    if (delegateAutoSimulateKeyRef.current === key) return;
    delegateAutoSimulateKeyRef.current = key;
    delegateSimulateInFlightRef.current = true;
    void handleSimulateFromTactics().finally(() => {
      delegateSimulateInFlightRef.current = false;
    });
  }, [
    currentFixture?.id,
    handleSimulateFromTactics,
    isSimulating,
    matchMode,
    renderSnapshotWithTactics,
    seriesAwayWins,
    seriesGameIndex,
    seriesHomeWins,
    stage,
  ]);

  const handlePressConference = useCallback(() => {
    console.info("[MatchSimulation] handlePressConference");
    if (!canOpenPressConference) {
      return;
    }

    setStage("press");
  }, [canOpenPressConference]);

  const handleFinishMatch = useCallback(async () => {
    console.info("[MatchSimulation] handleFinishMatch:start");
    if (seriesLength > 1 && !isSeriesComplete) {
      console.warn("[MatchSimulation] handleFinishMatch:blockedIncompleteSeries", {
        seriesAwayWins,
        seriesGameCount: seriesGames.length,
        seriesHomeWins,
        seriesLength,
      });
      return;
    }

    const finalized = await finalizeMatch();
    if (finalized) {
      navigate("/dashboard");
    }
  }, [finalizeMatch, isSeriesComplete, navigate, seriesAwayWins, seriesGames.length, seriesHomeWins, seriesLength]);

  const handleDraftResultContinue = useCallback((nextUserSide?: "blue" | "red") => {
    if (nextUserSide) {
      setUserSelectedSide(nextUserSide);
    }

    const stored = currentFixture?.id ? readStoredFixtureDraftResult(currentFixture.id) : null;
    const latestHomeWins = Math.max(seriesHomeWins, readSeriesWins(stored?.homeSeriesWins));
    const latestAwayWins = Math.max(seriesAwayWins, readSeriesWins(stored?.awaySeriesWins));
    const latestSeriesGames = normalizeStoredSeriesGames([
      ...normalizeStoredSeriesGames(stored?.seriesGames),
      ...seriesGames,
    ]);
    const seriesIsComplete = isSupportedSeriesComplete(
      seriesLength,
      latestHomeWins,
      latestAwayWins,
      latestSeriesGames,
    );

    if (!seriesIsComplete && seriesLength > 1) {
      setDraftPayload(null);
      setDraftResultSimulation(null);
      setChampionSelections(null);
      setFinalRuntimeState(null);
      setImportantEvents([]);
      setStage("draft");
      return;
    }

    void handleFinishMatch();
  }, [currentFixture?.id, handleFinishMatch, seriesAwayWins, seriesGames, seriesHomeWins, seriesLength]);

  useEffect(() => {
    if (matchMode !== "delegate") return;
    if (stage !== "draft_result") return;
    if (seriesLength <= 1 || isSeriesComplete) return;
    const key = `${currentFixture?.id ?? "fixture"}-${seriesGameIndex}-${seriesHomeWins}-${seriesAwayWins}`;
    if (delegateAutoAdvanceKeyRef.current === key) return;
    delegateAutoAdvanceKeyRef.current = key;
    handleDraftResultContinue();
  }, [
    currentFixture?.id,
    handleDraftResultContinue,
    isSeriesComplete,
    matchMode,
    seriesAwayWins,
    seriesGameIndex,
    seriesHomeWins,
    seriesLength,
    stage,
  ]);

  const handleSnapshotUpdate = useCallback((snap: MatchSnapshot) => {
    console.info("[MatchSimulation] handleSnapshotUpdate", {
      awayPlayers: snap.away_team.players.length,
      currentMinute: snap.current_minute,
      homePlayers: snap.home_team.players.length,
      phase: snap.phase,
      homeRoles: snap.home_roles,
      awayRoles: snap.away_roles,
      hasLolMap: !!snap.lol_map,
    });
    setSnapshot(snap);
  }, []);

  const handleImportantEvent = useCallback((evt: MatchEvent) => {
    console.info("[MatchSimulation] handleImportantEvent", {
      eventType: evt.event_type,
      minute: evt.minute,
      side: evt.side,
    });
    setImportantEvents((prev) => [...prev, evt]);
  }, []);

  // Loading state
  if (!snapshot || !gameState) {
    return (
      <div className="h-full bg-background flex items-center justify-center">
        <div className="flex flex-col items-center gap-3">
          <div className="size-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
          <span className="text-muted-foreground font-heading uppercase tracking-wider text-sm">
            {t("dashboard.loading")}
          </span>
        </div>
      </div>
    );
  }

  // Render the current stage
  console.debug("[MatchSimulation] render:stage", { stage, hasSnapshot: !!snapshot, hasGameState: !!gameState, userSide });
  try {
    switch (stage) {
    case "prematch":
      console.debug("[MatchSimulation] render:prematch", { userSide, currentFixture });
      return (
        <PreMatchSetup
          snapshot={snapshot}
          gameState={gameState}
          currentFixture={currentFixture}
          userSide={userSide || "Home"}
          onStart={handleStartMatch}
          onCancel={() => navigate("/dashboard")}
          onUpdateSnapshot={handleSnapshotUpdate}
        />
      );

    case "draft":
      console.debug("[MatchSimulation] render:draft", { userSide, allAi: effectiveMatchMode === "spectator" });
      return (
        <ChampionDraft
          snapshot={renderSnapshotWithTactics ?? renderSnapshot ?? snapshot}
          onComplete={handleDraftComplete}
          controlledSide={userSelectedSide}
          allAi={effectiveMatchMode === "spectator"}
          seriesLength={seriesLength}
          blueSeriesWins={blueSeriesWins}
          redSeriesWins={redSeriesWins}
          lockedChampionIds={seriesLength > 1 ? seriesUsedChampionIds : []}
          gameState={gameState}
        />
      );

    case "tactics":
      return (
        <MatchTacticsStage
          gameState={gameState}
          onGameUpdate={setGameState}
          onContinue={handleContinueFromTactics}
          onSimulate={handleSimulateFromTactics}
          isSimulating={isSimulating}
          simulationFeedback={simulationFeedback}
        />
      );

    case "draft_result":
      {
        const runtimeBasedResult = finalRuntimeState
          ? buildDraftResultFromRuntime({
            runtime: finalRuntimeState,
            snapshot: renderSnapshotWithTactics ?? snapshot,
            championSelections,
          })
          : null;
        const draftScreenResult = draftResultSimulation ?? runtimeBasedResult;

        if (draftScreenResult) {
          return (
            <DraftResultScreenV2
              snapshot={renderSnapshotWithTactics ?? snapshot}
              controlledSide={userSelectedSide}
              result={draftScreenResult}
              seriesGames={seriesGames}
              seriesLength={seriesLength}
              seriesGameIndex={Math.max(1, seriesGameIndex)}
              userSeriesWins={userSeriesWins}
              opponentSeriesWins={opponentSeriesWins}
              onPressConference={canOpenPressConference ? handlePressConference : undefined}
              onContinue={handleDraftResultContinue}
              teams={gameState.teams}
            />
          );
        }

        return (
          <LolResultScreen
            snapshot={renderSnapshotWithTactics ?? snapshot}
            gameState={gameState}
            currentFixture={currentFixture}
            userSide={userSide}
            importantEvents={importantEvents}
            finalRuntimeState={finalRuntimeState}
            onPressConference={handlePressConference}
            onFinish={handleFinishMatch}
          />
        );
      }

    case "postmatch":
      return (
        <LolResultScreen
          snapshot={snapshot}
          gameState={gameState}
          currentFixture={currentFixture}
          userSide={userSide}
          importantEvents={importantEvents}
          finalRuntimeState={finalRuntimeState}
          onPressConference={handlePressConference}
          onFinish={handleFinishMatch}
        />
      );

    case "first_half":
      return (
        <LolMatchLive
          key={stage}
          gameState={gameState}
          snapshot={renderSnapshotWithTactics ?? snapshot}
          blueTeamId={blueTeamId ?? undefined}
          championSelections={championSelections}
          onSnapshotUpdate={handleSnapshotUpdate}
          onImportantEvent={handleImportantEvent}
          onFullTime={handleFullTime}
        />
      );

    case "press":
      return (
        <PressConference
          snapshot={finalRuntimeState ? mergeRuntimeEventsIntoSnapshot(snapshot, finalRuntimeState.events, blueTeamId ?? undefined) : snapshot}
          gameState={gameState}
          userSide={userSide || "Home"}
          controlledSide={userSelectedSide}
          winnerSide={finalDraftResult?.winnerSide}
          onFinish={handleFinishMatch}
          onGameUpdate={setGameState}
        />
      );

    default:
      console.warn("[MatchSimulation] unknown stage", { stage });
      return null;
  }
  } catch (renderError) {
    console.error("[MatchSimulation] render:error", { stage, error: renderError });
    return (
      <div className="h-full bg-background flex items-center justify-center p-8">
        <div className="bg-destructive/10 border border-destructive/30 rounded-lg p-6 max-w-2xl">
          <h2 className="text-lg font-heading font-bold text-destructive mb-2">
            Render Error
          </h2>
          <p className="text-destructive/80 text-sm font-mono whitespace-pre-wrap">
            {String(renderError)}
          </p>
          <p className="text-muted-foreground text-xs mt-4">
            Stage: {stage}
          </p>
        </div>
      </div>
    );
  }
}
