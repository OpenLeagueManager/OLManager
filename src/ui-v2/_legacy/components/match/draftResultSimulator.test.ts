import { describe, expect, it } from "vitest";
import {
  simulateDraftMatchResult,
  type DraftEvaluationsBySide,
} from "@/ui-v2/_legacy/components/match/draftResultSimulator";
import type { ChampionDraftResultPayload } from "@/ui-v2/_legacy/components/match/ChampionDraft";
import type { MatchSnapshot } from "@/ui-v2/_legacy/components/match/types";
import type { GameStateData } from "@/store/gameStore";

const roles = ["TOP", "JUNGLE", "MID", "ADC", "SUPPORT"] as const;

function players(side: string) {
  return roles.map((role) => ({
    id: `${side}-${role}`,
    name: `${side} ${role}`,
    role,
    condition: 75,
    pace: 70,
    stamina: 70,
    strength: 70,
    agility: 70,
    passing: 70,
    shooting: 70,
    tackling: 70,
    dribbling: 70,
    defending: 70,
    positioning: 70,
    vision: 70,
    decisions: 70,
    composure: 70,
    aggression: 70,
    teamwork: 70,
    leadership: 70,
  }));
}

const snapshot = {
  home_team: { id: "blue", name: "Blue", players: players("blue") },
  away_team: { id: "red", name: "Red", players: players("red") },
} as unknown as MatchSnapshot;

const draft: ChampionDraftResultPayload = {
  blue: {
    picks: roles.map((role) => ({ role, championId: `blue-${role}` })),
    bans: [],
    score: { mastery: 50, synergy: 50, counter: 50, comfort: 50, preparation: 50, total: 250 },
  },
  red: {
    picks: roles.map((role) => ({ role, championId: `red-${role}` })),
    bans: [],
    score: { mastery: -50, synergy: -50, counter: -50, comfort: -50, preparation: -50, total: -250 },
  },
  history: ["draft"],
};

const gameState = { players: [], teams: [] } as unknown as GameStateData;

function evaluations(blueTotal: number, redTotal: number): DraftEvaluationsBySide {
  const create = (total: number, side: string) => roles.map((role) => ({
    champion_id: `${side}-${role}`,
    meta_power: total,
    mastery: total,
    skill_fit: total,
    execution_risk: 0,
    total,
    engine_modifier: 0.035,
  }));
  return { blue: create(blueTotal, "blue"), red: create(redTotal, "red") };
}

describe("simulateDraftMatchResult", () => {
  it("reacts to Rust pick totals instead of legacy draft score totals", () => {
    const result = simulateDraftMatchResult({
      snapshot,
      gameState,
      draft,
      draftEvaluations: evaluations(30, 90),
      seedSalt: "pick-evaluations",
    });

    expect(result.power.red).toBeGreaterThan(result.power.blue);
    expect(result.winnerSide).toBe("red");
  });

  it("remains deterministic for the same seed and evaluations", () => {
    const params = {
      snapshot,
      gameState,
      draft,
      draftEvaluations: evaluations(72, 68),
      seedSalt: "deterministic",
    };

    expect(simulateDraftMatchResult(params)).toEqual(simulateDraftMatchResult(params));
  });
});
