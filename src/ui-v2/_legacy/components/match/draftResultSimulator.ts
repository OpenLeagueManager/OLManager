import type { GameStateData } from "@/store/gameStore";
import type { ChampionDraftResultPayload } from "@/ui-v2/_legacy/components/match/ChampionDraft";
import type { MatchSnapshot } from "@/ui-v2/_legacy/components/match/types";
import type { LolTacticsData } from "@/store/types";
import { calculateLolOvr } from "@/lib/players/lolPlayerStats";
import {
  DEFAULT_LOL_TACTICS,
  ROLE_ORDER,
  computeCoherenceBreakdown,
  computeRoleModifiers,
  type DraftRole,
} from "@/lib/teams/lolTactics";
import { computeTeamTimingFit, getChampionTiming } from "@/lib/champions/championTiming";

type Side = "blue" | "red";
type Role = DraftRole;

const ATTRIBUTE_KEYS = [
  "pace",
  "stamina",
  "strength",
  "agility",
  "passing",
  "shooting",
  "tackling",
  "dribbling",
  "defending",
  "positioning",
  "vision",
  "decisions",
  "composure",
  "aggression",
  "teamwork",
  "leadership",
] as const;

export interface DraftPlayerResult {
  side: Side;
  playerId: string;
  playerName: string;
  role: Role;
  championId: string | null;
  kills: number;
  deaths: number;
  assists: number;
  gold: number;
  rating: number;
}

export interface DraftTeamObjectives {
  voidgrubs: number;
  dragons: number;
  dragonSoul: boolean;
  elderDragons: number;
  heralds: number;
  barons: number;
  towers: number;
  inhibitors: number;
}

export interface DraftTimelineEvent {
  minute: number;
  side: Side;
  type:
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
    | "nexus";
  label: string;
}

export interface DraftMatchResult {
  winnerSide: Side;
  durationMinutes: number;
  blueKills: number;
  redKills: number;
  mvp: DraftPlayerResult;
  playerResults: DraftPlayerResult[];
  goldDiffTimeline: Array<{ minute: number; diff: number }>;
  timelineEvents: DraftTimelineEvent[];
  objectives: {
    blue: DraftTeamObjectives;
    red: DraftTeamObjectives;
  };
  power: {
    blue: number;
    red: number;
    diff: number;
    autoWin: boolean;
    winProbBlue: number;
  };
}

export interface DraftPickEvaluation {
  champion_id: string;
  meta_power: number;
  mastery: number;
  skill_fit: number;
  execution_risk: number;
  total: number;
  engine_modifier: number;
}

