import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { MatchSnapshot } from "@/ui-v2/_legacy/components/match/types";
import type { GameStateData, ScrimReportData } from "@/store/gameStore";
import { useSettingsStore } from "@/store/settingsStore";
import { getChampionTiming } from "@/lib/champions/championTiming";
import { getLolStaffEffectsForTeam } from "@/lib/teams/lolStaffEffects";
import { resolvePlayerLolRole } from "@/lib/players/lolIdentity";
import { ROLE_ICON_PATHS } from "@/lib/players/roleIcons";
import championsSeed from "../../../../../assets/simulation/champions.json";
import championListSeed from "../../../../../assets/simulation/champion-list.json";
import aiConfigSeed from "../../../../../assets/simulation/ai-config.json";
import {
  computeBanRecommendationScore as computeUnifiedBanRecommendationScore,
  rankBanCandidates,
  type BanRecommendationContext,
} from "@/ui-v2/_legacy/components/match/draftIntelHelpers";
import type { DraftPickEvaluation, DraftStateEvaluation } from "@/ui-v2/_legacy/components/match/draftResultSimulator";

type Side = "blue" | "red";
type DraftActionType = "ban" | "pick";
type Role = "TOP" | "JUNGLE" | "MID" | "ADC" | "SUPPORT";

interface ChampionData {
  id: string;
  key: number;
  name: string;
  image: string;
  tags: string[];
  roleHints: Role[];
}

interface DraftAction {
  type: DraftActionType;
  side: Side;
  label: string;
}

interface DraftPick {
  role: Role;
  championId: string;
}

interface DraftSelection {
  championId: string;
}

export interface ScrimDraftPickInput {
  championId: string;
  playerId?: string | null;
}

export interface ScrimDraftSignal {
  comfort: number;
  preparation: number;
  synergy: number;
  reasons: string[];
}

interface DraftAdviceTip {
  sourceType: "coach" | "player";
  sourceName: string;
  sourceRole?: string;
  sourceImage: string;
  type: "ban" | "pick" | "warn";
  text: string;
  champion?: ChampionData;
}

interface DraftScoreBreakdown {
  mastery: number;
  synergy: number;
  counter: number;
  comfort: number;
  preparation: number;
  total: number;
}

export interface ChampionDraftResultPayload {
  blue: {
    picks: DraftPick[];
    bans: string[];
    score: DraftScoreBreakdown;
  };
  red: {
    picks: DraftPick[];
    bans: string[];
    score: DraftScoreBreakdown;
  };
  history: string[];
  canonical?: DraftStateEvaluation;
}

/** Sends only champions already locked and visible on the opposing draft side. */
export function authorizedRelationshipChampionIdsForViewer(
  viewerSide: Side,
  draft: Pick<ChampionDraftResultPayload, "blue" | "red">,
): string[] {
  const enemyPicks = viewerSide === "blue" ? draft.red.picks : draft.blue.picks;
  return Array.from(new Set(enemyPicks.map((pick) => pick.championId)));
}

interface TeamSeed {
  id: string;
  name: string;
  shortName: string;
  logo?: string;
}

interface PlayerSeed {
  ign: string;
  teamId: string;
  rating?: number;
  role: string;
  photo?: string;
  champions: Array<Array<string | number>>;
}

type RivalMasteryKnowledgeSource = "insignia" | "scouting" | "staff";

interface RivalMasteryDisplayEntry {
  champion: ChampionData;
  mastery: number;
  playerName: string;
  playerRole: Role | null;
  source: RivalMasteryKnowledgeSource;
}

type RivalMasteryOption = Omit<RivalMasteryDisplayEntry, "source">;
type CompactDraftTier = "short" | "mid" | "tall";
type ChampionSortMode = "alpha" | "meta" | "mastery";
type MetaTierFilter = "ALL" | "S" | "A" | "B" | "C" | "D";

export function selectRivalMasteryKnowledgeForPlayer(
  allKnownOptions: RivalMasteryOption[],
  usedChampionIds: Set<string>,
  selectedChampionIds: Set<string>,
  isScouted: boolean,
): {
  knownEntries: RivalMasteryDisplayEntry[];
  staffCandidates: RivalMasteryDisplayEntry[];
} {
  const sortedOptions = allKnownOptions.slice().sort((a, b) => b.mastery - a.mastery);
  const signature = sortedOptions[0];
  const signatureChampionId = signature?.champion.id ?? null;
  const knownEntries: RivalMasteryDisplayEntry[] = [];
  const localSelectedChampionIds = new Set(selectedChampionIds);

  if (
    signature &&
    !usedChampionIds.has(signature.champion.id) &&
    !localSelectedChampionIds.has(signature.champion.id)
  ) {
    knownEntries.push({ ...signature, source: "insignia" });
    localSelectedChampionIds.add(signature.champion.id);
  }

  const availableNonSignatureOptions = sortedOptions.filter((option) => {
    if (option.champion.id === signatureChampionId) return false;
    if (localSelectedChampionIds.has(option.champion.id)) return false;
    return true;
  });

  if (isScouted) {
    const scoutedExtra = availableNonSignatureOptions.find(
      (option) => !usedChampionIds.has(option.champion.id),
    );
    if (scoutedExtra) {
      knownEntries.push({ ...scoutedExtra, source: "scouting" });
      localSelectedChampionIds.add(scoutedExtra.champion.id);
    }
  }

  const staffCandidates = availableNonSignatureOptions
    .filter((option) => !localSelectedChampionIds.has(option.champion.id))
    .map((option) => ({ ...option, source: "staff" as const }));

  return { knownEntries, staffCandidates };
}

export function calculateStaffRevealBudget(metaDiscovery: number): number {
  const normalized = Math.max(0, Math.min(1, (metaDiscovery - 0.9) / 0.3));
  return Math.max(1, Math.min(5, 1 + Math.round(normalized * 4)));
}

function calculateCounterRevealBudget(metaDiscovery: number): number {
  if (metaDiscovery < 1.0) return 2;
  if (metaDiscovery < 1.1) return 3;
  return 4;
}

function calculateCounterConsultCharges(metaDiscovery: number): number {
  return metaDiscovery >= 1.05 ? 2 : 1;
}

export function selectStaffRevealEntries(
  candidates: RivalMasteryDisplayEntry[],
  budget: number,
  usedChampionIds: Set<string>,
): RivalMasteryDisplayEntry[] {
  return candidates
    .slice()
    .sort((a, b) => b.mastery - a.mastery)
    .slice(0, budget)
    .filter((candidate) => !usedChampionIds.has(candidate.champion.id));
}

interface ChampionsSeed {
  data?: {
    roles?: Record<string, string[]>;
    counterpicks?: Array<{ a: string; b: string; value: number }>;
  };
}

interface DraftAiConfigSeed {
  data?: {
    pick?: {
      masteryWeight?: number;
      metaWeight?: number;
      counterAdvantageWeight?: number;
      counterRiskWeight?: number;
    };
    ban?: {
      enemyMasteryWeight?: number;
      metaWeight?: number;
    };
    score?: {
      counterAdvantageWeight?: number;
      counterRiskWeight?: number;
    };
    timing?: {
      userTurnSeconds?: number;
      aiMinSeconds?: number;
      aiMaxSeconds?: number;
    };
  };
}

interface ChampionDraftProps {
  snapshot: MatchSnapshot;
  onComplete: (result: ChampionDraftResultPayload) => void;
  controlledSide?: Side;
  allAi?: boolean;
  seriesLength?: 1 | 3 | 5;
  blueSeriesWins?: number;
  redSeriesWins?: number;
  lockedChampionIds?: string[];
  gameState?: GameStateData;
}


// This should be put in another place. Cruncky to edit and test. Should we get real mastery and meta scores? This will be removed anyway.
const META_CHAMPION_SCORES: Record<string, number> = {
  ahri: 18,
  ambessa: 20,
  ashe: 12,
  aurora: 17,
  azir: 18,
  camille: 16,
  corki: 14,
  elise: 12,
  ezreal: 13,
  gragas: 15,
  hwei: 17,
  jayce: 16,
  jhin: 13,
  jinx: 15,
  kalista: 14,
  kaisa: 16,
  ksante: 17,
  leblanc: 14,
  leesin: 12,
  lissandra: 13,
  lucian: 15,
  nautilus: 14,
  nidalee: 12,
  orianna: 15,
  poppy: 15,
  rell: 14,
  renekton: 14,
  rumble: 18,
  sejuani: 16,
  skarner: 17,
  smolder: 13,
  sylas: 17,
  taliyah: 14,
  varus: 16,
  vi: 14,
  xayah: 13,
  yone: 14,
};

const ROLE_ORDER: Role[] = ["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"];
const ASSISTANT_COACH_PLACEHOLDER = "";
const EMPTY_LOCKED_CHAMPION_IDS: string[] = [];
const ROLE_ICON_URLS: Record<Role, string> = ROLE_ICON_PATHS;

interface DraftEvaluationRequest {
  playerId: string;
  championId: string;
  effectiveRole: Role;
}

function draftEvaluationKey(request: DraftEvaluationRequest): string {
  return `${request.playerId}:${request.championId}:${request.effectiveRole}`;
}

export function isCurrentDraftEvaluationRequest(
  requestId: number,
  activeRequestId: number,
  requestKey: string,
  activeRequestKey: string,
): boolean {
  return requestId === activeRequestId && requestKey === activeRequestKey;
}

export function canUserConfirmDraftChoice(
  pendingChampionId: string | null,
  isUserTurn: boolean,
): boolean {
  return isUserTurn && pendingChampionId !== null;
}

const DRAFT_SEQUENCE: DraftAction[] = [
  { type: "ban", side: "blue", label: "B1" },
  { type: "ban", side: "red", label: "R1" },
  { type: "ban", side: "blue", label: "B2" },
  { type: "ban", side: "red", label: "R2" },
  { type: "ban", side: "blue", label: "B3" },
  { type: "ban", side: "red", label: "R3" },
  { type: "pick", side: "blue", label: "B1" },
  { type: "pick", side: "red", label: "R1" },
  { type: "pick", side: "red", label: "R2" },
  { type: "pick", side: "blue", label: "B2" },
  { type: "pick", side: "blue", label: "B3" },
  { type: "pick", side: "red", label: "R3" },
  { type: "ban", side: "blue", label: "B4" },
  { type: "ban", side: "red", label: "R4" },
  { type: "ban", side: "blue", label: "B5" },
  { type: "ban", side: "red", label: "R5" },
  { type: "pick", side: "red", label: "R4" },
  { type: "pick", side: "blue", label: "B4" },
  { type: "pick", side: "blue", label: "B5" },
  { type: "pick", side: "red", label: "R5" },
];

const COACH_BAN_PHRASES: string[] = [
  "If we leave {{champion}} open, {{player}} punishes us: {{mastery}}% mastery.",
  "{{champion}} is pure comfort for {{player}} ({{mastery}}%). I would remove it in bans.",
  "Ban priority: {{champion}}. {{player}} plays it at a {{mastery}}% level.",
  "Do not give away {{champion}}; {{player}} already showed {{mastery}}% on that pick.",
  "Early ban on {{champion}}. That matchup is dangerous in {{player}}'s hands ({{mastery}}%).",
  "I am worried about {{champion}}: {{player}} looks very comfortable ({{mastery}}%).",
];

const PLAYER_COUNTER_PICK_PHRASES: string[] = [
  "Pick me {{champion}}. I win the matchup into {{enemy}}.",
  "Give me {{champion}}; I can break lane against {{enemy}}.",
  "With {{champion}} I get real advantage over {{enemy}}. Trust me.",
  "{{champion}} is the right answer into {{enemy}}. Leave it to me.",
  "If they go {{enemy}}, I want {{champion}}: I studied that lane.",
  "Comfort plus counter pick: {{champion}} into {{enemy}}. This is the moment.",
];

const PLAYER_SMART_COUNTER_PICK_PHRASES: string[] = [
  "Please pick {{champion}} for me; with my game read I can break this matchup into {{enemy}}.",
  "With {{champion}} I can outplay {{enemy}} from wave 1. Give me that pick.",
  "{{enemy}} gets exposed if you give me {{champion}}. I can snowball it.",
  "I have perfect timing to punish {{enemy}} with {{champion}}.",
  "Trust me: {{champion}} vs {{enemy}} is technical advantage for us.",
  "This matchup heavily favors me: {{champion}} into {{enemy}}. I want to play it.",
];

const PLAYER_COMFORT_PICK_PHRASES: string[] = [
  "I feel very solid on {{champion}} ({{mastery}}% mastery).",
  "If you give me {{champion}}, I can guarantee consistency ({{mastery}}%).",
  "{{champion}} is my most reliable pick right now ({{mastery}}%).",
  "I am comfortable carrying with {{champion}} ({{mastery}}% mastery).",
  "{{champion}} gives me early impact and control ({{mastery}}%).",
  "I want {{champion}}; I practiced it a lot ({{mastery}}%).",
];

const PLAYER_BAN_REQUEST_PHRASES: string[] = [
  "Can we ban {{threat}}? It is one of the few picks that shuts down my {{champion}}.",
  "If we remove {{threat}}, my {{champion}} becomes much freer.",
  "I need {{threat}} out; it hurts the plan too much with {{champion}}.",
  "Ban {{threat}} and I can play {{champion}} with much more pressure.",
  "{{threat}} is a hard counter to {{champion}}. Better remove it now.",
  "If you want me on {{champion}}, the best move is removing {{threat}}.",
];

function pickPhrase(
  pool: string[],
  seed: string,
  shift = 0,
): { index: number; template: string } {
  if (pool.length === 0) return { index: 0, template: "" };
  const index = (hashText(seed) + shift) % pool.length;
  return {
    index,
    template: pool[index] ?? pool[0] ?? "",
  };
}

function inferRoleHints(tags: string[]): Role[] {
  const set = new Set<Role>();
  if (tags.includes("Support")) set.add("SUPPORT");
  if (tags.includes("Marksman")) set.add("ADC");
  if (tags.includes("Mage")) set.add("MID");
  if (tags.includes("Assassin")) {
    set.add("MID");
    set.add("JUNGLE");
  }
  if (tags.includes("Tank") || tags.includes("Fighter")) {
    set.add("TOP");
    set.add("JUNGLE");
  }
  if (set.size === 0) set.add("MID");
  return [...set];
}

function splashUrl(championId: string): string {
  return `/champion-tiles/${championId}.webp`;
}

function loadingUrl(championId: string): string {
  return `/champion-loading/${championId}.webp`;
}

const TEAM_BRAND_MAP: Record<string, { tricode: string; logo: string | null }> = {
  "g2 esports": { tricode: "G2", logo: "/teams-icons/g2-esports.webp" },
  fnatic: { tricode: "FNC", logo: "/teams-icons/fnatic.webp" },
  "team vitality": { tricode: "VIT", logo: "/teams-icons/team-vitality.webp" },
  vitality: { tricode: "VIT", logo: "/teams-icons/team-vitality.webp" },
  "team heretics": { tricode: "HRTS", logo: "/teams-icons/team-heretics-lec.webp" },
  "sk gaming": { tricode: "SK", logo: "/teams-icons/sk-gaming.webp" },
  "movistar koi": { tricode: "MKOI", logo: "/teams-icons/mad-lions.webp" },
  "mad lions koi": { tricode: "MKOI", logo: "/teams-icons/mad-lions.webp" },
  "team bds": { tricode: "SHFT", logo: "/teams-icons/shifters.webp" },
  giantx: { tricode: "GX", logo: "/teams-icons/giantx-lec.webp" },
  heretics: { tricode: "HRTS", logo: "/teams-icons/team-heretics-lec.webp" },
  shifters: { tricode: "SHFT", logo: "/teams-icons/shifters.webp" },
  "natus vincere": { tricode: "NAVI", logo: "/teams-icons/natus-vincere.webp" },
  "karmine corp": { tricode: "KC", logo: "/teams-icons/karmine-corp.webp" },
};

const TEAM_SEEDS: TeamSeed[] = [];
const PLAYER_SEEDS: PlayerSeed[] = [];
const CHAMPIONS_SEED: ChampionsSeed = championsSeed as ChampionsSeed;
const AI_CONFIG_SEED: DraftAiConfigSeed = aiConfigSeed as DraftAiConfigSeed;