export interface DraftEvaluationsBySide {
  blue: DraftPickEvaluation[];
  red: DraftPickEvaluation[];
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function hashText(value: string): number {
  let hash = 0;
  for (let i = 0; i < value.length; i += 1) {
    hash = (hash * 31 + value.charCodeAt(i)) >>> 0;
  }
  return hash;
}

function mulberry32(seed: number): () => number {
  return () => {
    let t = (seed += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function average(values: number[]): number {
  if (values.length === 0) return 0;
  return values.reduce((sum, item) => sum + item, 0) / values.length;
}

function normalizeKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function gameStatePositionToDraftRole(position: string): Role | null {
  // position is already a LolRole ("TOP", "JUNGLE", "MID", "ADC", "SUPPORT")
  const normalized = normalizeKey(position);
  if (normalized === "top") return "TOP";
  if (normalized === "jungle") return "JUNGLE";
  if (normalized === "mid") return "MID";
  if (normalized === "adc" || normalized === "bot" || normalized === "bottom") return "ADC";
  if (normalized === "support" || normalized === "sup") return "SUPPORT";
  return null;
}

function toEnginePlayerFromState(
  player: GameStateData["players"][number],
): MatchSnapshot["home_team"]["players"][number] {
  return {
    id: player.id,
    name: player.match_name,
    role: player.position,
    condition: player.condition,
    fitness: player.fitness ?? 75,
    mechanics: player.attributes.mechanics,
    laning: player.attributes.laning,
    teamfighting: player.attributes.teamfighting,
    macro_play: player.attributes.macro_play,
    consistency: player.attributes.consistency,
    shotcalling: player.attributes.shotcalling,
    champion_pool: player.attributes.champion_pool,
    discipline: player.attributes.discipline,
    mental_resilience: player.attributes.mental_resilience,
    pace: player.attributes.mechanics,
    stamina: player.attributes.mental_resilience,
    strength: player.attributes.mental_resilience,
    agility: player.attributes.champion_pool,
    passing: player.attributes.teamfighting,
    shooting: player.attributes.laning,
    tackling: player.attributes.macro_play,
    dribbling: player.attributes.mechanics,
    defending: player.attributes.discipline,
    positioning: player.attributes.consistency,
    vision: player.attributes.macro_play,
    decisions: player.attributes.consistency,
    composure: player.attributes.discipline,
    aggression: player.attributes.shotcalling,
    teamwork: player.attributes.teamfighting,
    leadership: player.attributes.shotcalling,
    traits: player.traits,
  };
}

function resolvePlayersFromSeed(
  snapshot: MatchSnapshot,
  gameState: GameStateData,
  side: Side,
): MatchSnapshot["home_team"]["players"] {
  const team = side === "blue" ? snapshot.home_team : snapshot.away_team;
  const starters = side === "blue" ? snapshot.home_team.players : snapshot.away_team.players;

  const stateTeam = gameState.teams.find(
    (item) => normalizeKey(item.name) === normalizeKey(team.name),
  );
  if (!stateTeam) return starters.slice(0, 5);

  const stateTeamPlayers = gameState.players.filter(
    (player) => player.team_id === stateTeam.id,
  );
  if (stateTeamPlayers.length === 0) return starters.slice(0, 5);

  const selected: MatchSnapshot["home_team"]["players"] = [];
  const usedIds = new Set<string>();

  ROLE_ORDER.forEach((role) => {
    const roleSeed = stateTeamPlayers
      .filter((player) => gameStatePositionToDraftRole(player.position) === role)
      .sort((a, b) => (calculateLolOvr(b) - calculateLolOvr(a)))[0];
    if (!roleSeed) return;

    const match = stateTeamPlayers.find(
      (player) =>
        !usedIds.has(player.id) &&
        normalizeKey(player.match_name) === normalizeKey(roleSeed.match_name),
    );
    if (!match) return;

    selected.push(toEnginePlayerFromState(match));
    usedIds.add(match.id);
  });

  if (selected.length < 5 && stateTeamPlayers.length > 0) {
    ROLE_ORDER.forEach((role) => {
      if (selected.length >= 5) return;
      const fallback = stateTeamPlayers
        .filter(
          (player) =>
            !usedIds.has(player.id) &&
            gameStatePositionToDraftRole(player.position) === role,
        )
        .sort((a, b) => b.attributes.teamfighting + b.attributes.consistency - (a.attributes.teamfighting + a.attributes.consistency))[0];

      if (!fallback) return;
      selected.push(toEnginePlayerFromState(fallback));
      usedIds.add(fallback.id);
    });
  }

  if (selected.length < 5) {
    starters.forEach((player) => {
      if (selected.length >= 5) return;
      if (usedIds.has(player.id)) return;
      selected.push(player);
      usedIds.add(player.id);
    });
  }

  return selected.slice(0, 5);
}

function teamSideData(snapshot: MatchSnapshot, gameState: GameStateData, side: Side) {
  const team = side === "blue" ? snapshot.home_team : snapshot.away_team;
  const stateTeam = gameState.teams.find(
    (item) => normalizeKey(item.name) === normalizeKey(team.name),
  );
  const lineupPlayers =
    side === "blue" ? snapshot.home_team.players : snapshot.away_team.players;
  const resolvedPlayers =
    lineupPlayers.length >= 5
      ? lineupPlayers.slice(0, 5)
      : resolvePlayersFromSeed(snapshot, gameState, side);

  return side === "blue"
    ? {
      team: snapshot.home_team,
      players: resolvedPlayers,
      tactics: stateTeam?.lol_tactics ?? DEFAULT_LOL_TACTICS,
    }
    : {
      team: snapshot.away_team,
      players: resolvedPlayers,
      tactics: stateTeam?.lol_tactics ?? DEFAULT_LOL_TACTICS,
    };
}

function laneOvrByRole(players: MatchSnapshot["home_team"]["players"]): Record<Role, number> {
  const map: Record<Role, number> = {
    TOP: 70,
    JUNGLE: 70,
    MID: 70,
    ADC: 70,
    SUPPORT: 70,
  };

  players.forEach((player, index) => {
    const role = ROLE_ORDER[index] ?? "MID";
    map[role] = playerOverall(player);
  });

  return map;
}

function tacticsPowerBonus(params: {
  own: LolTacticsData;
  ownPlayers: MatchSnapshot["home_team"]["players"];
  enemyPlayers: MatchSnapshot["home_team"]["players"];
  ownPicks: ChampionDraftResultPayload["blue"]["picks"];
}): number {
  const { own, ownPlayers, enemyPlayers, ownPicks } = params;

  const ownLanesRaw = laneOvrByRole(ownPlayers);
  const roleModifiers = computeRoleModifiers(own);
  const pickByRole = new Map<Role, string | null>(
    ownPicks.map((pick) => [pick.role as Role, pick.championId]),
  );

  const adjustedRoleModifiers: Record<Role, number> = { ...roleModifiers };
  ROLE_ORDER.forEach((role) => {
    const current = roleModifiers[role];
    if (current >= 0) return;

    const championId = pickByRole.get(role) ?? null;
    const timing = getChampionTiming(championId);

    // Weakside tech: campeones de early ayudan a neutralizar castigo del rol débil.
    if (timing === "Early") {
      adjustedRoleModifiers[role] = Math.min(0, current + Math.abs(current));
    }
  });

  const ownLanes = {
    TOP: ownLanesRaw.TOP + adjustedRoleModifiers.TOP * 1.8,
    JUNGLE: ownLanesRaw.JUNGLE + adjustedRoleModifiers.JUNGLE * 1.8,
    MID: ownLanesRaw.MID + adjustedRoleModifiers.MID * 1.8,
    ADC: ownLanesRaw.ADC + adjustedRoleModifiers.ADC * 1.8,
    SUPPORT: ownLanesRaw.SUPPORT + adjustedRoleModifiers.SUPPORT * 1.8,
  };
  const enemyLanes = laneOvrByRole(enemyPlayers);

  let score = 0;

  const laneDelta = {
    Top: ownLanes.TOP - enemyLanes.TOP,
    Mid: ownLanes.MID - enemyLanes.MID,
    Bot: (ownLanes.ADC + ownLanes.SUPPORT) / 2 - (enemyLanes.ADC + enemyLanes.SUPPORT) / 2,
  };

  score += laneDelta[own.strong_side as keyof typeof laneDelta] * 0.22;

  const ownCondition = average(ownPlayers.map((player) => Number(player.condition ?? 70)));
  const ownComposure = average(ownPlayers.map((player) => Number(player.discipline ?? 70)));
  const ownTeamwork = average(ownPlayers.map((player) => Number(player.teamfighting ?? 70)));

  if (own.game_timing === "Early") score += (ownCondition - 72) * 0.15;
  if (own.game_timing === "Mid") score += (ownTeamwork - 70) * 0.11;
  if (own.game_timing === "Late") score += (ownComposure - 70) * 0.14;

  if (own.jungle_style === "Ganker") score += 1.8;
  if (own.jungle_style === "Invader") score += 1.2;
  if (own.jungle_style === "Farmer") score += 0.9;
  if (own.jungle_style === "Enabler") score += 1.5;

  if (own.strong_side === "Bot" && own.jungle_pathing === "TopToBot") score += 1.4;
  if (own.strong_side === "Top" && own.jungle_pathing === "BotToTop") score += 1.4;
  if (own.strong_side === "Mid") score += 0.6;

  if (own.fight_plan === "FrontToBack") score += (ownLanes.SUPPORT - 68) * 0.09;
  if (own.fight_plan === "Pick") score += (ownLanes.MID - 70) * 0.11;
  if (own.fight_plan === "Dive") score += (ownLanes.TOP - 70) * 0.1;
  if (own.fight_plan === "Siege") score += (ownLanes.ADC - 70) * 0.12;

  if (own.support_roaming === "Lane") {
    score += ((ownLanes.ADC + ownLanes.SUPPORT) / 2 - 68) * 0.08;
  } else if (own.support_roaming === "RoamMid") {
    score += (ownLanes.MID - 70) * 0.09;
    score += (ownLanes.JUNGLE - 70) * 0.05;
  } else {
    score += (ownLanes.TOP - 70) * 0.09;
    score += (ownLanes.JUNGLE - 70) * 0.05;
  }

  const coherenceScore = computeCoherenceBreakdown(own).reduce((sum, item) => sum + item.delta, 0);
  score += coherenceScore * 2.2;

  const teamTimingFit = computeTeamTimingFit({
    championIds: ownPicks.map((pick) => pick.championId),
    preference: own.game_timing,
  });
  score += teamTimingFit * 2.5;

  return clamp(score, -12, 12);
}

function playerOverall(player: MatchSnapshot["home_team"]["players"][number]): number {
  const values = ATTRIBUTE_KEYS.map((key) => Number(player[key] ?? 0));
  return average(values);
}

function teamMorale(gameState: GameStateData, players: MatchSnapshot["home_team"]["players"]): number {
  const moraleValues = players
    .map((player) => gameState.players.find((p) => p.id === player.id)?.morale)
    .filter((value): value is number => typeof value === "number");
  return moraleValues.length > 0 ? average(moraleValues) : 60;
}

function teamCondition(gameState: GameStateData, players: MatchSnapshot["home_team"]["players"]): number {
  const conditionValues = players
    .map((player) => {
      const fromState = gameState.players.find((p) => p.id === player.id)?.condition;
      return typeof fromState === "number" ? fromState : player.condition;
    })
    .filter((value): value is number => typeof value === "number");
  return conditionValues.length > 0 ? average(conditionValues) : 75;
}

function pickChampionByRole(
  picks: ChampionDraftResultPayload["blue"]["picks"],
  role: Role,
): string | null {
  return picks.find((pick) => pick.role === role)?.championId ?? null;
}

function weightedAllocation(total: number, weights: number[], rand: () => number): number[] {
  if (weights.length === 0) return [];
  const result = Array.from({ length: weights.length }, () => 0);
  const safeWeights = weights.map((w) => Math.max(0.001, w));
  const weightSum = safeWeights.reduce((sum, w) => sum + w, 0);

  for (let i = 0; i < total; i += 1) {
    let needle = rand() * weightSum;
    for (let idx = 0; idx < safeWeights.length; idx += 1) {
      needle -= safeWeights[idx];
      if (needle <= 0) {
        result[idx] += 1;
        break;
      }
    }
  }

  return result;
}

function pickMatchDuration(rand: () => number): number {
  const bucket = rand();
  if (bucket < 0.7) {
    // Mayoría de partidas: 25-40
    return 25 + Math.floor(rand() * 16);
  }
  if (bucket < 0.9) {
    // Partidas cortas: 18-24
    return 18 + Math.floor(rand() * 7);
  }
  // Partidas largas: 41-58
  return 41 + Math.floor(rand() * 18);
}

export function simulateDraftMatchResult(params: {
  snapshot: MatchSnapshot;
  gameState: GameStateData;
  draft: ChampionDraftResultPayload;
  draftEvaluations?: DraftEvaluationsBySide;
  seedSalt?: string;
}): DraftMatchResult {
  const { snapshot, gameState, draft, draftEvaluations, seedSalt = "" } = params;

  const seed = hashText(
    `${snapshot.home_team.name}|${snapshot.away_team.name}|${draft.history.join("|")}|${draftEvaluations?.blue.map((evaluation) => evaluation.total).join("|") ?? ""}|${draftEvaluations?.red.map((evaluation) => evaluation.total).join("|") ?? ""}|${seedSalt}`,
  );
  const rand = mulberry32(seed);

  const blueData = teamSideData(snapshot, gameState, "blue");
  const redData = teamSideData(snapshot, gameState, "red");

  const blueOverall = average(blueData.players.map(playerOverall));
  const redOverall = average(redData.players.map(playerOverall));

  const blueTacticsBonus = tacticsPowerBonus({
    own: blueData.tactics,
    ownPlayers: blueData.players,
    enemyPlayers: redData.players,
    ownPicks: draft.blue.picks,
  });
  const redTacticsBonus = tacticsPowerBonus({
    own: redData.tactics,
    ownPlayers: redData.players,
    enemyPlayers: blueData.players,
    ownPicks: draft.red.picks,
  });

  const blueMorale = teamMorale(gameState, blueData.players);
  const redMorale = teamMorale(gameState, redData.players);
  const blueCondition = teamCondition(gameState, blueData.players);
  const redCondition = teamCondition(gameState, redData.players);

  // Rust's total is the draft-policy contribution. Morale, condition, and tactics
  // remain independent inputs below rather than being folded into this score.
  const blueDraftStrength = average(draftEvaluations?.blue.map((evaluation) => evaluation.total) ?? [50]);
  const redDraftStrength = average(draftEvaluations?.red.map((evaluation) => evaluation.total) ?? [50]);

  const bluePower =
    blueOverall * 0.35 +
    blueDraftStrength * 0.20 +
    blueTacticsBonus +
    blueMorale * 0.15 +
    blueCondition * 0.10;
  const redPower =
    redOverall * 0.35 +
    redDraftStrength * 0.20 +
    redTacticsBonus +
    redMorale * 0.15 +
    redCondition * 0.10;

  const powerDiff = bluePower - redPower;
  const autoWinThreshold = 16;
  const autoWin = Math.abs(powerDiff) >= autoWinThreshold;
  const winProbBlue = clamp(50 + powerDiff * 3.5, 5, 95);
  const winnerSide: Side = autoWin
    ? powerDiff >= 0
      ? "blue"
      : "red"
    : rand() * 100 <= winProbBlue
      ? "blue"
      : "red";

  const strongSideBias = (side: Side): number => {
    const plan = side === "blue" ? blueData.tactics : redData.tactics;
    if (plan.strong_side === "Bot") return 0.03;
    if (plan.strong_side === "Top") return -0.03;
    return 0;
  };

  const timingBias = (side: Side, minute: number): number => {
    const plan = side === "blue" ? blueData.tactics : redData.tactics;
    if (plan.game_timing === "Early") return minute <= 14 ? 0.06 : -0.03;
    if (plan.game_timing === "Late") return minute >= 22 ? 0.06 : -0.03;
    return minute >= 12 && minute <= 24 ? 0.04 : 0;
  };

  const supportRoamBias = (side: Side, minute: number): number => {
    const plan = side === "blue" ? blueData.tactics : redData.tactics;
    if (plan.support_roaming === "Lane") return minute <= 14 ? 0.03 : 0;
    if (plan.support_roaming === "RoamMid") return minute <= 18 ? 0.04 : 0.01;
    return minute <= 14 ? 0.035 : -0.005;
  };

  const durationMinutes = pickMatchDuration(rand);
  const winnerBonus = clamp(Math.round(Math.abs(powerDiff) / 4), 0, 6);

  const blueKills = clamp(
    Math.round(9 + rand() * 8 + (winnerSide === "blue" ? 3 + winnerBonus : -1)),
    4,
    35,
  );
  const redKills = clamp(
    Math.round(9 + rand() * 8 + (winnerSide === "red" ? 3 + winnerBonus : -1)),
    4,
    35,
  );

  const blueObjectives: DraftTeamObjectives = {
    voidgrubs: 0,
    dragons: 0,
    dragonSoul: false,
    elderDragons: 0,
    heralds: 0,
    barons: 0,
    towers: 0,
    inhibitors: 0,
  };
  const redObjectives: DraftTeamObjectives = {
    voidgrubs: 0,
    dragons: 0,
    dragonSoul: false,
    elderDragons: 0,
    heralds: 0,
    barons: 0,
    towers: 0,
    inhibitors: 0,
  };

  const timelineEvents: DraftTimelineEvent[] = [];

  // First blood (temprano)
  const firstBloodMinute = clamp(2 + Math.floor(rand() * 7), 2, Math.min(9, durationMinutes - 1));
  const earlyBlueBias =
    strongSideBias("blue") - strongSideBias("red") +
    timingBias("blue", firstBloodMinute) - timingBias("red", firstBloodMinute) +
    supportRoamBias("blue", firstBloodMinute) - supportRoamBias("red", firstBloodMinute);
  const firstBloodSide: Side = rand() < 0.5 + earlyBlueBias ? "blue" : "red";
  timelineEvents.push({
    minute: firstBloodMinute,
    side: firstBloodSide,
    type: "first_blood",
    label: "First Blood",
  });

  // Voidgrubs: spawn 8:00, camp único (3 unidades), desaparece antes de 15:00
  if (durationMinutes >= 8) {
    const voidgrubMinute = clamp(8 + Math.floor(rand() * 5), 8, Math.min(14, durationMinutes - 1));
    const voidgrubBias =
      strongSideBias("blue") - strongSideBias("red") +
      timingBias("blue", voidgrubMinute) - timingBias("red", voidgrubMinute) +
      supportRoamBias("blue", voidgrubMinute) - supportRoamBias("red", voidgrubMinute);
    const voidgrubTaker: Side = rand() < 0.5 + voidgrubBias ? "blue" : "red";
    if (voidgrubTaker === "blue") blueObjectives.voidgrubs += 3;
    else redObjectives.voidgrubs += 3;

    timelineEvents.push({
      minute: voidgrubMinute,
      side: voidgrubTaker,
      type: "voidgrubs",
      label: "Voidgrubs x3",
    });
  }

  // Dragones: spawn 5:00, respawn 5:00; Soul al 4to, luego Elder cada 6:00
  let nextDragonSpawn = 5;
  let soulSide: Side | null = null;

  while (nextDragonSpawn < durationMinutes) {
    const takeMinute = clamp(nextDragonSpawn + Math.floor(rand() * 3), nextDragonSpawn, durationMinutes - 1);

    if (soulSide) {
      const elderBias = timingBias("blue", takeMinute) - timingBias("red", takeMinute);
      const elderTaker: Side = rand() < 0.5 + elderBias ? "blue" : "red";
      if (elderTaker === "blue") blueObjectives.elderDragons += 1;
      else redObjectives.elderDragons += 1;

      timelineEvents.push({
        minute: takeMinute,
        side: elderTaker,
        type: "elder",
        label: "Elder Dragon",
      });
      nextDragonSpawn = takeMinute + 6;
      continue;
    }

    const dragonBias =
      strongSideBias("blue") - strongSideBias("red") +
      timingBias("blue", takeMinute) - timingBias("red", takeMinute) +
      supportRoamBias("blue", takeMinute) - supportRoamBias("red", takeMinute);
    const dragonTaker: Side = rand() < 0.5 + dragonBias ? "blue" : "red";
    if (dragonTaker === "blue") blueObjectives.dragons += 1;
    else redObjectives.dragons += 1;

    timelineEvents.push({
      minute: takeMinute,
      side: dragonTaker,
      type: "dragon",
      label: "Dragon",
    });

    const blueHasSoul = blueObjectives.dragons >= 4;
    const redHasSoul = redObjectives.dragons >= 4;
    if (blueHasSoul || redHasSoul) {
      soulSide = blueHasSoul ? "blue" : "red";
      if (soulSide === "blue") blueObjectives.dragonSoul = true;
      else redObjectives.dragonSoul = true;

      timelineEvents.push({
        minute: takeMinute,
        side: soulSide,
        type: "dragon_soul",
        label: "Dragon Soul",
      });
      nextDragonSpawn = takeMinute + 6;
      continue;
    }

    nextDragonSpawn = takeMinute + 5;
  }

  // Herald: aparece 15:00, desaparece antes de 20:00
  if (durationMinutes >= 15) {
    const heraldMinute = clamp(15 + Math.floor(rand() * 4), 15, Math.min(19, durationMinutes - 1));
    const heraldBias =
      strongSideBias("blue") - strongSideBias("red") +
      timingBias("blue", heraldMinute) - timingBias("red", heraldMinute) +
      supportRoamBias("blue", heraldMinute) - supportRoamBias("red", heraldMinute);
    const heraldTaker: Side = rand() < 0.5 + heraldBias ? "blue" : "red";
    if (heraldTaker === "blue") blueObjectives.heralds += 1;
    else redObjectives.heralds += 1;

    timelineEvents.push({
      minute: heraldMinute,
      side: heraldTaker,
      type: "herald",
      label: "Herald",
    });
  }

  // Baron: 20:00, respawn 6:00
  let nextBaronSpawn = 20;
  while (nextBaronSpawn < durationMinutes) {
    const takeMinute = clamp(nextBaronSpawn + Math.floor(rand() * 3), nextBaronSpawn, durationMinutes - 1);
    const baronBias = timingBias("blue", takeMinute) - timingBias("red", takeMinute);
    const baronTaker: Side = rand() < 0.5 + baronBias ? "blue" : "red";
    if (baronTaker === "blue") blueObjectives.barons += 1;
    else redObjectives.barons += 1;

    timelineEvents.push({
      minute: takeMinute,
      side: baronTaker,
      type: "baron",
      label: "Baron",
    });

    nextBaronSpawn = takeMinute + 6;
  }

  // Torres/Inhibidores/Nexo: secuencia más parecida a LoL real
  const winnerTowers = clamp(
    Math.round(6 + durationMinutes * 0.16 + rand() * 1.4 + Math.abs(powerDiff) / 5),
    8,
    11,
  );
  const loserTowers = clamp(
    Math.round(2 + durationMinutes * 0.07 + rand() * 1.1 - Math.abs(powerDiff) / 9),
    0,
    8,
  );

  const adjustedWinnerTowers = Math.max(winnerTowers, loserTowers + 2);
  const adjustedLoserTowers = Math.min(loserTowers, adjustedWinnerTowers - 2);

  const winnerInhibitors = clamp(
    1 + Math.floor(Math.max(0, adjustedWinnerTowers - 8) / 1.5) + (rand() < 0.35 ? 1 : 0),
    1,
    3,
  );
  const loserInhibitors = clamp(
    Math.floor(Math.max(0, adjustedLoserTowers - 8) / 2),
    0,
    1,
  );

  if (winnerSide === "blue") {
    blueObjectives.towers = adjustedWinnerTowers;
    redObjectives.towers = adjustedLoserTowers;
    blueObjectives.inhibitors = winnerInhibitors;
    redObjectives.inhibitors = loserInhibitors;
  } else {
    redObjectives.towers = adjustedWinnerTowers;
    blueObjectives.towers = adjustedLoserTowers;
    redObjectives.inhibitors = winnerInhibitors;
    blueObjectives.inhibitors = loserInhibitors;
  }

  const pushTimedEvents = (
    side: Side,
    type: DraftTimelineEvent["type"],
    label: string,
    count: number,
    startMinute: number,
    endMinute: number,
  ) => {
    if (count <= 0) return;
    const start = clamp(startMinute, 1, Math.max(1, durationMinutes - 1));
    const end = clamp(endMinute, start, Math.max(start, durationMinutes - 1));
    for (let i = 0; i < count; i += 1) {
      const ratio = (i + 1) / (count + 1);
      const minute = Math.round(start + (end - start) * ratio + (rand() - 0.5) * 1.6);
      timelineEvents.push({
        minute: clamp(minute, start, end),
        side,
        type,
        label,
      });
    }
  };

  const winnerNonNexusTowers = Math.max(0, adjustedWinnerTowers - 2);
  const loserNonNexusTowers = adjustedLoserTowers;

  const winnerSideForEndgame: Side = winnerSide;
  const loserSideForEndgame: Side = winnerSide === "blue" ? "red" : "blue";
  const winnerInhibCount = winnerSideForEndgame === "blue" ? blueObjectives.inhibitors : redObjectives.inhibitors;
  const loserInhibCount = loserSideForEndgame === "blue" ? blueObjectives.inhibitors : redObjectives.inhibitors;

  // Distribución de estructuras para que el cierre de partida sea lógico:
  // el ganador tumba torres de línea + inhibidores + torres de nexo + nexo.
  pushTimedEvents(loserSideForEndgame, "turret", "Turret", loserNonNexusTowers, 11, Math.max(12, durationMinutes - 10));
  pushTimedEvents(winnerSideForEndgame, "turret", "Turret", winnerNonNexusTowers, 11, Math.max(12, durationMinutes - 6));

  pushTimedEvents(loserSideForEndgame, "inhibitor", "Inhibitor", loserInhibCount, Math.max(20, durationMinutes - 10), Math.max(21, durationMinutes - 7));
  pushTimedEvents(winnerSideForEndgame, "inhibitor", "Inhibitor", winnerInhibCount, Math.max(22, durationMinutes - 7), Math.max(23, durationMinutes - 3));

  timelineEvents.push({ minute: Math.max(20, durationMinutes - 2), side: winnerSideForEndgame, type: "nexus_turret", label: "Nexus Turret" });
  timelineEvents.push({ minute: Math.max(21, durationMinutes - 1), side: winnerSideForEndgame, type: "nexus_turret", label: "Nexus Turret" });
  timelineEvents.push({ minute: Math.max(22, durationMinutes), side: winnerSideForEndgame, type: "nexus", label: "Nexus" });

  timelineEvents.sort((a, b) => a.minute - b.minute);

  const blueKillWeights = [1.1, 1.15, 1.35, 1.45, 0.85].map((base) => base + rand() * 0.4);
  const redKillWeights = [1.1, 1.15, 1.35, 1.45, 0.85].map((base) => base + rand() * 0.4);
  const blueDeathWeights = [1.0, 1.0, 1.05, 1.05, 0.9].map((base) => base + rand() * 0.4);
  const redDeathWeights = [1.0, 1.0, 1.05, 1.05, 0.9].map((base) => base + rand() * 0.4);

  const blueKillsByPlayer = weightedAllocation(blueKills, blueKillWeights, rand);
  const redKillsByPlayer = weightedAllocation(redKills, redKillWeights, rand);
  const blueDeathsByPlayer = weightedAllocation(redKills, blueDeathWeights, rand);
  const redDeathsByPlayer = weightedAllocation(blueKills, redDeathWeights, rand);

  const blueAssistPool = clamp(Math.round(blueKills * (1.7 + rand() * 0.8)), blueKills, blueKills * 4);
  const redAssistPool = clamp(Math.round(redKills * (1.7 + rand() * 0.8)), redKills, redKills * 4);
  const blueAssistsByPlayer = weightedAllocation(blueAssistPool, [1, 1.1, 1, 1, 2.5], rand);
  const redAssistsByPlayer = weightedAllocation(redAssistPool, [1, 1.1, 1, 1, 2.5], rand);

  const buildSideResults = (
    side: Side,
    teamPlayers: typeof blueData.players,
    picks: ChampionDraftResultPayload["blue"]["picks"],
    killsByPlayer: number[],
    deathsByPlayer: number[],
    assistsByPlayer: number[],
    isWinner: boolean,
  ): DraftPlayerResult[] => {
    return teamPlayers.map((player, index) => {
      const role = ROLE_ORDER[index] ?? "MID";
      const kills = killsByPlayer[index] ?? 0;
      const deaths = deathsByPlayer[index] ?? 0;
      const assists = assistsByPlayer[index] ?? 0;
      const championId = pickChampionByRole(picks, role);
      const ratingRaw = 5.5 + kills * 0.45 + assists * 0.12 - deaths * 0.35 + (isWinner ? 0.8 : -0.4);
      const rating = clamp(Number(ratingRaw.toFixed(1)), 1, 10);
      const gold = Math.max(
        6000,
        Math.round(
          6800 +
            (durationMinutes - 20) * 120 +
            kills * 340 +
            assists * 120 -
            deaths * 60 +
            rand() * 1400 +
            (isWinner ? 800 : 0),
        ),
      );

      return {
        side,
        playerId: player.id,
        playerName: player.name,
        role,
        championId,
        kills,
        deaths,
        assists,
        gold,
        rating,
      };
    });
  };

  const bluePlayerResults = buildSideResults(
    "blue",
    blueData.players,
    draft.blue.picks,
    blueKillsByPlayer,
    blueDeathsByPlayer,
    blueAssistsByPlayer,
    winnerSide === "blue",
  );
  const redPlayerResults = buildSideResults(
    "red",
    redData.players,
    draft.red.picks,
    redKillsByPlayer,
    redDeathsByPlayer,
    redAssistsByPlayer,
    winnerSide === "red",
  );

  const playerResults = [...bluePlayerResults, ...redPlayerResults];
  const mvp = [...playerResults].sort((a, b) => {
    if (b.rating !== a.rating) return b.rating - a.rating;
    const aImpact = a.kills + a.assists;
    const bImpact = b.kills + b.assists;
    return bImpact - aImpact;
  })[0];

  const timelineMinutes = Array.from({ length: Math.floor(durationMinutes / 2) + 1 }, (_, idx) => idx * 2);
  if (timelineMinutes[timelineMinutes.length - 1] !== durationMinutes) {
    timelineMinutes.push(durationMinutes);
  }

  const eventGoldImpact = (event: DraftTimelineEvent): number => {
    const sign = event.side === "blue" ? 1 : -1;
    switch (event.type) {
      case "first_blood":
        return sign * 450;
      case "voidgrubs":
        return sign * 320;
      case "dragon":
        return sign * 220;
      case "dragon_soul":
        return 0;
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

  const blueSoulMinute = timelineEvents.find((event) => event.type === "dragon_soul" && event.side === "blue")?.minute;
  const redSoulMinute = timelineEvents.find((event) => event.type === "dragon_soul" && event.side === "red")?.minute;

  const blueTotalGold = bluePlayerResults.reduce((sum, player) => sum + player.gold, 0);
  const redTotalGold = redPlayerResults.reduce((sum, player) => sum + player.gold, 0);
  let finalGoldDiff = blueTotalGold - redTotalGold;
  if (finalGoldDiff === 0) {
    finalGoldDiff = winnerSide === "blue" ? 900 : -900;
  }

  const eventContributionByMinute = timelineMinutes.map((minute) => {
    return timelineEvents
      .filter((event) => event.minute <= minute)
      .reduce((sum, event) => sum + eventGoldImpact(event), 0);
  });

  const rawDiffTimeline = timelineMinutes.map((minute, idx) => {
    if (idx === 0) return 0;
    const progress = clamp(minute / Math.max(1, durationMinutes), 0, 1);
    const baseCurve = finalGoldDiff * Math.pow(progress, 1.12);
    const eventContribution = eventContributionByMinute[idx] * 0.42;

    let soulMomentum = 0;
    if (typeof blueSoulMinute === "number" && minute > blueSoulMinute) {
      soulMomentum += (minute - blueSoulMinute) * 32;
    }
    if (typeof redSoulMinute === "number" && minute > redSoulMinute) {
      soulMomentum -= (minute - redSoulMinute) * 32;
    }

    const volatility = (1 - progress) * (220 + Math.abs(powerDiff) * 10);
    const jitter = Math.round((rand() - 0.5) * volatility);

    return baseCurve + eventContribution + soulMomentum + jitter;
  });

  const rawFirst = rawDiffTimeline[0] ?? 0;
  const rawLast = rawDiffTimeline[rawDiffTimeline.length - 1] ?? 0;
  const denominator = rawLast - rawFirst;

  const goldDiffTimeline = timelineMinutes.map((minute, idx) => {
    const rawValue = rawDiffTimeline[idx] ?? 0;
    const normalized =
      Math.abs(denominator) < 1
        ? (finalGoldDiff * minute) / Math.max(1, durationMinutes)
        : ((rawValue - rawFirst) / denominator) * finalGoldDiff;

    return {
      minute,
      diff: clamp(Math.round(normalized), -22000, 22000),
    };
  });

  return {
    winnerSide,
    durationMinutes,
    blueKills,
    redKills,
    mvp,
    playerResults,
    goldDiffTimeline,
    timelineEvents,
    objectives: {
      blue: blueObjectives,
      red: redObjectives,
    },
    power: {
      blue: Number(bluePower.toFixed(2)),
      red: Number(redPower.toFixed(2)),
      diff: Number(powerDiff.toFixed(2)),
      autoWin,
      winProbBlue,
    },
  };
}