function numberOrDefault(value: number | undefined, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

const AI_WEIGHTS = {
  pick: {
    masteryWeight: numberOrDefault(AI_CONFIG_SEED.data?.pick?.masteryWeight, 1.15),
    metaWeight: numberOrDefault(AI_CONFIG_SEED.data?.pick?.metaWeight, 1.1),
    counterAdvantageWeight: numberOrDefault(AI_CONFIG_SEED.data?.pick?.counterAdvantageWeight, 4),
    counterRiskWeight: numberOrDefault(AI_CONFIG_SEED.data?.pick?.counterRiskWeight, 3),
  },
  score: {
    counterAdvantageWeight: numberOrDefault(AI_CONFIG_SEED.data?.score?.counterAdvantageWeight, 2),
    counterRiskWeight: numberOrDefault(AI_CONFIG_SEED.data?.score?.counterRiskWeight, 2),
  },
};

const AI_TIMING = {
  userTurnMs: numberOrDefault(AI_CONFIG_SEED.data?.timing?.userTurnSeconds, 30) * 1000,
  aiMinMs: numberOrDefault(AI_CONFIG_SEED.data?.timing?.aiMinSeconds, 3) * 1000,
  aiMaxMs: numberOrDefault(AI_CONFIG_SEED.data?.timing?.aiMaxSeconds, 10) * 1000,
};

function normalizeKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function mapSeedRoleToDraftRole(role: string): Role | null {
  const key = normalizeKey(role);
  if (key === "top") return "TOP";
  if (key === "jungle") return "JUNGLE";
  if (key === "mid" || key === "middle") return "MID";
  if (key === "bot" || key === "bottom" || key === "adc") return "ADC";
  if (key === "support" || key === "sup") return "SUPPORT";
  return null;
}

function mapSnapshotPositionToDraftRole(role: string): Role {
  // Handle PascalCase engine roles (Top, Jungle, Mid, Adc, Support) directly
  const engineKey = role.toLowerCase().replace(/[^a-z]/g, "");
  if (engineKey === "top") return "TOP";
  if (engineKey === "jungle") return "JUNGLE";
  if (engineKey === "mid") return "MID";
  if (engineKey === "adc") return "ADC";
  if (engineKey === "support") return "SUPPORT";

  // Fallback: map legacy positions to LoL roles
  const key = normalizeKey(role);
  if (key.includes("top") || key === "defender") return "TOP";
  if (key.includes("jung") || key === "midfielder" || key === "centralmidfielder") return "JUNGLE";
  if (key.includes("attackingmidfielder") || key === "mid") return "MID";
  if (key.includes("adc") || key.includes("bot") || key === "forward" || key === "striker") return "ADC";
  return "SUPPORT";
}

function roleOrderedSnapshotPlayersWithResolver<T extends { role?: string; id: string; name?: string }>(
  players: T[],
  resolveRole: (player: T) => Role,
): T[] {
  const byRole = new Map<Role, T>();
  const used = new Set<string>();

  for (const role of ROLE_ORDER) {
    const player = players.find(
      (candidate) => !used.has(candidate.id) && resolveRole(candidate) === role,
    );
    if (!player) continue;
    byRole.set(role, player);
    used.add(player.id);
  }

  const remainder = players.filter((candidate) => !used.has(candidate.id));
  const ordered = ROLE_ORDER.map((role) => byRole.get(role)).filter((value): value is T => !!value);
  return [...ordered, ...remainder].slice(0, 5);
}

function tierToMetaScore(tier: string): number {
  const normalized = tier.toUpperCase();
  if (normalized === "S") return 20;
  if (normalized === "A") return 17;
  if (normalized === "B") return 14;
  if (normalized === "C") return 10;
  if (normalized === "D") return 7;
  return 12;
}

function metaScoreToTier(score: number): Exclude<MetaTierFilter, "ALL"> {
  if (score >= 19) return "S";
  if (score >= 16) return "A";
  if (score >= 13) return "B";
  if (score >= 9) return "C";
  return "D";
}

function masteryBarTone(mastery: number): "gold" | "green" | "red" {
  if (mastery >= 90) return "gold";
  if (mastery >= 55) return "green";
  return "red";
}

export function computeBanRecommendationScore(context: BanRecommendationContext): number {
  return computeUnifiedBanRecommendationScore(context);
}

function knownMetaTierForChampion(
  champion: ChampionData,
  runtimeMetaScoreByChampion: Map<string, number>,
  discoveredMetaChampionIds: Set<string>,
): Exclude<MetaTierFilter, "ALL"> | "?" {
  const normalizedChampionId = normalizeKey(champion.id);
  if (!discoveredMetaChampionIds.has(normalizedChampionId)) {
    return "?";
  }
  const runtime = runtimeMetaScoreByChampion.get(normalizedChampionId);
  if (typeof runtime === "number") {
    return metaScoreToTier(runtime);
  }
  return "?";
}

const CHAMPION_ROLE_HINTS = new Map<string, Role[]>();
Object.entries(CHAMPIONS_SEED.data?.roles ?? {}).forEach(([championName, roles]) => {
  const mappedRoles = (roles ?? [])
    .map((role) => mapSeedRoleToDraftRole(role))
    .filter((role): role is Role => role !== null);

  if (mappedRoles.length > 0) {
    const unique = Array.from(new Set(mappedRoles));
    CHAMPION_ROLE_HINTS.set(normalizeKey(championName), unique);
  }
});

const CHAMPION_COUNTER_VALUES = new Map<string, number>();
(CHAMPIONS_SEED.data?.counterpicks ?? []).forEach(({ a, b, value }) => {
  const left = normalizeKey(a);
  const right = normalizeKey(b);
  if (!left || !right) return;
  CHAMPION_COUNTER_VALUES.set(`${left}::${right}`, Number(value) || 0);
});

function inferRoleHintsFromSeed(championId: string, championName: string, tags: string[]): Role[] {
  const seedRolesById = CHAMPION_ROLE_HINTS.get(normalizeKey(championId));
  if (seedRolesById && seedRolesById.length > 0) return seedRolesById;

  const seedRolesByName = CHAMPION_ROLE_HINTS.get(normalizeKey(championName));
  if (seedRolesByName && seedRolesByName.length > 0) return seedRolesByName;

  return inferRoleHints(tags);
}

function hashText(value: string): number {
  let hash = 0;
  for (let i = 0; i < value.length; i += 1) {
    hash = (hash * 31 + value.charCodeAt(i)) >>> 0;
  }
  return hash;
}

function reportTimestamp(report: ScrimReportData): number {
  const raw = report.created_on || report.date;
  const parsed = Date.parse(raw);
  return Number.isFinite(parsed) ? parsed : 0;
}

export function calculateScrimDraftSignal(
  reports: ScrimReportData[],
  teamId: string,
  upcomingOpponentTeamId: string,
  picks: ScrimDraftPickInput[],
): ScrimDraftSignal {
  const playedReports = reports
    .filter((report) => report.team_id === teamId && report.status === "Played")
    .slice()
    .sort((left, right) => reportTimestamp(right) - reportTimestamp(left))
    .slice(0, 8);

  if (playedReports.length === 0 || picks.length === 0) {
    return { comfort: 0, preparation: 0, synergy: 0, reasons: [] };
  }

  let comfort = 0;
  let preparation = 0;
  let synergy = 0;
  const reasons = new Set<string>();
  const pickedChampionKeys = new Set(picks.map((pick) => normalizeKey(pick.championId)));

  picks.forEach((pick) => {
    const championKey = normalizeKey(pick.championId);
    if (!championKey) return;

    const practicedBySamePlayer = playedReports.some((report) =>
      report.player_champion_picks.some((scrimPick) => {
        if (normalizeKey(scrimPick.champion_id) !== championKey) return false;
        return pick.playerId ? scrimPick.player_id === pick.playerId : true;
      }),
    );

    if (practicedBySamePlayer) {
      comfort += 1;
      reasons.add("recent champion reps");
    }
  });

  playedReports.forEach((report) => {
    const practicedChampionKeys = new Set(
      report.player_champion_picks.map((pick) => normalizeKey(pick.champion_id)),
    );
    const overlap = Array.from(pickedChampionKeys).filter((championKey) =>
      practicedChampionKeys.has(championKey),
    ).length;

    if (overlap >= 2) {
      synergy += overlap >= 4 ? 2 : 1;
      reasons.add("scrimmed core together");
    }

    if (report.opponent_team_id === upcomingOpponentTeamId) {
      preparation += report.focus === "DraftPrep" || report.post_decision === "VodReview" ? 2 : 1;
      reasons.add("recent prep vs this opponent");
    }
  });

  return {
    comfort: Math.min(4, comfort),
    preparation: Math.min(3, preparation),
    synergy: Math.min(4, synergy),
    reasons: Array.from(reasons),
  };
}

function counterValue(allyChampionId: string, enemyChampionId: string): number {
  const key = `${normalizeKey(allyChampionId)}::${normalizeKey(enemyChampionId)}`;
  return CHAMPION_COUNTER_VALUES.get(key) ?? 0;
}

function teamTriCode(name: string): string {
  const normalizedName = normalizeKey(name);
  const fromSeed = TEAM_SEEDS.find((team) => normalizeKey(team.name) === normalizedName);
  if (fromSeed?.shortName) return fromSeed.shortName.toUpperCase();

  const key = name.trim().toLowerCase();
  const known = TEAM_BRAND_MAP[key];
  if (known) return known.tricode;

  const cleaned = name.replace(/[^A-Za-z0-9\s]/g, " ").trim();
  if (!cleaned) return "TEAM";
  const words = cleaned.split(/\s+/).filter(Boolean);
  if (words.length >= 2) return words.map((word) => word[0]).join("").toUpperCase().slice(0, 4);
  return cleaned.slice(0, 4).toUpperCase();
}

function teamLogo(name: string, teams?: import("@/store/types").TeamData[]): string | null {
  const normalizedName = normalizeKey(name);
  if (teams) {
    const fromTeams = teams.find((t) => normalizeKey(t.name) === normalizedName);
    if (fromTeams?.logo_url) return fromTeams.logo_url;
  }

  const key = name.trim().toLowerCase();
  const known = TEAM_BRAND_MAP[key]?.logo;
  if (known) return known;

  const fromSeed = TEAM_SEEDS.find((team) => normalizeKey(team.name) === normalizedName);
  if (fromSeed?.logo) {
    const logoFileName = fromSeed.logo.split("/").pop();
    const fallback = logoFileName ? `/teams-icons/${logoFileName.toLowerCase()}` : null;
    return fallback;
  }

  return null;
}

function tricodeSizeClass(code: string): string {
  if (code.length >= 4) return "text-2xl";
  if (code.length === 3) return "text-3xl";
  return "text-4xl";
}

function playerSeedPhotoUrl(photo?: string): string | null {
  if (!photo) return null;
  if (photo.startsWith("/images/")) return `/data/lec${photo}`;
  return photo;
}

export default function ChampionDraft({
  snapshot,
  onComplete,
  controlledSide = "blue",
  allAi = false,
  seriesLength = 1,
  blueSeriesWins = 0,
  redSeriesWins = 0,
  lockedChampionIds = EMPTY_LOCKED_CHAMPION_IDS,
  gameState,
}: ChampionDraftProps) {
  const { t } = useTranslation();

  const leagueLogo = gameState?.user_competition_id
    ? gameState.leagues.find((l) => l.competition_id === gameState.user_competition_id)?.logo
      ?? `/competitions-icons/${gameState.user_competition_id}.webp`
    : "/competitions-icons/lec.webp";

  const debugToolsEnabled = useSettingsStore(
    (state) => state.settings.debug_tools_enabled,
  );
  const [champions, setChampions] = useState<ChampionData[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [roleFilter, setRoleFilter] = useState<"ALL" | Role>("ALL");
  const [sortMode, setSortMode] = useState<ChampionSortMode>("alpha");
  const [metaTierFilter, setMetaTierFilter] = useState<MetaTierFilter>("ALL");
  const [searchTerm, setSearchTerm] = useState("");
  const [stepIndex, setStepIndex] = useState(0);
  const [blueBans, setBlueBans] = useState<string[]>([]);
  const [redBans, setRedBans] = useState<string[]>([]);
  const [bluePicks, setBluePicks] = useState<DraftSelection[]>([]);
  const [redPicks, setRedPicks] = useState<DraftSelection[]>([]);
  const [blueRoleOrder, setBlueRoleOrder] = useState<number[] | null>(null);
  const [redRoleOrder, setRedRoleOrder] = useState<number[] | null>(null);
  const [pendingChampionId, setPendingChampionId] = useState<string | null>(null);
  const [swapSource, setSwapSource] = useState<{ side: Side; index: number } | null>(null);
  const [draftHistory, setDraftHistory] = useState<string[]>([]);
  const [turnDurationMs, setTurnDurationMs] = useState<number>(AI_TIMING.userTurnMs);
  const [turnStartedAt, setTurnStartedAt] = useState<number>(() => Date.now());
  const [turnRemainingMs, setTurnRemainingMs] = useState<number>(AI_TIMING.userTurnMs);
  const [showFinalRoleReassignFx, setShowFinalRoleReassignFx] = useState(false);
  const [counterConsultUsesLeft, setCounterConsultUsesLeft] = useState<number>(1);
  const [consultedCounterChampionIds, setConsultedCounterChampionIds] = useState<Set<string>>(() => new Set());
  const autoResolvedStepKeyRef = useRef<string | null>(null);
  const finalRoleReassignFxPlayedRef = useRef(false);
  const draftEvaluationCacheRef = useRef(new Map<string, DraftPickEvaluation>());
  const draftEvaluationRequestIdRef = useRef(0);
  const draftEvaluationRequestKeyRef = useRef("");
  const [draftEvaluationVersion, setDraftEvaluationVersion] = useState(0);

  const bluePlayerIds = useMemo(
    () => snapshot.home_team.players.map((player) => player.id),
    [snapshot.home_team.players],
  );
  const redPlayerIds = useMemo(
    () => snapshot.away_team.players.map((player) => player.id),
    [snapshot.away_team.players],
  );
  const userTeamId = controlledSide === "blue" ? snapshot.home_team.id : snapshot.away_team.id;
  const userStaffEffects = getLolStaffEffectsForTeam(gameState, userTeamId);

  useEffect(() => {
    setCounterConsultUsesLeft(calculateCounterConsultCharges(userStaffEffects.metaDiscovery));
    setConsultedCounterChampionIds(new Set());
  }, [userStaffEffects.metaDiscovery, userTeamId]);

  useEffect(() => {
    try {
      const data = championListSeed as { champions: Array<{ id: string; key: number; name: string; tags: string[]; image: string }> };
      const list = (data.champions ?? [])
        .map((champion) => ({
          id: champion.id,
          key: champion.key,
          name: champion.name,
          image: `/champion-tiles/${champion.id}.webp`,
          tags: champion.tags,
          roleHints: inferRoleHintsFromSeed(champion.id, champion.name, champion.tags),
        }))
        .sort((a, b) => a.name.localeCompare(b.name));

      setChampions(list);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const currentStep = DRAFT_SEQUENCE[stepIndex] ?? null;

  const usedChampionIds = useMemo(() => {
    const set = new Set<string>();
    lockedChampionIds.forEach((championId) => set.add(championId));
    [...blueBans, ...redBans].forEach((championId) => set.add(championId));
    [...bluePicks, ...redPicks].forEach((pick) => set.add(pick.championId));
    return set;
  }, [blueBans, bluePicks, lockedChampionIds, redBans, redPicks]);

  const championById = useMemo(() => {
    const map = new Map<string, ChampionData>();
    champions.forEach((champion) => map.set(champion.id, champion));
    return map;
  }, [champions]);

  const seriesLockedChampions = useMemo(() => {
    return lockedChampionIds
      .map((championId) => championById.get(championId) ?? null)
      .filter((champion): champion is ChampionData => champion !== null);
  }, [championById, lockedChampionIds]);

  const finished = stepIndex >= DRAFT_SEQUENCE.length;

  const handleSelectChampion = (champion: ChampionData, actor: "user" | "ai" = "user") => {
    if (!currentStep || finished) return;
    if (actor === "user" && (allAi || currentStep.side !== controlledSide)) return;

    if (currentStep.type === "ban") {
      if (currentStep.side === "blue") {
        setBlueBans((prev) => [...prev, champion.id]);
      } else {
        setRedBans((prev) => [...prev, champion.id]);
      }
      setDraftHistory((prev) => [...prev, champion.id]);
      setStepIndex((prev) => prev + 1);
      return;
    }

    if (currentStep.side === "blue") {
      setBluePicks((prev) => [...prev, { championId: champion.id }]);
      setBlueRoleOrder(null);
    } else {
      setRedPicks((prev) => [...prev, { championId: champion.id }]);
      setRedRoleOrder(null);
    }
    setDraftHistory((prev) => [...prev, champion.id]);
    setStepIndex((prev) => prev + 1);
    setPendingChampionId(null);
  };

  const handleChampionTileClick = (champion: ChampionData): void => {
    if (!currentStep || finished || !isUserTurn) return;
    if (usedChampionIds.has(champion.id)) return;
    setPendingChampionId((prev) => (prev === champion.id ? null : champion.id));
  };

  const handleConfirmPendingAction = (): void => {
    if (!pendingChampionId) return;
    const champion = championById.get(pendingChampionId);
    if (!champion) return;
    handleSelectChampion(champion, "user");
  };

  const handleConsultStaffCounterIntel = (): void => {
    if (!pendingChampionId || counterConsultUsesLeft <= 0) return;
    setConsultedCounterChampionIds((prev) => {
      const next = new Set(prev);
      if (next.has(pendingChampionId)) return prev;
      next.add(pendingChampionId);
      return next;
    });
    setCounterConsultUsesLeft((prev) => Math.max(0, prev - 1));
  };

  const isUserTurn = !!currentStep && !allAi && currentStep.side === controlledSide && !finished;

  const totalSteps = DRAFT_SEQUENCE.length;
  const currentStepNumber = Math.min(stepIndex + 1, totalSteps);
  const actionLabel =
    currentStep?.type === "ban"
      ? t("match.draft.actions.ban")
      : t("match.draft.actions.pick");
  const sideLabel =
    currentStep?.side === "blue"
      ? t("match.draft.sides.blue")
      : t("match.draft.sides.red");

  const bluePlayers = useMemo(
    () => {
      const homeSeed = TEAM_SEEDS.find(
        (team) => normalizeKey(team.name) === normalizeKey(snapshot.home_team.name),
      );
      const homeSeedByIgn = new Map(
        PLAYER_SEEDS
          .filter((entry) => entry.teamId === homeSeed?.id)
          .map((entry) => [normalizeKey(entry.ign), entry]),
      );

      return roleOrderedSnapshotPlayersWithResolver(snapshot.home_team.players, (player) => {
        const fromState = gameState?.players.find((candidate) => candidate.id === player.id);
        if (fromState) {
          const role = resolvePlayerLolRole(fromState) as Role;
          console.debug("[ChampionDraft] resolve:fromState", { playerId: player.id, name: player.name, naturalPosition: fromState.natural_position, role, fromStateId: fromState.id, snapRole: player.role });
          return role;
        }

        const fromSeed = homeSeedByIgn.get(normalizeKey((player as { name?: string }).name ?? ""));
        const mappedSeedRole = fromSeed ? mapSeedRoleToDraftRole(String(fromSeed.role ?? "")) : null;
        if (mappedSeedRole) {
          console.debug("[ChampionDraft] resolve:fromSeed", { playerId: player.id, name: player.name, seedRole: fromSeed?.role, mappedRole: mappedSeedRole });
          return mappedSeedRole;
        }

        const fallbackRole = mapSnapshotPositionToDraftRole(player.role ?? "");
        console.debug("[ChampionDraft] resolve:fallback", { playerId: player.id, name: player.name, engineRole: player.role, fallbackRole });
        return fallbackRole;
      });
    },
    [gameState?.players, snapshot.home_team.name, snapshot.home_team.players],
  );
  const redPlayers = useMemo(
    () => {
      const awaySeed = TEAM_SEEDS.find(
        (team) => normalizeKey(team.name) === normalizeKey(snapshot.away_team.name),
      );
      const awaySeedByIgn = new Map(
        PLAYER_SEEDS
          .filter((entry) => entry.teamId === awaySeed?.id)
          .map((entry) => [normalizeKey(entry.ign), entry]),
      );

      return roleOrderedSnapshotPlayersWithResolver(snapshot.away_team.players, (player) => {
        const fromState = gameState?.players.find((candidate) => candidate.id === player.id);
        if (fromState) return resolvePlayerLolRole(fromState) as Role;

        const fromSeed = awaySeedByIgn.get(normalizeKey((player as { name?: string }).name ?? ""));
        const mappedSeedRole = fromSeed ? mapSeedRoleToDraftRole(String(fromSeed.role ?? "")) : null;
        if (mappedSeedRole) return mappedSeedRole;

        return mapSnapshotPositionToDraftRole(player.role ?? "");
      });
    },
    [gameState?.players, snapshot.away_team.name, snapshot.away_team.players],
  );
  const bluePlayerLabels = useMemo(
    () => bluePlayers.map((player) => player.name.toUpperCase()),
    [bluePlayers],
  );
  const redPlayerLabels = useMemo(
    () => redPlayers.map((player) => player.name.toUpperCase()),
    [redPlayers],
  );

  const blueHeader = "";
  const redHeader = "";

  const roleMasteryForChampion = (side: Side, role: Role, championId: string): number => {
    const roleIndex = ROLE_ORDER.indexOf(role);
    if (roleIndex < 0) return 25;
    return resolvePlayerMastery(side, roleIndex, championId);
  };

  const displayMasteryForChampion = (champion: ChampionData): number => {
    const sampleSide = currentStep?.side ?? controlledSide;
    if (roleFilter !== "ALL") {
      return roleMasteryForChampion(sampleSide, roleFilter, champion.id);
    }
    const samplePickIndex = sampleSide === "blue" ? bluePicks.length : redPicks.length;
    return resolvePlayerMastery(sampleSide, samplePickIndex, champion.id);
  };

  const roleAssignmentScore = (side: Side, role: Role, championId: string): number => {
    const champion = championById.get(championId);
    const roleBonus = champion?.roleHints.includes(role) ? 20 : 0;
    return roleMasteryForChampion(side, role, championId) + roleBonus;
  };

  const buildBestRoleOrder = (side: Side, selections: DraftSelection[]): number[] => {
    const availableIndices = selections.map((_, idx) => idx);
    if (availableIndices.length <= 1) return availableIndices;

    const roleSubset = ROLE_ORDER.slice(0, availableIndices.length);
    let bestOrder = [...availableIndices];
    let bestScore = Number.NEGATIVE_INFINITY;

    const permute = (arr: number[], l: number): void => {
      if (l === arr.length) {
        const score = arr.reduce((sum, idx, roleIdx) => {
          const role = roleSubset[roleIdx];
          if (!role) return sum;
          return sum + roleAssignmentScore(side, role, selections[idx].championId);
        }, 0);
        if (score > bestScore) {
          bestScore = score;
          bestOrder = [...arr];
        }
        return;
      }

      for (let i = l; i < arr.length; i += 1) {
        [arr[l], arr[i]] = [arr[i], arr[l]];
        permute(arr, l + 1);
        [arr[l], arr[i]] = [arr[i], arr[l]];
      }
    };

    permute([...availableIndices], 0);
    return bestOrder;
  };

  const championAssignableRoles = (championId: string): Role[] => {
    const hints = championById.get(championId)?.roleHints ?? [];
    return hints.length > 0 ? hints : ROLE_ORDER;
  };

  const canAssignUniqueRoles = (selections: DraftSelection[]): boolean => {
    if (selections.length === 0) return true;
    if (selections.length > ROLE_ORDER.length) return false;

    const options = selections
      .map((selection) => championAssignableRoles(selection.championId))
      .sort((left, right) => left.length - right.length);

    const search = (index: number, used: Set<Role>): boolean => {
      if (index >= options.length) return true;
      const roles = options[index] ?? [];
      for (const role of roles) {
        if (used.has(role)) continue;
        used.add(role);
        if (search(index + 1, used)) return true;
        used.delete(role);
      }
      return false;
    };

    return search(0, new Set<Role>());
  };

  const assignedRolesForSelections = (selections: DraftSelection[]): Set<Role> => {
    const result = new Set<Role>();
    const search = (index: number, used: Set<Role>): boolean => {
      if (index >= selections.length) {
        used.forEach((role) => result.add(role));
        return true;
      }
      const roles = championAssignableRoles(selections[index].championId);
      for (const role of roles) {
        if (used.has(role)) continue;
        used.add(role);
        if (search(index + 1, used)) return true;
        used.delete(role);
      }
      return false;
    };

    search(0, new Set<Role>());
    return result;
  };

  const currentPickEvaluationContext = useMemo((): Omit<DraftEvaluationRequest, "championId"> | null => {
    if (currentStep?.type !== "pick") return null;
    const pickIndex = currentStep.side === "blue" ? bluePicks.length : redPicks.length;
    const player = currentStep.side === "blue" ? bluePlayers[pickIndex] : redPlayers[pickIndex];
    const effectiveRole = ROLE_ORDER[pickIndex];
    if (!player || !effectiveRole) return null;
    return { playerId: player.id, effectiveRole };
  }, [bluePicks.length, bluePlayers, currentStep?.side, currentStep?.type, redPicks.length, redPlayers]);

  const currentPickEvaluationContextKey = currentPickEvaluationContext
    ? `${stepIndex}:${currentStep?.side}:${currentPickEvaluationContext.playerId}:${currentPickEvaluationContext.effectiveRole}`
    : "";

  useEffect(() => {
    if (!currentPickEvaluationContext) return;
    const requests = champions
      .filter((champion) => !usedChampionIds.has(champion.id))
      .map((champion) => ({ ...currentPickEvaluationContext, championId: champion.id }));
    const missing = requests.filter((request) => !draftEvaluationCacheRef.current.has(draftEvaluationKey(request)));
    if (missing.length === 0) return;

    const requestId = draftEvaluationRequestIdRef.current + 1;
    draftEvaluationRequestIdRef.current = requestId;
    draftEvaluationRequestKeyRef.current = currentPickEvaluationContextKey;
    let active = true;
    invoke<DraftPickEvaluation[]>("evaluate_draft_picks", { picks: missing })
      .then((evaluations) => {
        if (!active || !isCurrentDraftEvaluationRequest(
          requestId,
          draftEvaluationRequestIdRef.current,
          currentPickEvaluationContextKey,
          draftEvaluationRequestKeyRef.current,
        )) return;
        evaluations.forEach((evaluation, index) => {
          const request = missing[index];
          if (request) draftEvaluationCacheRef.current.set(draftEvaluationKey(request), evaluation);
        });
        setDraftEvaluationVersion((version) => version + 1);
      })
      .catch(() => {
        // A failed recommendation must never prevent a user pick or trap an AI turn.
        if (active && isCurrentDraftEvaluationRequest(
          requestId,
          draftEvaluationRequestIdRef.current,
          currentPickEvaluationContextKey,
          draftEvaluationRequestKeyRef.current,
        )) {
          missing.forEach((request) => {
            draftEvaluationCacheRef.current.set(draftEvaluationKey(request), {
              champion_id: request.championId,
              meta_power: 0,
              mastery: 0,
              skill_fit: 0,
              execution_risk: 0,
              total: 0,
              engine_modifier: 0,
            });
          });
          setDraftEvaluationVersion((version) => version + 1);
        }
      });
    return () => { active = false; };
  }, [champions, currentPickEvaluationContext, currentPickEvaluationContextKey, usedChampionIds]);

  const pendingEvaluation = pendingChampionId && currentPickEvaluationContext
    ? draftEvaluationCacheRef.current.get(draftEvaluationKey({
      ...currentPickEvaluationContext,
      championId: pendingChampionId,
    })) ?? null
    : null;

  const buildOrderedPicks = (
    side: Side,
    selections: DraftSelection[],
    manualOrder: number[] | null,
  ): Array<DraftPick | null> => {
    const naturalOrder = selections.map((_, idx) => idx);
    const order = !finished
      ? naturalOrder
      : manualOrder && manualOrder.length === selections.length
        ? manualOrder
        : buildBestRoleOrder(side, selections);

    return ROLE_ORDER.map((role, roleIdx) => {
      const selectionIndex = order[roleIdx];
      if (selectionIndex === undefined) return null;
      const selection = selections[selectionIndex];
      if (!selection) return null;
      return { role, championId: selection.championId };
    });
  };

  const blueBanDisplay: Array<string | null> = [...blueBans, null, null, null, null].slice(0, 5);
  const redBanDisplay: Array<string | null> = [...redBans, null, null, null, null].slice(0, 5);

  const homeTeamSeed = useMemo(
    () => TEAM_SEEDS.find((team) => normalizeKey(team.name) === normalizeKey(snapshot.home_team.name)),
    [snapshot.home_team.name],
  );
  const awayTeamSeed = useMemo(
    () => TEAM_SEEDS.find((team) => normalizeKey(team.name) === normalizeKey(snapshot.away_team.name)),
    [snapshot.away_team.name],
  );
  const rivalTeamSeed = controlledSide === "blue" ? awayTeamSeed : homeTeamSeed;

  const championLookupByNormalizedName = useMemo(() => {
    const map = new Map<string, ChampionData>();
    champions.forEach((champion) => {
      map.set(normalizeKey(champion.name), champion);
      map.set(normalizeKey(champion.id), champion);
    });
    return map;
  }, [champions]);

  const playerMasteryMap = useMemo(() => {
    const map = new Map<string, Map<string, number>>();
    PLAYER_SEEDS.forEach((player) => {
      const key = `${player.teamId}:${normalizeKey(player.ign)}`;
      const championMap = new Map<string, number>();
      player.champions.forEach((entry) => {
        const championName = String(entry[0] ?? "");
        const mastery = Number(entry[1] ?? 0);
        if (championName.length === 0) return;
        championMap.set(normalizeKey(championName), mastery);
      });
      map.set(key, championMap);
    });
    return map;
  }, []);

  const runtimeMasteryByPlayerId = useMemo(() => {
    const map = new Map<string, Map<string, number>>();
    (gameState?.champion_masteries ?? []).forEach((entry) => {
      let championMap = map.get(entry.player_id);
      if (!championMap) {
        championMap = new Map<string, number>();
        map.set(entry.player_id, championMap);
      }
      championMap.set(normalizeKey(entry.champion_id), Number(entry.mastery ?? 25));
    });
    return map;
  }, [gameState?.champion_masteries]);

  const runtimeMetaScoreByChampion = useMemo(() => {
    const map = new Map<string, number>();
    (gameState?.champion_patch?.hidden_meta ?? []).forEach((entry) => {
      map.set(normalizeKey(entry.champion_id), tierToMetaScore(String(entry.tier ?? "B")));
    });
    return map;
  }, [gameState?.champion_patch?.hidden_meta]);

  const scrimReportsByTeamId = useMemo(() => {
    const map = new Map<string, ScrimReportData[]>();
    (gameState?.teams ?? []).forEach((team) => {
      map.set(team.id, team.scrim_reports ?? []);
    });
    return map;
  }, [gameState?.teams]);

  const discoveredMetaChampionIds = useMemo(() => {
    const discovered = new Set<string>();
    (gameState?.champion_patch?.discovered_champion_ids ?? []).forEach((championId) => {
      discovered.add(normalizeKey(String(championId ?? "")));
    });
    return discovered;
  }, [gameState?.champion_patch?.discovered_champion_ids]);

  const resolvePlayerMastery = (
    side: Side,
    pickIndex: number,
    championId: string,
  ): number => {
    const playerId = side === "blue" ? bluePlayerIds[pickIndex] : redPlayerIds[pickIndex];
    if (playerId) {
      const runtimeMap = runtimeMasteryByPlayerId.get(playerId);
      if (runtimeMap) {
        return runtimeMap.get(normalizeKey(championId)) ?? 25;
      }
    }

    const teamId = side === "blue" ? homeTeamSeed?.id : awayTeamSeed?.id;
    const playerName = side === "blue" ? bluePlayers[pickIndex]?.name : redPlayers[pickIndex]?.name;
    if (!teamId || !playerName) return 25;

    const masteryByChampion = playerMasteryMap.get(`${teamId}:${normalizeKey(playerName)}`);
    if (!masteryByChampion) return 25;
    return masteryByChampion.get(normalizeKey(championId)) ?? 25;
  };

  const resolveTeamChampionMastery = (side: Side, championId: string): number => {
    const teamId = side === "blue" ? homeTeamSeed?.id : awayTeamSeed?.id;
    const teamPlayers = side === "blue" ? bluePlayers : redPlayers;
    if (!teamId || teamPlayers.length === 0) return 25;

    let best = 25;
    teamPlayers.forEach((player) => {
      const masteryByChampion = playerMasteryMap.get(`${teamId}:${normalizeKey(player.name)}`);
      const value = masteryByChampion?.get(normalizeKey(championId)) ?? 25;
      if (value > best) best = value;
    });
    return best;
  };

  const blueOrderedPicks = buildOrderedPicks("blue", bluePicks, finished ? blueRoleOrder : null);
  const redOrderedPicks = buildOrderedPicks("red", redPicks, finished ? redRoleOrder : null);

  const metaScoreForChampion = (champion: ChampionData): number => {
    const runtime = runtimeMetaScoreByChampion.get(normalizeKey(champion.id));
    if (typeof runtime === "number") return runtime;
    const direct = META_CHAMPION_SCORES[normalizeKey(champion.id)] ?? META_CHAMPION_SCORES[normalizeKey(champion.name)];
    return direct ?? 0;
  };

  const enemySideFor = (side: Side): Side => (side === "blue" ? "red" : "blue");

  const knownRivalChampionIds = useMemo(() => {
    const rivalSide: Side = controlledSide === "blue" ? "red" : "blue";
    const rivalPlayers = rivalSide === "blue" ? bluePlayers : redPlayers;
    const rivalTeamSeedId = rivalSide === "blue" ? homeTeamSeed?.id : awayTeamSeed?.id;

    const known = new Set<string>();
    rivalPlayers.forEach((player) => {
      const masteryMap = rivalTeamSeedId
        ? playerMasteryMap.get(`${rivalTeamSeedId}:${normalizeKey(player.name)}`)
        : undefined;
      if (!masteryMap || masteryMap.size === 0) return;

      let bestChampionKey: string | null = null;
      let bestMastery = Number.NEGATIVE_INFINITY;
      masteryMap.forEach((mastery, championKey) => {
        if (mastery > bestMastery) {
          bestMastery = mastery;
          bestChampionKey = championKey;
        }
      });

      if (!bestChampionKey) return;
      const champion = championLookupByNormalizedName.get(bestChampionKey);
      if (champion) {
        known.add(champion.id);
      }
    });

    return known;
  }, [
    awayTeamSeed?.id,
    bluePlayers,
    championLookupByNormalizedName,
    controlledSide,
    homeTeamSeed?.id,
    playerMasteryMap,
    redPlayers,
  ]);

  const visibleChampions = useMemo(() => {
    const filtered = champions.filter((champion) => {
      if (roleFilter !== "ALL" && !champion.roleHints.includes(roleFilter)) return false;
      if (searchTerm.trim().length > 0) {
        const query = searchTerm.trim().toLowerCase();
        if (!champion.name.toLowerCase().includes(query)) return false;
      }

      if (metaTierFilter !== "ALL") {
        const tier = knownMetaTierForChampion(
          champion,
          runtimeMetaScoreByChampion,
          discoveredMetaChampionIds,
        );
        if (tier !== metaTierFilter) return false;
      }


      return true;
    });

    if (sortMode === "meta") {
      return filtered.sort((left, right) => {
        const leftKnown =
          knownMetaTierForChampion(left, runtimeMetaScoreByChampion, discoveredMetaChampionIds) !== "?";
        const rightKnown =
          knownMetaTierForChampion(right, runtimeMetaScoreByChampion, discoveredMetaChampionIds) !== "?";
        if (leftKnown !== rightKnown) return leftKnown ? -1 : 1;

        const metaDelta = metaScoreForChampion(right) - metaScoreForChampion(left);
        if (metaDelta !== 0) return metaDelta;
        return left.name.localeCompare(right.name);
      });
    }

    if (sortMode === "mastery") {
      return filtered.sort((left, right) => {
        const masteryDelta = displayMasteryForChampion(right) - displayMasteryForChampion(left);
        if (masteryDelta !== 0) return masteryDelta;
        return left.name.localeCompare(right.name);
      });
    }

    return filtered.sort((left, right) => left.name.localeCompare(right.name));
  }, [
    bluePicks.length,
    champions,
    controlledSide,
    currentStep,
    knownRivalChampionIds,
    metaTierFilter,
    redPicks.length,
    roleFilter,
    sortMode,
    metaScoreForChampion,
    discoveredMetaChampionIds,
    runtimeMetaScoreByChampion,
    displayMasteryForChampion,
    resolvePlayerMastery,
    searchTerm,
  ]);

  const getAvailableChampions = (): ChampionData[] =>
    champions.filter((champion) => !usedChampionIds.has(champion.id));

  const selectAiChampionForCurrentStep = (): ChampionData | null => {
    if (!currentStep) return null;

    const available = getAvailableChampions();
    if (available.length === 0) return null;

    if (currentStep.type === "pick") {
      const aiSide = currentStep.side;
      const ownPicks = aiSide === "blue" ? bluePicks : redPicks;
      const enemyPicks = aiSide === "blue" ? redPicks : bluePicks;
      const coveredRoles = assignedRolesForSelections(ownPicks);
      const missingRoles = ROLE_ORDER.filter((role) => !coveredRoles.has(role));

      const candidates = available.filter((champion) =>
        canAssignUniqueRoles([...ownPicks, { championId: champion.id }]),
      );
      const roleConstrainedCandidates =
        missingRoles.length > 0
          ? candidates.filter((champion) => champion.roleHints.some((role) => missingRoles.includes(role)))
          : [];
      const scoringPool = roleConstrainedCandidates.length > 0 ? roleConstrainedCandidates : candidates;

      if (scoringPool.length === 0) return available[0] ?? null;

      if (!currentPickEvaluationContext) return null;

      let bestChampion: ChampionData | null = null;
      let bestScore = Number.NEGATIVE_INFINITY;

      scoringPool.forEach((champion) => {
        const evaluation = draftEvaluationCacheRef.current.get(draftEvaluationKey({
          ...currentPickEvaluationContext,
          championId: champion.id,
        }));
        if (!evaluation) return;
        const roleNeedBonus =
          missingRoles.length > 0 && champion.roleHints.some((role) => missingRoles.includes(role)) ? 12 : 0;
        let counter = 0;

        enemyPicks.forEach((enemyPick) => {
          counter += counterValue(champion.id, enemyPick.championId) * AI_WEIGHTS.pick.counterAdvantageWeight;
          counter -= counterValue(enemyPick.championId, champion.id) * AI_WEIGHTS.pick.counterRiskWeight;
        });

        const score = evaluation.total + counter + roleNeedBonus;
        if (score > bestScore) {
          bestScore = score;
          bestChampion = champion;
        }
      });

      return bestChampion;
    }

    const targetSide = enemySideFor(currentStep.side);
    const targetPicks = targetSide === "blue" ? bluePicks : redPicks;
    const enemyCoveredRoles = assignedRolesForSelections(targetPicks);
    const ranked = rankBanCandidates({
      available: available.map((champion) => ({
        championId: champion.id,
        roleHints: champion.roleHints,
      })),
      enemyCoveredRoles,
      resolveEnemyMastery: (championId) => resolveTeamChampionMastery(targetSide, championId),
      resolveMetaScore: (championId) => {
        const champion = championById.get(championId);
        return champion ? metaScoreForChampion(champion) : 0;
      },
      resolveScoringContext: (candidate) => {
        return {
          roleAlreadyCovered: candidate.roleHints.length > 0
            && candidate.roleHints.every((role) => enemyCoveredRoles.has(role as Role)),
          enemyJungleLocked: enemyCoveredRoles.has("JUNGLE"),
          isFlexThreat: candidate.roleHints.length >= 2,
          draftHashSeed: `${stepIndex}:${targetSide}:${candidate.championId}`,
        };
      },
    });
    const bestBanId = ranked[0]?.championId;
    return bestBanId ? championById.get(bestBanId) ?? null : null;
  };

  const selectTimeoutChampionForUserTurn = (): ChampionData | null => {
    if (!currentStep || currentStep.side !== controlledSide) return null;

    const available = getAvailableChampions();
    if (available.length === 0) return null;

    if (pendingChampionId) {
      const marked = championById.get(pendingChampionId);
      if (marked && !usedChampionIds.has(marked.id)) return marked;
    }

    if (currentStep.type === "ban") {
      const randomIndex = Math.floor(Math.random() * available.length);
      return available[randomIndex] ?? null;
    }

    const ownPicks = controlledSide === "blue" ? bluePicks : redPicks;
    const coveredRoles = assignedRolesForSelections(ownPicks);
    const missingRoles = ROLE_ORDER.filter((role) => !coveredRoles.has(role));
    const roleCandidates = available.filter((champion) =>
      missingRoles.some((role) => champion.roleHints.includes(role)),
    );

    const feasibleCandidates = available.filter((champion) =>
      canAssignUniqueRoles([...ownPicks, { championId: champion.id }]),
    );
    const feasibleIds = new Set(feasibleCandidates.map((champion) => champion.id));
    const prioritizedRoleCandidates = roleCandidates.filter((champion) => feasibleIds.has(champion.id));
    const pool = prioritizedRoleCandidates.length > 0
      ? prioritizedRoleCandidates
      : feasibleCandidates.length > 0
        ? feasibleCandidates
        : available;
    const randomIndex = Math.floor(Math.random() * pool.length);
    return pool[randomIndex] ?? null;
  };

  const handleSkipDraftDebug = (): void => {
    if (finished || loading || champions.length === 0) return;

    let nextBlueBans = [...blueBans];
    let nextRedBans = [...redBans];
    let nextBluePicks = [...bluePicks];
    let nextRedPicks = [...redPicks];
    const nextHistory = [...draftHistory];

    const pickForStep = (step: DraftAction): ChampionData | null => {
      const used = new Set<string>(lockedChampionIds);
      nextBlueBans.forEach((id) => used.add(id));
      nextRedBans.forEach((id) => used.add(id));
      nextBluePicks.forEach((pick) => used.add(pick.championId));
      nextRedPicks.forEach((pick) => used.add(pick.championId));

      const available = champions.filter((champion) => !used.has(champion.id));
      if (available.length === 0) return null;

      if (step.type === "pick") {
        const aiSide = step.side;
        const ownPicks = aiSide === "blue" ? nextBluePicks : nextRedPicks;
        const enemyPicks = aiSide === "blue" ? nextRedPicks : nextBluePicks;
        const coveredRoles = assignedRolesForSelections(ownPicks);
        const missingRoles = ROLE_ORDER.filter((role) => !coveredRoles.has(role));

        const candidates = available.filter((champion) =>
          canAssignUniqueRoles([...ownPicks, { championId: champion.id }]),
        );
        const roleConstrainedCandidates =
          missingRoles.length > 0
            ? candidates.filter((champion) => champion.roleHints.some((role) => missingRoles.includes(role)))
            : [];
        const scoringPool = roleConstrainedCandidates.length > 0 ? roleConstrainedCandidates : candidates;
        if (scoringPool.length === 0) return available[0] ?? null;

        let bestChampion: ChampionData | null = null;
        let bestScore = Number.NEGATIVE_INFINITY;

        scoringPool.forEach((champion) => {
          const mastery = resolveTeamChampionMastery(aiSide, champion.id);
          const meta = metaScoreForChampion(champion);
          const roleNeedBonus =
            missingRoles.length > 0 && champion.roleHints.some((role) => missingRoles.includes(role)) ? 12 : 0;
          let counter = 0;

          enemyPicks.forEach((enemyPick) => {
            counter += counterValue(champion.id, enemyPick.championId) * AI_WEIGHTS.pick.counterAdvantageWeight;
            counter -= counterValue(enemyPick.championId, champion.id) * AI_WEIGHTS.pick.counterRiskWeight;
          });

          const score =
            mastery * AI_WEIGHTS.pick.masteryWeight +
            meta * AI_WEIGHTS.pick.metaWeight +
            counter +
            roleNeedBonus;

          if (score > bestScore) {
            bestScore = score;
            bestChampion = champion;
          }
        });

        return bestChampion;
      }

      const targetSide = enemySideFor(step.side);
      const targetPicks = targetSide === "blue" ? nextBluePicks : nextRedPicks;
      const enemyCoveredRoles = assignedRolesForSelections(targetPicks);
      const ranked = rankBanCandidates({
        available: available.map((champion) => ({
          championId: champion.id,
          roleHints: champion.roleHints,
        })),
        enemyCoveredRoles,
        resolveEnemyMastery: (championId) => resolveTeamChampionMastery(targetSide, championId),
        resolveMetaScore: (championId) => {
          const champion = championById.get(championId);
          return champion ? metaScoreForChampion(champion) : 0;
        },
        resolveScoringContext: (candidate) => {
          return {
            roleAlreadyCovered: candidate.roleHints.length > 0
              && candidate.roleHints.every((role) => enemyCoveredRoles.has(role as Role)),
            enemyJungleLocked: enemyCoveredRoles.has("JUNGLE"),
            isFlexThreat: candidate.roleHints.length >= 2,
            draftHashSeed: `${stepIndex}:${targetSide}:${candidate.championId}:debug`,
          };
        },
      });
      const bestBanId = ranked[0]?.championId;
      return bestBanId ? championById.get(bestBanId) ?? null : null;
    };

    let processedSteps = 0;
    for (let i = stepIndex; i < DRAFT_SEQUENCE.length; i += 1) {
      const step = DRAFT_SEQUENCE[i];
      const champion = pickForStep(step);
      if (!champion) break;

      if (step.type === "ban") {
        if (step.side === "blue") nextBlueBans = [...nextBlueBans, champion.id];
        else nextRedBans = [...nextRedBans, champion.id];
      } else if (step.side === "blue") {
        nextBluePicks = [...nextBluePicks, { championId: champion.id }];
      } else {
        nextRedPicks = [...nextRedPicks, { championId: champion.id }];
      }

      nextHistory.push(champion.id);
      processedSteps += 1;
    }

    if (processedSteps === 0) return;

    setBlueBans(nextBlueBans);
    setRedBans(nextRedBans);
    setBluePicks(nextBluePicks);
    setRedPicks(nextRedPicks);
    setDraftHistory(nextHistory);
    setBlueRoleOrder(null);
    setRedRoleOrder(null);
    setPendingChampionId(null);
    setStepIndex(Math.min(DRAFT_SEQUENCE.length, stepIndex + processedSteps));
  };

  const currentStepKey = currentStep
    ? `${stepIndex}-${currentStep.type}-${currentStep.side}-${currentStep.label}`
    : "finished";

  useEffect(() => {
    if (!currentStep || finished) return;

    const duration = AI_TIMING.userTurnMs;

    setTurnDurationMs(duration);
    setTurnStartedAt(Date.now());
    setTurnRemainingMs(duration);
    autoResolvedStepKeyRef.current = null;
    setPendingChampionId(null);
    setSwapSource(null);
  }, [controlledSide, currentStepKey, currentStep, finished]);

  useEffect(() => {
    if (!currentStep || finished) return;

    const interval = setInterval(() => {
      const elapsed = Date.now() - turnStartedAt;
      const remaining = Math.max(0, turnDurationMs - elapsed);
      setTurnRemainingMs(remaining);
    }, 100);

    return () => clearInterval(interval);
  }, [currentStep, finished, turnDurationMs, turnStartedAt]);

  useEffect(() => {
    if (!currentStep || finished || loading || champions.length === 0) return;
    if (!allAi && currentStep.side === controlledSide) return;

    const minMs = Math.min(AI_TIMING.aiMinMs, AI_TIMING.aiMaxMs);
    const maxMs = Math.max(AI_TIMING.aiMinMs, AI_TIMING.aiMaxMs);
    const aiDelay = minMs + Math.floor(Math.random() * Math.max(1, maxMs - minMs + 1));

    const timer = setTimeout(() => {
      if (autoResolvedStepKeyRef.current === currentStepKey) return;
      const autoChampion = selectAiChampionForCurrentStep();
      if (autoChampion) {
        autoResolvedStepKeyRef.current = currentStepKey;
        handleSelectChampion(autoChampion, "ai");
      }
    }, aiDelay);

    return () => clearTimeout(timer);
  }, [
    allAi,
    champions,
    controlledSide,
    currentStep,
    currentStepKey,
    currentPickEvaluationContext,
    draftEvaluationVersion,
    finished,
    loading,
    usedChampionIds,
  ]);

  useEffect(() => {
    if (!currentStep || finished || loading || champions.length === 0) return;
    if (turnRemainingMs > 0) return;
    if (autoResolvedStepKeyRef.current === currentStepKey) return;

    const autoChampion = !allAi && currentStep.side === controlledSide
      ? selectTimeoutChampionForUserTurn()
      : selectAiChampionForCurrentStep();
    if (autoChampion) {
      autoResolvedStepKeyRef.current = currentStepKey;
      handleSelectChampion(autoChampion, !allAi && currentStep.side === controlledSide ? "user" : "ai");
    }
  }, [
    allAi,
    bluePicks,
    championById,
    champions,
    controlledSide,
    currentStep,
    currentStepKey,
    draftEvaluationVersion,
    finished,
    loading,
    pendingChampionId,
    redPicks,
    turnRemainingMs,
    usedChampionIds,
  ]);

  const scoreDraft = (side: Side): DraftScoreBreakdown => {
    const ownPicks = side === "blue" ? bluePicks : redPicks;
    const enemyPicks = side === "blue" ? redPicks : bluePicks;
    const teamId = side === "blue" ? snapshot.home_team.id : snapshot.away_team.id;
    const opponentTeamId = side === "blue" ? snapshot.away_team.id : snapshot.home_team.id;
    const playerIds = side === "blue" ? bluePlayerIds : redPlayerIds;
    const staffEffects = getLolStaffEffectsForTeam(gameState, teamId);

    let mastery = 0;
    let synergy = 0;
    let counter = 0;
    let comfort = 0;
    let preparation = 0;

    ownPicks.forEach((pick, idx) => {
      const champMastery = resolvePlayerMastery(side, idx, pick.championId);
      if (champMastery >= 75) mastery += 3;
      else if (champMastery >= 50) mastery += 2;
      else if (champMastery >= 25) mastery += 1;

      enemyPicks.forEach((enemyPick) => {
        counter +=
          counterValue(pick.championId, enemyPick.championId) *
          AI_WEIGHTS.score.counterAdvantageWeight;
        counter -=
          counterValue(enemyPick.championId, pick.championId) *
          AI_WEIGHTS.score.counterRiskWeight;
      });
    });

    if (ownPicks.length > 0) {
      preparation = Math.round(Math.max(-1, Math.min(3, (staffEffects.tactics - 1) * 4 + (staffEffects.analysis - 1) * 3)));
    }

    const scrimSignal = calculateScrimDraftSignal(
      scrimReportsByTeamId.get(teamId) ?? [],
      teamId,
      opponentTeamId,
      ownPicks.map((pick, index) => ({ championId: pick.championId, playerId: playerIds[index] ?? null })),
    );
    comfort += scrimSignal.comfort;
    preparation += scrimSignal.preparation;
    synergy += scrimSignal.synergy;

    return {
      mastery,
      synergy,
      counter,
      comfort,
      preparation,
      total: mastery + synergy + counter + comfort + preparation,
    };
  };

  const blueScore = useMemo(() => scoreDraft("blue"), [
    bluePicks,
    redPicks,
    bluePlayerIds,
    snapshot.home_team.id,
    snapshot.home_team.draft_strategy,
    snapshot.away_team.id,
    gameState?.staff,
    scrimReportsByTeamId,
  ]);
  const redScore = useMemo(() => scoreDraft("red"), [
    bluePicks,
    redPicks,
    redPlayerIds,
    snapshot.away_team.id,
    snapshot.away_team.draft_strategy,
    snapshot.home_team.id,
    gameState?.staff,
    scrimReportsByTeamId,
  ]);

  const controlledScrimSignal = useMemo(() => {
    const side = controlledSide;
    const teamId = side === "blue" ? snapshot.home_team.id : snapshot.away_team.id;
    const opponentTeamId = side === "blue" ? snapshot.away_team.id : snapshot.home_team.id;
    const picks = side === "blue" ? bluePicks : redPicks;
    const playerIds = side === "blue" ? bluePlayerIds : redPlayerIds;
    return calculateScrimDraftSignal(
      scrimReportsByTeamId.get(teamId) ?? [],
      teamId,
      opponentTeamId,
      picks.map((pick, index) => ({ championId: pick.championId, playerId: playerIds[index] ?? null })),
    );
  }, [
    bluePicks,
    bluePlayerIds,
    controlledSide,
    redPicks,
    redPlayerIds,
    scrimReportsByTeamId,
    snapshot.away_team.id,
    snapshot.home_team.id,
  ]);

  useEffect(() => {
    if (!finished) return;
    if (!blueRoleOrder && bluePicks.length > 0) {
      setBlueRoleOrder(buildBestRoleOrder("blue", bluePicks));
    }
    if (!redRoleOrder && redPicks.length > 0) {
      setRedRoleOrder(buildBestRoleOrder("red", redPicks));
    }
  }, [bluePicks, blueRoleOrder, finished, redPicks, redRoleOrder]);

  useEffect(() => {
    if (!finished) {
      finalRoleReassignFxPlayedRef.current = false;
      setShowFinalRoleReassignFx(false);
      return;
    }

    if (finalRoleReassignFxPlayedRef.current) return;

    const blueReady = bluePicks.length === 0 || !!blueRoleOrder;
    const redReady = redPicks.length === 0 || !!redRoleOrder;
    if (!blueReady || !redReady) return;

    finalRoleReassignFxPlayedRef.current = true;
    setShowFinalRoleReassignFx(true);
    const timeoutId = setTimeout(() => setShowFinalRoleReassignFx(false), 800);
    return () => clearTimeout(timeoutId);
  }, [bluePicks.length, blueRoleOrder, finished, redPicks.length, redRoleOrder]);

  const armSwapFromRole = (side: Side, roleIdx: number): void => {
    if (!finished) return;
    setSwapSource((prev) => {
      if (prev && prev.side === side && prev.index === roleIdx) return null;
      return { side, index: roleIdx };
    });
  };

  const completeSwapOnRole = (side: Side, targetRoleIdx: number): void => {
    if (!swapSource || swapSource.side !== side) return;
    const roleOrder = side === "blue" ? blueRoleOrder : redRoleOrder;
    if (!roleOrder) {
      setSwapSource(null);
      return;
    }

    if (swapSource.index === targetRoleIdx) {
      setSwapSource(null);
      return;
    }

    const next = [...roleOrder];
    [next[swapSource.index], next[targetRoleIdx]] = [next[targetRoleIdx], next[swapSource.index]];
    if (side === "blue") setBlueRoleOrder(next);
    else setRedRoleOrder(next);
    setSwapSource(null);
  };

  const controlledScore = controlledSide === "blue" ? blueScore : redScore;
  const rivalScore = controlledSide === "blue" ? redScore : blueScore;

  const rivalMasteryDisplay = useMemo(() => {
    if (!rivalTeamSeed) return [];

    const rivalSide: Side = controlledSide === "blue" ? "red" : "blue";
    const rivalRosterPlayers = rivalSide === "blue" ? bluePlayers : redPlayers;
    const rivalRosterPlayerIds = rivalSide === "blue" ? bluePlayerIds : redPlayerIds;
    const userTeamId = controlledSide === "blue" ? snapshot.home_team.id : snapshot.away_team.id;
    const staffEffects = getLolStaffEffectsForTeam(gameState, userTeamId);
    const staffRevealBudget = calculateStaffRevealBudget(staffEffects.metaDiscovery);

    const scoutedPlayerKeys = new Set<string>();
    (gameState?.messages ?? []).forEach((message) => {
      const report = message.context?.scout_report;
      if (!report) return;
      const reportPlayerId = report.player_id;
      const matchedGamePlayer = gameState?.players.find((player) => player.id === reportPlayerId);
      if (matchedGamePlayer) {
        scoutedPlayerKeys.add(normalizeKey(matchedGamePlayer.match_name));
      }
      if (report.player_name) {
        scoutedPlayerKeys.add(normalizeKey(report.player_name));
      }
    });

    const rivalSeedPlayers = PLAYER_SEEDS.filter((player) => player.teamId === rivalTeamSeed.id);
    const byIgn = new Map<string, PlayerSeed>();
    rivalSeedPlayers.forEach((player) => {
      byIgn.set(normalizeKey(player.ign), player);
    });

    const matchedPlayers: Array<{ seed: PlayerSeed; playerId: string | null }> = [];
    const matchedKeys = new Set<string>();
    rivalRosterPlayers.forEach((player, index) => {
      const key = normalizeKey(player.name);
      const match = byIgn.get(key);
      if (match && !matchedKeys.has(key)) {
        matchedPlayers.push({ seed: match, playerId: rivalRosterPlayerIds[index] ?? null });
        matchedKeys.add(key);
      }
    });

    if (matchedPlayers.length < 5) {
      rivalSeedPlayers
        .filter((player) => !matchedKeys.has(normalizeKey(player.ign)))
        .sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0))
        .slice(0, 5 - matchedPlayers.length)
        .forEach((player) => matchedPlayers.push({ seed: player, playerId: null }));
    }

    if (matchedPlayers.length === 0) return [];

    const selectedChampionIds = new Set<string>();
    const result: RivalMasteryDisplayEntry[] = [];
    const staffCandidates: RivalMasteryDisplayEntry[] = [];

    const addKnownChampion = (entry: RivalMasteryDisplayEntry): boolean => {
      if (selectedChampionIds.has(entry.champion.id)) return false;
      selectedChampionIds.add(entry.champion.id);
      result.push(entry);
      return true;
    };

    matchedPlayers.forEach(({ seed, playerId }) => {
      const isScouted = Boolean(playerId && scoutedPlayerKeys.has(normalizeKey(seed.ign))) || scoutedPlayerKeys.has(normalizeKey(seed.ign));
      const allKnownOptions: RivalMasteryOption[] = (seed.champions ?? [])
        .map((entry) => {
          const championName = String(entry[0] ?? "");
          const mastery = Number(entry[1] ?? 0);
          if (!championName) return null;
          const champion = championLookupByNormalizedName.get(normalizeKey(championName));
          if (!champion) return null;
          return {
            champion,
            mastery,
            playerName: String(seed.ign ?? "").trim() || "N/A",
            playerRole: mapSeedRoleToDraftRole(String(seed.role ?? "")),
          };
        })
        .filter(
          (
            item,
          ): item is {
            champion: ChampionData;
            mastery: number;
            playerName: string;
            playerRole: Role | null;
          } => item !== null,
        )
        .sort((a, b) => b.mastery - a.mastery);

      const playerKnowledge = selectRivalMasteryKnowledgeForPlayer(
        allKnownOptions,
        usedChampionIds,
        selectedChampionIds,
        isScouted,
      );

      playerKnowledge.knownEntries.forEach(addKnownChampion);
      playerKnowledge.staffCandidates.forEach((candidate) => {
        staffCandidates.push(candidate);
      });
    });

    selectStaffRevealEntries(staffCandidates, staffRevealBudget, usedChampionIds)
      .forEach(addKnownChampion);

    return result;
  }, [
    bluePlayerIds,
    bluePlayers,
    championLookupByNormalizedName,
    controlledSide,
    gameState,
    redPlayers,
    redPlayerIds,
    rivalTeamSeed,
    snapshot.away_team.id,
    snapshot.home_team.id,
    usedChampionIds,
  ]);

  const selectedChampionCounterIntel = useMemo(() => {
    if (!pendingChampionId || !currentStep || currentStep.type !== "pick" || currentStep.side !== controlledSide) {
      return null;
    }

    const selectedChampion = championById.get(pendingChampionId);
    if (!selectedChampion) return null;

    const revealBudget = calculateCounterRevealBudget(userStaffEffects.metaDiscovery);
    const consulted = consultedCounterChampionIds.has(selectedChampion.id);
    const enemyPicks = controlledSide === "blue" ? redPicks : bluePicks;
    const enemyPickedIds = new Set(enemyPicks.map((pick) => pick.championId));

    const availableForEnemy = champions.filter(
      (champion) => !usedChampionIds.has(champion.id) || enemyPickedIds.has(champion.id),
    );

    const favorableAll = availableForEnemy
      .map((enemyChampion) => ({
        champion: enemyChampion,
        value: counterValue(selectedChampion.id, enemyChampion.id),
        isPicked: enemyPickedIds.has(enemyChampion.id),
      }))
      .filter((row) => row.value >= 1)
      .sort((a, b) => b.value - a.value);

    const riskyAll = availableForEnemy
      .map((enemyChampion) => ({
        champion: enemyChampion,
        value: counterValue(enemyChampion.id, selectedChampion.id),
        isPicked: enemyPickedIds.has(enemyChampion.id),
      }))
      .filter((row) => row.value >= 1)
      .sort((a, b) => b.value - a.value);

    const withPickedPriority = (rows: Array<{ champion: ChampionData; value: number; isPicked: boolean }>) => {
      if (consulted) return rows;
      const top = rows.slice(0, revealBudget);
      const selected = new Set(top.map((row) => row.champion.id));
      const pickedMissing = rows
        .filter((row) => row.isPicked && !selected.has(row.champion.id))
        .sort((a, b) => b.value - a.value);
      if (pickedMissing.length === 0) return top;
      const merged = [...top];
      pickedMissing.forEach((row) => {
        if (merged.length >= revealBudget) merged.pop();
        merged.push(row);
      });
      return merged.sort((a, b) => {
        if (a.isPicked !== b.isPicked) return a.isPicked ? -1 : 1;
        return b.value - a.value;
      });
    };

    const favorable = withPickedPriority(favorableAll);
    const risky = withPickedPriority(riskyAll);

    return {
      selectedChampion,
      revealBudget,
      consulted,
      favorable,
      risky,
    };
  }, [
    bluePicks,
    champions,
    championById,
    controlledSide,
    currentStep,
    pendingChampionId,
    redPicks,
    consultedCounterChampionIds,
    userStaffEffects.metaDiscovery,
    usedChampionIds,
  ]);

  // ---------------------------------------------------------------------------
  // Dynamic Draft Tips - Assistant Coach & Player Suggestions
  // ---------------------------------------------------------------------------
  const assistantCoachTips = useMemo<DraftAdviceTip[]>(() => {
    const tips: DraftAdviceTip[] = [];
    if (!gameState) return tips;

    const userTeamId = controlledSide === "blue" ? snapshot.home_team.id : snapshot.away_team.id;
    const assistantCoach = gameState.staff.find(
      (staff) => staff.team_id === userTeamId && staff.role === "AssistantManager",
    );
    const coachName =
      assistantCoach && (assistantCoach.first_name || assistantCoach.last_name)
        ? `${assistantCoach.first_name ?? ""} ${assistantCoach.last_name ?? ""}`.trim()
        : t("match.draft.assistantCoach");
    const coachImage = assistantCoach?.profile_image_url ?? ASSISTANT_COACH_PLACEHOLDER;

    if (finished) {
      return [
        {
          sourceType: "coach",
          sourceName: coachName,
          sourceRole: t("match.draft.assistantCoach"),
          sourceImage: coachImage,
          type: "warn",
          text: t("match.draft.completed", { defaultValue: "Draft completed." }),
        },
      ];
    }

    const draftAdviceStage: "ban" | "pick" | "post" = finished
      ? "post"
      : currentStep?.type === "ban"
        ? "ban"
        : "pick";

    const uniquePhrase = (
      keyPrefix: string,
      pool: string[],
      seed: string,
      vars: Record<string, string | number>,
    ): string => {
      const { index, template } = pickPhrase(pool, seed);
      return t(`${keyPrefix}.${index}`, {
        ...vars,
        defaultValue: template,
      });
    };

    const ownPicksList = controlledSide === "blue" ? bluePicks : redPicks;
    const enemyPicksList = controlledSide === "blue" ? redPicks : bluePicks;
    const ownCoveredRoles = new Set<Role>();
    ownPicksList.forEach((pick) => {
      const champion = championById.get(pick.championId);
      champion?.roleHints.forEach((role) => ownCoveredRoles.add(role));
    });
    const availableChampions = champions.filter((champion) => !usedChampionIds.has(champion.id));

    const coachSkill = assistantCoach?.attributes?.coaching ?? 60;

    const addCoachTip = (type: "ban" | "pick" | "warn", text: string, champion?: ChampionData) => {
      tips.push({
        sourceType: "coach",
        sourceName: coachName,
        sourceRole: t("match.draft.assistantCoach"),
        sourceImage: coachImage,
        type,
        text,
      champion,
      });
    };

    const enemyPickedRoles = new Set<Role>();
    enemyPicksList.forEach((pick) => {
      const champion = championById.get(pick.championId);
      const primaryRole = champion?.roleHints?.[0];
      if (primaryRole) enemyPickedRoles.add(primaryRole);
    });

    const rivalMasteryCandidates = rivalMasteryDisplay.slice();
    const enemyLockedJungle = enemyPickedRoles.has("JUNGLE");
    const draftHashSeed = `${controlledSide}:${stepIndex}:${blueBans.join("|")}:${redBans.join("|")}:${bluePicks
      .map((pick) => pick.championId)
      .join("|")}:${redPicks.map((pick) => pick.championId).join("|")}`;
    const rivalMasteries = rivalMasteryCandidates
      .map((entry) => {
        const candidateTier = knownMetaTierForChampion(
          entry.champion,
          runtimeMetaScoreByChampion,
          discoveredMetaChampionIds,
        );
        const tier: Exclude<MetaTierFilter, "ALL"> = candidateTier === "?" ? "B" : candidateTier;
        const roleHints = entry.champion.roleHints;
        const isFlexThreat = roleHints.length >= 2;
        const isSpecialThreat = entry.mastery >= 97;
        const roleAlreadyCovered = Boolean(entry.playerRole && enemyPickedRoles.has(entry.playerRole));
        const score = computeBanRecommendationScore({
          enemyMastery: entry.mastery,
          metaScore: metaScoreForChampion(entry.champion),
          tier,
          roleHints,
          roleAlreadyCovered,
          enemyJungleLocked: enemyLockedJungle,
          isFlexThreat,
          isSpecialThreat,
          draftHashSeed: `${draftHashSeed}:${entry.champion.id}`,
        });
        return { ...entry, recommendationScore: score };
      })
      .sort((a, b) => b.recommendationScore - a.recommendationScore);
    if (draftAdviceStage === "ban" && rivalMasteries.length > 0 && coachSkill >= 50) {
      const topRival = rivalMasteries[0];
      if (topRival.mastery >= 75) {
        addCoachTip(
          "ban",
          uniquePhrase(
            "match.draft.phrases.coachBan",
            COACH_BAN_PHRASES,
            `coach-ban-${topRival.champion.id}-${topRival.playerName}-${stepIndex}`,
            {
            champion: topRival.champion.name,
            player: topRival.playerName,
            mastery: topRival.mastery,
            },
          ),
          topRival.champion,
        );
      }
    }

    if (coachSkill >= 70 && ownPicksList.length > 0) {
      const lastOwnPick = ownPicksList[ownPicksList.length - 1]?.championId;
      if (lastOwnPick && enemyPicksList.length > 0) {
        enemyPicksList.forEach((enemyPick) => {
          const counterVal = counterValue(lastOwnPick, enemyPick.championId);
          if (counterVal <= -2) {
            const enemyChamp = championById.get(enemyPick.championId);
            if (enemyChamp) {
              addCoachTip(
                "warn",
                t("match.draft.warnCounterLastPick", {
                  defaultValue: "Careful with {{champion}}: it is countering your last pick.",
                  champion: enemyChamp.name,
                }),
                enemyChamp,
              );
            }
          }
        });
      }
    }

    if (coachSkill >= 55) {
      const hasEarly = ownPicksList.some((pick) => getChampionTiming(pick.championId) === "Early");
      const hasLate = ownPicksList.some((pick) => getChampionTiming(pick.championId) === "Late");

      if (ownPicksList.length >= 3 && !hasEarly && !hasLate) {
        addCoachTip(
          "warn",
          t("match.draft.warnTimingIdentity", {
            defaultValue: "We are missing a clear timing identity. Look for an Early or Late pick to close better.",
          }),
        );
      }

      if (ownPicksList.length >= 4) {
        const rolesCovered = new Set<Role>();
        ownPicksList.forEach((pick) => {
          const champion = championById.get(pick.championId);
          champion?.roleHints.forEach((role) => rolesCovered.add(role));
        });

        if (rolesCovered.size < 4) {
          addCoachTip(
            "warn",
            t("match.draft.warnRoleCoverage", {
              defaultValue: "We still have role gaps. Add a more flexible pick.",
            }),
          );
        }
      }
    }

    const ownPlayers = controlledSide === "blue" ? bluePlayers : redPlayers;
    const ownTeamSeed = controlledSide === "blue" ? homeTeamSeed : awayTeamSeed;
    const ownTeamSeedId = ownTeamSeed?.id ?? null;
    const ownTeamSeedPlayers = ownTeamSeed
      ? PLAYER_SEEDS.filter((seedPlayer) => seedPlayer.teamId === ownTeamSeed.id)
      : [];
    const usedSeedPlayerIgns = new Set<string>();

    ownPlayers.slice(0, 5).forEach((player, index) => {
      const directPlayerKey = ownTeamSeedId ? `${ownTeamSeedId}:${normalizeKey(player.name)}` : "";
      let masteryMap = playerMasteryMap.get(directPlayerKey);
      let matchedSeedPlayer = ownTeamSeedPlayers.find(
        (seedPlayer) => normalizeKey(seedPlayer.ign) === normalizeKey(player.name),
      );
      if (matchedSeedPlayer) {
        usedSeedPlayerIgns.add(normalizeKey(matchedSeedPlayer.ign));
      }

      if (!masteryMap && ownTeamSeedPlayers.length > 0) {
        const role = ROLE_ORDER[index];
        const roleMatch = ownTeamSeedPlayers.find((seedPlayer) => {
          const roleOk = mapSeedRoleToDraftRole(seedPlayer.role) === role;
          const notUsed = !usedSeedPlayerIgns.has(normalizeKey(seedPlayer.ign));
          return roleOk && notUsed;
        });

        const fallback = roleMatch ?? ownTeamSeedPlayers.find(
          (seedPlayer) => !usedSeedPlayerIgns.has(normalizeKey(seedPlayer.ign)),
        );

        if (fallback) {
          usedSeedPlayerIgns.add(normalizeKey(fallback.ign));
          matchedSeedPlayer = fallback;
          masteryMap = ownTeamSeedId
            ? playerMasteryMap.get(`${ownTeamSeedId}:${normalizeKey(fallback.ign)}`)
            : undefined;
        }
      }

      if (!masteryMap) {
        const runtimeMap = runtimeMasteryByPlayerId.get(player.id);
        if (runtimeMap && runtimeMap.size > 0) {
          masteryMap = runtimeMap;
          matchedSeedPlayer = undefined;
        } else {
          const flexibleRole = mapSeedRoleToDraftRole(String(matchedSeedPlayer?.role ?? "")) ?? ROLE_ORDER[index];
          const flexibleImage =
            player.profile_image_url ??
            playerSeedPhotoUrl(matchedSeedPlayer?.photo) ??
            `/player-photos/${player.id}.webp`;
          tips.push({
            sourceType: "player",
            sourceName: player.name,
            sourceRole: flexibleRole,
            sourceImage: flexibleImage,
            type: "pick",
            text: t("match.draft.phrases.playerFlexible", "No tengo prioridades para el pick. Me puedo adaptar al equipo."),
          });
          return;
        }
      }

      const sourceRole = mapSeedRoleToDraftRole(String(matchedSeedPlayer?.role ?? "")) ?? ROLE_ORDER[index];
      const sourceImage =
        player.profile_image_url ??
        playerSeedPhotoUrl(matchedSeedPlayer?.photo) ??
        `/player-photos/${player.id}.webp`;

      const playerState = gameState.players.find((item) => item.id === player.id);
      const gameIq = playerState
        ? Math.round(
          (Number(playerState.attributes.consistency ?? 70) +
            Number(playerState.attributes.macro_play ?? 70) +
            Number(playerState.attributes.consistency ?? 70) +
            Number(playerState.attributes.discipline ?? 70)) /
          4,
        )
        : 70;

      const masteredChampions: Array<{ champion: ChampionData; mastery: number }> = [];
      masteryMap.forEach((mastery, normalizedChampionKey) => {
        const champion = championLookupByNormalizedName.get(normalizedChampionKey);
        if (!champion) return;
        masteredChampions.push({ champion, mastery });
      });

      if (masteredChampions.length === 0) return;
      masteredChampions.sort((a, b) => b.mastery - a.mastery);

      const bestChampionAny = masteredChampions[0];

      let bestMastery = 0;
      let resolvedBestChampion: ChampionData | null = null;

      for (const { champion, mastery } of masteredChampions) {
        if (usedChampionIds.has(champion.id)) continue;
        if (sourceRole && !champion.roleHints.includes(sourceRole)) continue;
        if (mastery > bestMastery) {
          bestMastery = mastery;
          resolvedBestChampion = champion;
        }
      }

      if (!resolvedBestChampion) return;

      const strongCounterTarget = enemyPicksList
        .map((enemyPick) => {
          const enemyChampion = championById.get(enemyPick.championId);
          if (!enemyChampion) return null;
          return {
            enemyChampion,
            value: counterValue(resolvedBestChampion.id, enemyChampion.id),
          };
        })
        .filter((item): item is { enemyChampion: ChampionData; value: number } => item !== null)
        .sort((a, b) => b.value - a.value)[0];

      if (draftAdviceStage !== "ban" && bestMastery >= 50) {
        if (sourceRole && ownCoveredRoles.has(sourceRole)) {
          return;
        }
        const pickPhraseText =
          strongCounterTarget && strongCounterTarget.value >= 2
            ? uniquePhrase(
              gameIq >= 78
                ? "match.draft.phrases.playerSmartCounterPick"
                : "match.draft.phrases.playerCounterPick",
              gameIq >= 78 ? PLAYER_SMART_COUNTER_PICK_PHRASES : PLAYER_COUNTER_PICK_PHRASES,
                 `player-counter-pick-${player.id}-${resolvedBestChampion.id}-${strongCounterTarget.enemyChampion.id}-${stepIndex}`,
               {
                 champion: resolvedBestChampion.name,
                 enemy: strongCounterTarget.enemyChampion.name,
               },
             )
            : uniquePhrase(
              "match.draft.phrases.playerComfortPick",
              PLAYER_COMFORT_PICK_PHRASES,
               `player-comfort-pick-${player.id}-${resolvedBestChampion.id}-${stepIndex}`,
               { champion: resolvedBestChampion.name, mastery: bestMastery },
             );

        tips.push({
          sourceType: "player",
          sourceName: player.name,
          sourceRole,
          sourceImage,
          type: "pick",
          text: pickPhraseText,
          champion: resolvedBestChampion,
        });
      }

      const strongestThreat = availableChampions
        .map((champion) => ({ champion, value: counterValue(champion.id, bestChampionAny.champion.id) }))
        .filter((item) => item.value >= 2)
        .sort((a, b) => b.value - a.value)[0];

      if (draftAdviceStage === "ban" && strongestThreat && bestChampionAny.mastery >= 70) {
        const banTarget =
          availableChampions
            .map((champion) => ({ champion, value: counterValue(champion.id, bestChampionAny.champion.id) }))
            .filter((item) => item.value >= 2)
            .sort((a, b) => b.value - a.value)[0]?.champion ?? strongestThreat.champion;

        tips.push({
          sourceType: "player",
          sourceName: player.name,
          sourceRole,
          sourceImage,
          type: "ban",
          text: uniquePhrase(
            "match.draft.phrases.playerBanRequest",
            PLAYER_BAN_REQUEST_PHRASES,
              `player-ban-${player.id}-${banTarget.id}-${bestChampionAny.champion.id}-${stepIndex}`,
              {
                threat: banTarget.name,
                champion: bestChampionAny.champion.name,
              },
            ),
          champion: banTarget,
        });
      }
    });

    const hasPlayerPickTip = tips.some((tip) => tip.sourceType === "player" && tip.type === "pick");
    if (draftAdviceStage !== "ban" && !hasPlayerPickTip) {
      const fallbackPlayerIndex = ownPlayers.findIndex((_, index) => {
        const fallbackRole =
          mapSeedRoleToDraftRole(String(ownTeamSeedPlayers[index]?.role ?? "")) ?? ROLE_ORDER[index] ?? null;
        return fallbackRole ? !ownCoveredRoles.has(fallbackRole) : true;
      });
      const fallbackPlayer =
        fallbackPlayerIndex >= 0 ? ownPlayers[fallbackPlayerIndex] : ownPlayers[0];
      const fallbackSeedPlayer =
        fallbackPlayerIndex >= 0 ? ownTeamSeedPlayers[fallbackPlayerIndex] : ownTeamSeedPlayers[0];
      const fallbackPlayerRole =
        mapSeedRoleToDraftRole(String(fallbackSeedPlayer?.role ?? "")) ?? ROLE_ORDER[Math.max(0, fallbackPlayerIndex)] ?? null;
      if (fallbackPlayer) {
        tips.push({
          sourceType: "player",
          sourceName: fallbackPlayer.name,
          sourceRole: fallbackPlayerRole ?? ROLE_ORDER[0],
          sourceImage:
            fallbackPlayer.profile_image_url ??
            playerSeedPhotoUrl(fallbackSeedPlayer?.photo) ??
            `/player-photos/${fallbackPlayer.id}.webp`,
          type: "pick",
          text: t("match.draft.phrases.playerFlexible", "Sin información de draft. Sugerencia: podemos priorizar picks seguros o adaptarnos al draft enemigo."),
        });
      }
    }

    const coachTips = tips.filter((tip) => tip.sourceType === "coach");
    const playerTips = tips.filter((tip) => tip.sourceType === "player");

    if (draftAdviceStage === "pick") {
      const playerPickTips = playerTips.filter((tip) => tip.type === "pick").slice(0, 5);
      const coachWarnTips = coachTips.filter((tip) => tip.type !== "pick").slice(0, 1);
      return [...playerPickTips, ...coachWarnTips].slice(0, 5);
    }

    const mixed: DraftAdviceTip[] = [];
    let coachIdx = 0;
    let playerIdx = 0;

    while (mixed.length < 4 && (coachIdx < coachTips.length || playerIdx < playerTips.length)) {
      if (coachIdx < coachTips.length) {
        mixed.push(coachTips[coachIdx]);
        coachIdx += 1;
        if (mixed.length >= 4) break;
      }
      if (playerIdx < playerTips.length) {
        mixed.push(playerTips[playerIdx]);
        playerIdx += 1;
      }
    }

    return mixed.slice(0, 4);
  }, [
    champions,
    gameState,
    currentStep?.type,
    controlledSide,
    finished,
    homeTeamSeed,
    awayTeamSeed,
    snapshot.home_team.id,
    snapshot.away_team.id,
    stepIndex,
    blueBans,
    redBans,
    bluePicks,
    redPicks,
    bluePlayers,
    redPlayers,
    playerMasteryMap,
    championById,
    championLookupByNormalizedName,
    usedChampionIds,
    rivalMasteryDisplay,
    discoveredMetaChampionIds,
    runtimeMetaScoreByChampion,
    metaScoreForChampion,
    t,
  ]);

  const patchLabel =
    gameState?.champion_patch?.current_patch_label ??
    (typeof gameState?.champion_patch?.patch_year === "number" &&
    typeof gameState?.champion_patch?.patch_index_in_year === "number"
      ? `${gameState.champion_patch.patch_year}.${gameState.champion_patch.patch_index_in_year}`
      : typeof gameState?.champion_patch?.current_patch === "number"
        ? String(gameState.champion_patch.current_patch)
        : "--");

  const topBgChampion =
    draftHistory[draftHistory.length - 1] ??
    blueOrderedPicks.find((pick) => pick)?.championId ??
    redOrderedPicks.find((pick) => pick)?.championId ??
    "Sylas";
  const [displayedBgChampion, setDisplayedBgChampion] = useState(topBgChampion);
  const [bgOpacity, setBgOpacity] = useState(1);
  const [isCompactLayout, setIsCompactLayout] = useState(false);
  const [compactTier, setCompactTier] = useState<CompactDraftTier>("mid");

  useEffect(() => {
    if (topBgChampion === displayedBgChampion) return;

    setBgOpacity(0);
    const timeoutId = setTimeout(() => {
      setDisplayedBgChampion(topBgChampion);
      setBgOpacity(1);
    }, 140);

    return () => clearTimeout(timeoutId);
  }, [displayedBgChampion, topBgChampion]);

  useEffect(() => {
    const updateCompactLayout = () => {
      const isLandscape = window.innerWidth > window.innerHeight;
      const isCoarsePointer = window.matchMedia("(pointer: coarse)").matches;
      const hasTouch = navigator.maxTouchPoints > 0;
      const isAndroid = /Android/i.test(navigator.userAgent);
      const hasTightHeight = window.innerHeight <= 900;
      const looksLikePhoneScreen = Math.min(window.screen.width, window.screen.height) <= 900;
      const shortEdge = Math.min(window.innerWidth, window.innerHeight);

      if (shortEdge <= 390) {
        setCompactTier("short");
      } else if (shortEdge <= 430) {
        setCompactTier("mid");
      } else {
        setCompactTier("tall");
      }

      setIsCompactLayout(
        isAndroid || ((isCoarsePointer || hasTouch || looksLikePhoneScreen) && isLandscape && hasTightHeight),
      );
    };

    updateCompactLayout();
    window.addEventListener("resize", updateCompactLayout);
    return () => window.removeEventListener("resize", updateCompactLayout);
  }, []);

  const blueTriCode = teamTriCode(snapshot.home_team.name);
  const redTriCode = teamTriCode(snapshot.away_team.name);
  const controlledTriCode = controlledSide === "blue" ? blueTriCode : redTriCode;
  const blueLogo = teamLogo(snapshot.home_team.name, gameState?.teams);
  const redLogo = teamLogo(snapshot.away_team.name, gameState?.teams);
  const seriesSquares = seriesLength === 5 ? 3 : seriesLength === 3 ? 2 : 0;
  const scoreDelta = controlledScore.total - rivalScore.total;
  const blueWinProb = Math.max(5, Math.min(95, Math.round(50 + scoreDelta * 4)));
  const grade =
    scoreDelta >= 6 ? "S" : scoreDelta >= 3 ? "A" : scoreDelta >= 0 ? "B" : scoreDelta >= -3 ? "C" : "D";
  const scoreRows = [
    { label: t("match.draft.scoreLabels.counter"), value: controlledScore.counter },
    { label: t("match.draft.scoreLabels.synergy"), value: controlledScore.synergy },
    { label: t("match.draft.scoreLabels.mastery"), value: controlledScore.mastery },
    { label: t("match.draft.scoreLabels.comfort"), value: controlledScore.comfort },
    { label: t("match.draft.scoreLabels.preparation"), value: controlledScore.preparation },
  ];
  const controlledScrimBonusTotal =
    controlledScrimSignal.comfort + controlledScrimSignal.preparation + controlledScrimSignal.synergy;
  const formattedScoreDelta = scoreDelta >= 0 ? `+${scoreDelta}` : `${scoreDelta}`;
  const seriesBansRequiresTwoRows = seriesLength > 1 && seriesLockedChampions.length > 10;
  const compactBoardLayoutClass =
    compactTier === "short"
      ? "h-[78px] grid-cols-[repeat(5,minmax(0,1fr))_74px_repeat(5,minmax(0,1fr))]"
      : compactTier === "mid"
        ? "h-[92px] grid-cols-[repeat(5,minmax(0,1fr))_84px_repeat(5,minmax(0,1fr))]"
        : "h-[104px] grid-cols-[repeat(5,minmax(0,1fr))_96px_repeat(5,minmax(0,1fr))]";
  const compactPanelsColsClass =
    compactTier === "short"
      ? "grid-cols-[132px_minmax(0,1fr)_132px]"
      : compactTier === "mid"
        ? "grid-cols-[148px_minmax(0,1fr)_148px]"
        : "grid-cols-[164px_minmax(0,1fr)_164px]";
  const topSectionHeightClass = seriesBansRequiresTwoRows ? "h-28" : "h-20";

  return (
    <div className={`h-dvh bg-background text-foreground p-2 md:p-4 ${isCompactLayout ? "overflow-y-auto" : "overflow-hidden"}`}>
      <div className={`w-full max-w-[2000px] 2xl:max-w-[90vw] mx-auto flex flex-col gap-2 md:gap-3 ${isCompactLayout ? "min-h-full overflow-visible pb-3" : "h-full overflow-hidden"}`}>
        <section className="order-2 shrink-0 rounded-md overflow-hidden border border-border">
          <div
            className={`relative flex items-stretch bg-linear-to-b from-card via-bg-card to-[#000] border-b-4 border-primary shadow-[0_0_16px_rgba(0,242,255,0.35)] ${topSectionHeightClass}`}
          >
            <div
              className="absolute inset-0"
              style={{
                backgroundImage: `url(${splashUrl(displayedBgChampion)})`,
                backgroundPosition: "center",
                backgroundSize: "cover",
                opacity: bgOpacity * 0.24,
                transition: "opacity 220ms ease",
              }}
            />
            <div className="absolute inset-0 bg-background/72" />

            <div className="relative z-10 border-t-2 border-primary/90 bg-background/40 w-[250px] p-2.5">
              <p className="font-heading uppercase font-bold tracking-[0.08em] mb-1 truncate text-primary [text-shadow:0_0_10px_rgba(0,0,0,0.9)] text-sm">
                {blueHeader}
              </p>
              <div className="flex gap-1">
                {blueBanDisplay.map((championId, index) => {
                  const champion = championId ? championById.get(championId) : null;
                  return (
                    <button
                      key={`top-blue-ban-${index}`}
                      disabled
                      className="relative border border-border/50 bg-background overflow-hidden w-8 h-8"
                    >
                      {champion ? (
                        <img
                          src={champion.image}
                          alt={champion.name}
                          className="w-full h-full object-cover grayscale opacity-70"
                        />
                      ) : null}
                      <span className="absolute top-1/2 -left-[10%] w-[120%] h-px bg-orange-500 -rotate-45" />
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="relative z-10 flex-1 flex flex-col items-center justify-center gap-1.5 px-2">
              {seriesLength > 1 && seriesLockedChampions.length > 0 ? (
                <div className="flex flex-col items-center gap-1">
                  <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground/30 font-heading font-bold">
                    {t("match.draft.seriesBans", { defaultValue: "SERIES BANS" })}
                  </p>
                  <div className="flex flex-wrap items-center justify-center gap-1 max-w-[520px]">
                    {seriesLockedChampions.map((champion) => (
                      <span
                        key={`series-lock-${champion.id}`}
                        className="relative w-8 h-8 border border-border/50 bg-background overflow-hidden"
                        title={champion.name}
                      >
                        <img
                          src={champion.image}
                          alt={champion.name}
                          className="w-full h-full object-cover grayscale opacity-80"
                        />
                        <span className="absolute top-1/2 -left-[10%] w-[120%] h-px bg-orange-500 -rotate-45" />
                      </span>
                    ))}
                  </div>
                </div>
              ) : null}

              <p className="text-xs uppercase tracking-[0.2em] text-muted-foreground/50 font-semibold">
                {t("match.draft.championSelection")}
              </p>
            </div>

            <div className="relative z-10 text-right border-t-2 border-orange-500/80 bg-background/40 w-[250px] p-2.5">
              <p className="font-heading uppercase font-bold tracking-[0.08em] mb-1 truncate text-orange-400 [text-shadow:0_0_10px_rgba(0,0,0,0.9)] text-sm">
                {redHeader}
              </p>
              <div className="flex gap-1 justify-end">
                  {redBanDisplay.map((championId, index) => {
                  const champion = championId ? championById.get(championId) : null;
                  return (
                    <button
                      key={`top-red-ban-${index}`}
                      disabled
                      className="relative border border-border/50 bg-background overflow-hidden w-11 h-11"
                    >
                      {champion ? (
                        <img
                          src={champion.image}
                          alt={champion.name}
                          className="w-full h-full object-cover grayscale opacity-70"
                        />
                      ) : null}
                      <span className="absolute top-1/2 -left-[10%] w-[120%] h-px bg-orange-500 -rotate-45" />
                    </button>
                  );
                })}
              </div>
            </div>
          </div>

          <div className={`${isCompactLayout ? `${compactBoardLayoutClass} grid` : "h-[340px] grid grid-cols-[repeat(5,minmax(0,1fr))_240px_repeat(5,minmax(0,1fr))]"} border-b-4 border-primary shadow-[0_0_16px_rgba(0,242,255,0.45)]`}>
            {blueOrderedPicks.map((pick, index) => (
              <DraftSlot
                key={`blue-slot-${ROLE_ORDER[index]}`}
                side="blue"
                playerName={bluePlayerLabels[index] ?? ROLE_ORDER[index]}
                role={ROLE_ORDER[index]}
                pick={pick}
                championById={championById}
                showSwapControls={finished && controlledSide === "blue"}
                onSwapArm={() => armSwapFromRole("blue", index)}
                onSwapTarget={() => completeSwapOnRole("blue", index)}
                swapArmed={swapSource?.side === "blue" && swapSource.index === index}
                swapTargetable={!!swapSource && swapSource.side === "blue" && swapSource.index !== index}
                reorderFxActive={showFinalRoleReassignFx}
                compact={isCompactLayout}
              />
            ))}

            <div className={`bg-linear-to-b from-card via-bg-card to-[#000] border-t-4 border-white flex flex-col items-center justify-between ${isCompactLayout ? "py-1 px-0.5" : "py-2 px-1"}`}>
              <div className={`flex items-center justify-between min-h-2 ${isCompactLayout ? "w-full mt-0" : "w-[88%] mt-1"}`}>
                {seriesSquares > 0 ? (
                  <div className="flex gap-1">
                    {Array.from({ length: seriesSquares }).map((_, index) => (
                      <span
                        key={`blue-series-${index}`}
                        className={`${isCompactLayout ? "w-2.5 h-1" : "w-4 h-2"} border border-white/60 ${index < blueSeriesWins ? "bg-white" : "bg-background/60"}`}
                      />
                    ))}
                  </div>
                ) : (
                  <span />
                )}
                {seriesSquares > 0 ? (
                  <div className="flex gap-1">
                    {Array.from({ length: seriesSquares }).map((_, index) => (
                      <span
                        key={`red-series-${index}`}
                        className={`${isCompactLayout ? "w-2.5 h-1" : "w-4 h-2"} border border-white/60 ${index < redSeriesWins ? "bg-white" : "bg-background/60"}`}
                      />
                    ))}
                  </div>
                ) : (
                  <span />
                )}
              </div>

              <div className={`${isCompactLayout ? "w-full flex items-center justify-between gap-1 mt-0.5 px-0.5" : "w-[92%] grid grid-cols-[92px_auto_92px] items-center justify-center gap-2 mt-2"}`}>
                <div className={`${isCompactLayout ? "flex flex-col items-center min-w-0" : "flex flex-col items-center w-[92px]"}`}>
                  {!isCompactLayout && blueLogo ? (
                    <img
                      src={blueLogo}
                      alt={blueTriCode}
                      className="w-11 h-11 object-contain mb-1 drop-shadow-[0_0_8px_rgba(0,0,0,0.7)]"
                    />
                  ) : null}
                  <p
                    className={`w-full text-center ${isCompactLayout ? "text-2xs" : tricodeSizeClass(blueTriCode)} leading-none font-black tracking-tight uppercase`}
                  >
                    {blueTriCode}
                  </p>
                </div>

                {isCompactLayout ? (
                  <p className="text-2xs font-bold tracking-[0.12em] text-muted-foreground/50">VS</p>
                ) : (
                  <div className="w-10 h-10" />
                )}

                <div className={`${isCompactLayout ? "flex flex-col items-center min-w-0" : "flex flex-col items-center w-[92px]"}`}>
                  {!isCompactLayout && redLogo ? (
                    <img
                      src={redLogo}
                      alt={redTriCode}
                      className="w-11 h-11 object-contain mb-1 drop-shadow-[0_0_8px_rgba(0,0,0,0.7)]"
                    />
                  ) : null}
                  <p
                    className={`w-full text-center ${isCompactLayout ? "text-2xs" : tricodeSizeClass(redTriCode)} leading-none font-black tracking-tight uppercase`}
                  >
                    {redTriCode}
                  </p>
                </div>
              </div>

              {!isCompactLayout ? (
                <div className="size-10 overflow-hidden rounded-md mt-1">
                  <img
                    src={leagueLogo}
                    alt={t("match.draft.leagueLogoAlt")}
                    className="size-full object-cover"
                  />
                </div>
              ) : null}
              <p className={`${isCompactLayout ? "text-2xs tracking-[0.08em] mb-0.5" : "text-xs tracking-[0.15em] mb-1"} text-muted-foreground/50 uppercase`}>
                {t("match.draft.patchLabel", { patch: patchLabel })}
              </p>
            </div>

            {redOrderedPicks.map((pick, index) => (
              <DraftSlot
                key={`red-slot-${ROLE_ORDER[index]}`}
                side="red"
                playerName={redPlayerLabels[index] ?? ROLE_ORDER[index]}
                role={ROLE_ORDER[index]}
                pick={pick}
                championById={championById}
                showSwapControls={finished && controlledSide === "red"}
                onSwapArm={() => armSwapFromRole("red", index)}
                onSwapTarget={() => completeSwapOnRole("red", index)}
                swapArmed={swapSource?.side === "red" && swapSource.index === index}
                swapTargetable={!!swapSource && swapSource.side === "red" && swapSource.index !== index}
                reorderFxActive={showFinalRoleReassignFx}
                compact={isCompactLayout}
              />
            ))}
          </div>
        </section>

        <section className={`order-1 rounded-md border border-primary/25 bg-background p-2 md:p-3 shadow-[0_0_28px_rgba(18,215,255,0.06)] ${isCompactLayout ? "shrink-0 overflow-visible" : "flex-1 min-h-0 overflow-hidden"}`}>
          <div className={`grid gap-2 md:gap-3 items-stretch ${isCompactLayout ? compactPanelsColsClass : "grid-cols-1 xl:grid-cols-[270px_minmax(0,1fr)_270px] 2xl:grid-cols-[320px_minmax(0,1fr)_320px]"} ${isCompactLayout ? "h-auto min-h-0" : "h-full min-h-0"}`}>
            <aside className={`${isCompactLayout ? "flex" : "hidden xl:flex"} lg:w-[280px] xl:w-[300px] h-full flex-col gap-2 min-h-0 overflow-y-auto scrollbar-draft pr-1`}>
              {assistantCoachTips.length > 0 ? (
                <div className="rounded-md border border-primary/25 bg-background p-3 text-xs text-gray-200">
                  <p className="font-heading uppercase tracking-wide text-xs text-foreground mb-2">
                    {t("match.draft.tipsTitle")}
                  </p>
                  {(() => {
                    const tip = assistantCoachTips[0];
                    if (!tip) return null;
                    return (
                      <article className="rounded-md border border-white/10 bg-[#070915] px-2.5 py-2">
                        <div className="flex items-start gap-2">
                          <img
                            src={tip.sourceImage}
                            alt={tip.sourceName}
                            className="w-8 h-8 rounded-full object-cover border border-primary/40 bg-[#090d1f]"
                            loading="lazy"
                            onError={(event) => {
                              const target = event.currentTarget;
                              if (!ASSISTANT_COACH_PLACEHOLDER) {
                                target.style.display = "none";
                                return;
                              }
                              if (target.src === ASSISTANT_COACH_PLACEHOLDER || target.src.endsWith(ASSISTANT_COACH_PLACEHOLDER)) return;
                              target.src = ASSISTANT_COACH_PLACEHOLDER;
                            }}
                          />

                          <div className="min-w-0 flex-1">
                            <p className="text-sm font-semibold leading-none text-foreground truncate">
                              {tip.sourceName}
                            </p>
                            <div className="mt-1 flex items-center gap-1">
                              {tip.sourceType === "player" && tip.sourceRole && ROLE_ORDER.includes(tip.sourceRole as Role) ? (
                                <img
                                  src={ROLE_ICON_URLS[tip.sourceRole as Role]}
                                  alt={tip.sourceRole}
                                  className="w-3 h-3 invert opacity-75"
                                  loading="lazy"
                                />
                              ) : null}
                              <p className="text-2xs uppercase tracking-wide text-cyan-300/90">
                                {tip.sourceRole
                                  ?? (tip.sourceType === "coach"
                                    ? t("match.draft.assistantCoach")
                                    : t("match.draft.playerLabel"))}
                              </p>
                            </div>
                          </div>
                        </div>

                        <p
                          className={`mt-2 text-xs leading-snug ${
                            tip.type === "ban" ? "text-orange-200" : tip.type === "pick" ? "text-cyan-200" : "text-yellow-200"
                          }`}
                        >
                          “{tip.text}”
                        </p>

                        {tip.champion ? (
                          <div className="mt-2 pt-2 border-t border-white/10 flex items-center gap-2">
                            <img
                              src={tip.champion.image}
                              alt={tip.champion.name}
                              className="w-7 h-7 rounded-sm object-cover border border-orange-400/60"
                              loading="lazy"
                            />
                            <span className="text-xs text-gray-200 truncate">{tip.champion.name}</span>
                          </div>
                        ) : null}
                      </article>
                    );
                  })()}
                </div>
              ) : (
                <div className="rounded-md border border-primary/25 bg-background p-3 text-xs text-gray-200">
                  <p className="font-heading uppercase tracking-wide text-xs text-foreground mb-2">
                    {t("match.draft.tipsTitle")}
                  </p>
                  <p className="text-xs mb-1">• {t("match.draft.tip1")}</p>
                  <p className="text-xs mb-1">• {t("match.draft.tip2")}</p>
                  <p className="text-xs">• {t("match.draft.tip3")}</p>
                </div>
              )}

              <div className="rounded-md border border-orange-400/30 bg-background p-3 text-xs text-gray-200">
                <p className="font-heading uppercase tracking-wide text-xs text-foreground mb-2">
                  {t("match.draft.scoreTitle")}
                </p>
                <div className="mb-2 text-xs text-muted-foreground/50">
                  <p>
                    {t("match.draft.winProb")} <span className="text-cyan-300 font-semibold">{controlledTriCode} {blueWinProb}%</span>
                  </p>
                </div>
                <div className="space-y-1 text-xs">
                  {scoreRows.map((row) => (
                    <p key={row.label} className="text-muted-foreground/50">
                      {row.label}
                      <span className={`float-right font-semibold ${row.value >= 0 ? "text-cyan-300" : "text-red-400"}`}>
                        {row.value >= 0 ? `+${row.value}` : row.value}
                      </span>
                    </p>
                  ))}
                </div>
                {controlledScrimBonusTotal > 0 ? (
                  <div className="mt-2 rounded border border-primary/20 bg-primary/5 px-2 py-1.5 text-2xs text-primary">
                    <p className="font-semibold uppercase tracking-wide">
                      {t("match.draft.scrimSignalTitle", { defaultValue: "Scrim prep" })} +{controlledScrimBonusTotal}
                    </p>
                    <p className="mt-1 text-primary/80">
                      {controlledScrimSignal.reasons.join(" · ")}
                    </p>
                  </div>
                ) : null}
                <div className="mt-2 pt-2 border-t border-white/10 text-xs">
                  <p className="text-muted-foreground/50">
                    {t("match.draft.total")} <span className="float-right font-bold text-foreground">{controlledScore.total}</span>
                  </p>
                  <p className="text-muted-foreground/50 mt-1">
                    {t("match.draft.rivalTotal")} <span className="float-right font-bold text-gray-200">{rivalScore.total}</span>
                  </p>
                  <p className="text-muted-foreground/50 mt-1">
                    {t("match.draft.scoreDelta")} <span className={`float-right font-bold ${scoreDelta >= 0 ? "text-cyan-300" : "text-red-400"}`}>{formattedScoreDelta}</span>
                  </p>
                  <p className="text-muted-foreground/50 mt-1">
                    {t("match.draft.grade")} <span className="float-right font-bold text-orange-300">{grade}</span>
                  </p>
                </div>
              </div>
            </aside>

            <div className={`rounded-md border border-primary/25 bg-background/95 p-3 space-y-2 relative shadow-[inset_0_0_0_1px_rgba(255,255,255,0.03)] flex flex-col ${isCompactLayout ? "h-auto min-h-[60dvh] overflow-visible" : "h-full min-h-0 overflow-hidden"}`}>
              <div className="absolute inset-0 pointer-events-none bg-[radial-gradient(circle_at_35%_45%,rgba(0,209,255,0.12),transparent_40%),radial-gradient(circle_at_70%_45%,rgba(255,146,56,0.1),transparent_45%)]" />

              <div className="relative grid grid-cols-1 lg:grid-cols-[1fr_auto_auto] gap-2 items-center">
                <div className="flex flex-wrap gap-1">
                  <button
                    className={`px-2 py-1 text-xs border rounded ${roleFilter === "ALL" ? "border-cyan-300/80 bg-primary/10 text-primary" : "border-white/20 text-gray-200"}`}
                    onClick={() => setRoleFilter("ALL")}
                  >
                    ALL
                  </button>
                  {ROLE_ORDER.map((role) => (
                      <button
                        key={`chip-${role}`}
                        className={`px-2 py-1 text-xs border rounded ${roleFilter === role ? "border-cyan-300/80 bg-primary/10 text-primary" : "border-white/20 text-gray-200"}`}
                        onClick={() => setRoleFilter(role)}
                      >
                      {role}
                    </button>
                  ))}
                </div>

                <div className="flex items-center gap-1">
                  {(["alpha", "meta", "mastery"] as const).map((mode) => (
                    <button
                      key={`sort-${mode}`}
                      className={`px-2 py-1 text-xs border rounded ${sortMode === mode ? "border-orange-300/80 bg-orange-500/10 text-orange-400" : "border-white/20 text-gray-200"}`}
                      onClick={() => setSortMode(mode)}
                    >
                      {mode === "alpha" ? "A-Z" : mode === "meta" ? "META" : "MAST"}
                    </button>
                  ))}
                </div>

                <div className="flex items-center gap-1">
                  {(["ALL", "S", "A", "B", "C", "D"] as const).map((tier) => (
                    <button
                      key={`tier-${tier}`}
                      className={`px-2 py-1 text-xs border rounded ${metaTierFilter === tier ? "border-cyan-300/80 bg-primary/10 text-primary" : "border-white/20 text-gray-200"}`}
                      onClick={() => setMetaTierFilter(tier)}
                    >
                      {tier}
                    </button>
                  ))}
                </div>

                <input
                  value={searchTerm}
                  onChange={(event) => setSearchTerm(event.target.value)}
                  className="rounded-md bg-card border border-white/15 px-2 py-1 text-xs md:py-1.5 md:text-xs w-36 md:w-44"
                  placeholder={t("match.draft.searchPlaceholder")}
                />

              </div>

              <div className="relative text-center text-gray-200 text-xs">
                {finished
                  ? t("match.draft.completed")
                  : `${actionLabel} · ${sideLabel} ${currentStep?.label ?? ""}`}
                <span className="ml-2 text-gray-400">
                  {t("match.draft.stepCounter", {
                    current: currentStepNumber,
                    total: totalSteps,
                  })}
                </span>
                {!finished ? (
                  <span className="ml-2 font-semibold text-muted-foreground/50">
                    {Math.ceil(turnRemainingMs / 1000)}s
                  </span>
                ) : null}
              </div>

              {!finished && debugToolsEnabled ? (
                <div className="relative flex justify-end">
                  <button
                    type="button"
                    onClick={handleSkipDraftDebug}
                    className="rounded-md border border-fuchsia-300/60 bg-fuchsia-500/15 hover:bg-fuchsia-500/25 text-fuchsia-100 px-2 py-1 text-2xs font-heading font-bold uppercase tracking-wide"
                  >
                    Skip draft (debug)
                  </button>
                </div>
              ) : null}

              <div className="relative w-full h-1.5 rounded-full bg-navy-900 overflow-hidden">
                <div
                  className={`h-full transition-[width] duration-100 ease-linear ${currentStep?.side === "blue" ? "bg-primary" : "bg-orange-500"}`}
                  style={{ width: `${Math.max(0, Math.min(100, (turnRemainingMs / Math.max(1, turnDurationMs)) * 100))}%` }}
                />
              </div>

              {!finished && isUserTurn ? (
                <div className="relative flex items-center justify-between gap-2 rounded-md border border-white/12 bg-navy-900 px-2 py-1.5">
                  <p className="text-xs text-muted-foreground/50 truncate">
                    {pendingChampionId
                      ? `${t("match.draft.selected")}: ${championById.get(pendingChampionId)?.name ?? pendingChampionId}`
                      : t("match.draft.selectThenConfirm")}
                  </p>
                  <button
                    type="button"
                    onClick={handleConfirmPendingAction}
                    disabled={!canUserConfirmDraftChoice(pendingChampionId, isUserTurn)}
                    className="rounded-md bg-orange-500 hover:bg-orange-400 disabled:opacity-40 disabled:cursor-not-allowed text-navy-900 px-3 py-1 text-xs font-heading font-bold uppercase tracking-wide"
                  >
                    {currentStep?.type === "ban"
                      ? t("match.draft.actions.ban")
                      : t("match.draft.actions.pick")}
                  </button>
                </div>
              ) : null}
              {pendingEvaluation ? (
                <div className="relative grid grid-cols-4 gap-1 rounded-md border border-orange-500/30 bg-orange-500/5 px-2 py-1.5 text-2xs">
                  <span>Meta {pendingEvaluation.meta_power}</span>
                  <span>Mastery {pendingEvaluation.mastery}</span>
                  <span>Skill fit {pendingEvaluation.skill_fit}</span>
                  <span className="text-orange-300">Risk {pendingEvaluation.execution_risk}</span>
                </div>
              ) : null}

                <div className="relative min-h-0 flex-1 overflow-y-auto scrollbar-draft pr-1">
                {loading ? (
                  <p className="relative text-sm text-muted-foreground/50">{t("match.draft.loadingChampions")}</p>
                ) : error ? (
                  <p className="relative text-sm text-red-300">
                    {t("match.draft.loadError")}: {error}
                  </p>
                ) : visibleChampions.length === 0 ? (
                  <p className="relative text-sm text-muted-foreground/50">{t("match.draft.noChampionsForFilters")}</p>
                ) : (
                  <div className="relative grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-5 2xl:grid-cols-10 gap-2">
                    {visibleChampions.map((champion) => {
                      const isUsed = usedChampionIds.has(champion.id);
                      const showMastery = roleFilter !== "ALL";
                      const mastery = showMastery ? displayMasteryForChampion(champion) : 0;
                      const masteryTone = masteryBarTone(mastery);
                      const metaTier = knownMetaTierForChampion(
                        champion,
                        runtimeMetaScoreByChampion,
                        discoveredMetaChampionIds,
                      );
                      return (
                        <button
                          key={champion.id}
                          onClick={() => handleChampionTileClick(champion)}
                          disabled={finished || !isUserTurn || isUsed}
                          className={`rounded-sm border transition-colors p-1 text-left overflow-hidden ${isUsed ? "border-white/10 bg-background" : pendingChampionId === champion.id ? "border-orange-400 bg-orange-500/10" : "border-white/15 bg-card hover:border-orange-400"}`}
                        >
                          <img
                            src={champion.image}
                            alt={champion.name}
                            className={`w-full aspect-square object-cover ${isUsed ? "grayscale opacity-45" : ""}`}
                            loading="lazy"
                          />
                          <p
                            className={`px-2 py-1 text-xs font-semibold truncate border-t ${isUsed ? "bg-background border-white/5 text-gray-500" : "bg-background border-white/10"}`}
                          >
                            {champion.name}
                          </p>
                          <div className="px-2 pb-1.5">
                            <div className="flex items-center justify-between text-xs font-bold">
                              <span className={metaTier === "?" ? "text-gray-500" : metaTier === "S" ? "text-red-400" : metaTier === "A" ? "text-orange-300" : "text-slate-300"}>{metaTier}</span>
                              {showMastery ? <span className="text-gray-400">{mastery}</span> : null}
                            </div>
                            {showMastery ? (
                              <div className="mt-0.5 h-1 rounded bg-background/40 overflow-hidden">
                                <div
                                  className={`h-full ${masteryTone === "gold" ? "bg-amber-400" : masteryTone === "green" ? "bg-emerald-400" : "bg-rose-400"}`}
                                  style={{ width: `${Math.max(0, Math.min(100, mastery))}%` }}
                                />
                              </div>
                            ) : null}
                          </div>
                        </button>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>

            <aside className={`${isCompactLayout ? "flex" : "hidden xl:flex"} lg:w-[280px] xl:w-[300px] h-full flex-col rounded-md border border-primary/25 bg-background p-2 min-h-0 overflow-y-auto scrollbar-draft pr-1`}>
              {selectedChampionCounterIntel ? (
                <>
                  <p className="text-xs font-heading uppercase tracking-wide text-gray-200 mb-2 text-right">
                    {t("match.draft.counterIntelTitle", {
                      defaultValue: "Counter intel · {{champion}}",
                      champion: selectedChampionCounterIntel.selectedChampion.name,
                    })}
                  </p>
                  <p className="text-2xs text-cyan-200/80 mb-2 text-right">
                    {selectedChampionCounterIntel.consulted
                      ? t("match.draft.counterIntelScaleFull", {
                        defaultValue: "Consulta aplicada: vista completa de counters",
                      })
                      : t("match.draft.counterIntelScale", {
                        defaultValue: "Meta reading: top {{count}} each side",
                        count: selectedChampionCounterIntel.revealBudget,
                      })}
                  </p>
                  <div className="mb-2 flex justify-end">
                    <button
                      type="button"
                      onClick={handleConsultStaffCounterIntel}
                      disabled={
                        !pendingChampionId
                        || counterConsultUsesLeft <= 0
                        || consultedCounterChampionIds.has(pendingChampionId)
                      }
                      className="rounded-md border border-cyan-300/40 bg-cyan-500/10 hover:bg-cyan-500/20 disabled:opacity-40 disabled:cursor-not-allowed text-primary px-2 py-1 text-2xs font-heading font-bold uppercase tracking-wide"
                    >
                      {t("match.draft.counterIntelConsult", { defaultValue: "Consulta al staff" })}
                      {` (${counterConsultUsesLeft})`}
                    </button>
                  </div>

                  <div className="space-y-1.5">
                    <p className="text-2xs font-heading uppercase tracking-wide text-cyan-200">
                      {t("match.draft.counterIntelPros", { defaultValue: "In your favor" })}
                    </p>
                    {selectedChampionCounterIntel.favorable.length === 0 ? (
                      <div className="rounded-sm border border-white/10 bg-card p-2 text-2xs text-gray-500 text-center">
                        {t("match.draft.counterIntelNoPros", { defaultValue: "No clear favorable counters detected." })}
                      </div>
                    ) : null}
                    {selectedChampionCounterIntel.favorable.map((row) => (
                      <div
                        key={`counter-pro-${row.champion.id}`}
                        className={`rounded-sm border p-1 ${row.isPicked ? "border-cyan-300/70 bg-cyan-500/10" : "border-white/10 bg-card"}`}
                      >
                        <div className="flex items-center gap-2">
                          <img src={row.champion.image} alt={row.champion.name} className="w-9 h-9 object-cover rounded-sm" loading="lazy" />
                          <div className="min-w-0 flex-1">
                            <p className="text-xs truncate">{row.champion.name}</p>
                            <p className={`text-2xs uppercase tracking-wide ${row.isPicked ? "text-cyan-200" : "text-gray-400"}`}>
                              {row.isPicked
                                ? t("match.draft.counterIntelPicked", { defaultValue: "Already picked by rival" })
                                : t("match.draft.counterIntelPotential", { defaultValue: "Potential rival pick" })}
                            </p>
                          </div>
                          <p className="text-xs text-cyan-300 font-semibold">+{row.value}</p>
                        </div>
                      </div>
                    ))}

                    <p className="pt-1 text-2xs font-heading uppercase tracking-wide text-orange-200">
                      {t("match.draft.counterIntelCons", { defaultValue: "Against your pick" })}
                    </p>
                    {selectedChampionCounterIntel.risky.length === 0 ? (
                      <div className="rounded-sm border border-white/10 bg-card p-2 text-2xs text-gray-500 text-center">
                        {t("match.draft.counterIntelNoCons", { defaultValue: "No immediate hard counters found." })}
                      </div>
                    ) : null}
                    {selectedChampionCounterIntel.risky.map((row) => (
                      <div
                        key={`counter-con-${row.champion.id}`}
                        className={`rounded-sm border p-1 ${row.isPicked ? "border-orange-300/75 bg-orange-500/10" : "border-white/10 bg-card"}`}
                      >
                        <div className="flex items-center gap-2">
                          <img src={row.champion.image} alt={row.champion.name} className="w-9 h-9 object-cover rounded-sm" loading="lazy" />
                          <div className="min-w-0 flex-1">
                            <p className="text-xs truncate">{row.champion.name}</p>
                            <p className={`text-2xs uppercase tracking-wide ${row.isPicked ? "text-orange-200" : "text-gray-400"}`}>
                              {row.isPicked
                                ? t("match.draft.counterIntelPicked", { defaultValue: "Already picked by rival" })
                                : t("match.draft.counterIntelPotential", { defaultValue: "Potential rival pick" })}
                            </p>
                          </div>
                          <p className="text-xs text-orange-300 font-semibold">-{row.value}</p>
                        </div>
                      </div>
                    ))}
                  </div>
                </>
              ) : (
                <>
                  <p className="text-xs font-heading uppercase tracking-wide text-gray-200 mb-2 text-right">
                    {t("match.draft.enemyComfortTitle")}
                  </p>
                  <div className="space-y-1">
                    {rivalMasteryDisplay.length === 0 ? (
                      <div className="rounded-sm border border-white/10 bg-card p-2 text-2xs text-gray-500 text-center">
                        {t("match.draft.enemyComfortUnknown")}
                      </div>
                    ) : null}
                    {rivalMasteryDisplay.map(({ champion, mastery, playerName, source }) => (
                      <div key={`rival-row-${champion.id}-${playerName}-${source}`} className="rounded-sm border border-white/10 bg-card p-1">
                        <div className="flex items-center gap-2">
                          <img
                            src={champion.image}
                            alt={champion.name}
                            className="w-10 h-10 object-cover rounded-sm"
                            loading="lazy"
                          />
                          <div className="min-w-0 flex-1">
                            <p className="text-xs truncate">{champion.name}</p>
                            <p className="text-2xs text-gray-400 truncate">{playerName}</p>
                            <p className="text-2xs uppercase tracking-wide text-cyan-200/70 truncate">
                              {source === "insignia"
                                ? t("match.draft.masterySourceSignature")
                                : source === "scouting"
                                  ? t("match.draft.masterySourceScouting")
                                  : t("match.draft.masterySourceStaff")}
                            </p>
                            <div className="mt-1 h-1.5 bg-background/35 rounded overflow-hidden">
                              <div className="h-full bg-primary" style={{ width: `${Math.min(100, mastery)}%` }} />
                            </div>
                          </div>
                          <p className="text-xs text-cyan-300 font-semibold">{mastery}</p>
                        </div>
                      </div>
                    ))}
                  </div>
                </>
              )}
            </aside>
          </div>
        </section>

        {finished ? (
          <button
            className="w-full md:w-80 self-center rounded-md bg-orange-500 hover:bg-orange-400 text-navy-900 py-2 font-heading font-bold uppercase tracking-wide"
            onClick={() =>
              onComplete({
                blue: {
                  picks: blueOrderedPicks.filter((pick): pick is DraftPick => pick !== null),
                  bans: blueBans,
                  score: blueScore,
                },
                red: {
                  picks: redOrderedPicks.filter((pick): pick is DraftPick => pick !== null),
                  bans: redBans,
                  score: redScore,
                },
                history: draftHistory,
              })
            }
          >
            {t("match.draft.finalizeDraft")}
          </button>
        ) : null}
      </div>
    </div>
  );
}

function DraftSlot({
  side,
  playerName,
  role,
  pick,
  championById,
  showSwapControls,
  onSwapArm,
  onSwapTarget,
  swapArmed,
  swapTargetable,
  reorderFxActive,
  compact = false,
}: {
  side: Side;
  playerName: string;
  role: Role;
  pick: DraftPick | null;
  championById: Map<string, ChampionData>;
  showSwapControls: boolean;
  onSwapArm: () => void;
  onSwapTarget: () => void;
  swapArmed: boolean;
  swapTargetable: boolean;
  reorderFxActive: boolean;
  compact?: boolean;
}) {
  const { t } = useTranslation();
  const champion = pick ? championById.get(pick.championId) : null;

  return (
    <div
      onClick={() => {
        if (!showSwapControls || !swapTargetable) return;
        onSwapTarget();
      }}
      className={`relative bg-background overflow-hidden border-r border-white/10 text-left transition-all duration-300 ${side === "blue" ? "border-t-4 border-t-cyan-400" : "border-t-4 border-t-orange-500"} ${swapTargetable ? "ring-1 ring-orange-300/70" : ""} ${swapArmed ? "ring-2 ring-cyan-300" : ""} ${reorderFxActive ? "shadow-[0_0_16px_rgba(34,211,238,0.45)] scale-[1.01]" : ""}`}
    >
      {champion ? (
        <>
          <img
            src={loadingUrl(champion.id)}
            alt={champion.name}
            className="absolute inset-0 w-full h-full object-cover object-top opacity-95"
          />
          <div className="absolute inset-0 bg-linear-to-t from-black/20 via-transparent to-transparent" />
        </>
      ) : null}

      <div
        className={`absolute inset-y-0 ${side === "blue" ? "left-0 border-r border-r-cyan-300/40" : "right-0 border-l border-l-orange-400/45"} ${compact ? "w-5" : "w-8"} bg-background/65 border-border/50 z-20 flex items-center justify-center`}
      >
        <span
          className={`${compact ? "text-2xs tracking-[0.04em]" : "text-sm tracking-[0.09em]"} font-black uppercase text-foreground z-30 leading-none [writing-mode:vertical-rl] [text-shadow:0_0_10px_rgba(0,0,0,0.95)] ${side === "blue" ? "rotate-180" : ""}`}
        >
          {playerName}
        </span>
      </div>

      <img
        src={ROLE_ICON_URLS[role]}
        alt={role}
        className={`absolute ${compact ? "bottom-1 w-[12px]" : "bottom-2 w-[18px]"} invert opacity-70 ${side === "blue" ? "right-2" : "left-2"}`}
      />

      {showSwapControls ? (
        <div className={`absolute bottom-2 z-30 flex flex-col gap-1 ${side === "blue" ? "left-10" : "right-10"}`}>
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onSwapArm();
            }}
            className={`w-5 h-5 rounded border text-2xs leading-none text-foreground ${swapArmed ? "bg-cyan-500/35 border-cyan-300/80" : "bg-background/65 border-border/50"}`}
            title={t("match.draft.swapTargetTitle")}
          >
            ▲
          </button>
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onSwapArm();
            }}
            className={`w-5 h-5 rounded border text-2xs leading-none text-foreground ${swapArmed ? "bg-cyan-500/35 border-cyan-300/80" : "bg-background/65 border-border/50"}`}
            title={t("match.draft.swapTargetTitle")}
          >
            ▼
          </button>
        </div>
      ) : null}
    </div>
  );
}
