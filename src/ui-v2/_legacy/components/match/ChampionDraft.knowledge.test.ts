import { describe, expect, it } from "vitest";
import {
  computeBanRecommendationScore,
  calculateScrimDraftSignal,
  calculateStaffRevealBudget,
  selectRivalMasteryKnowledgeForPlayer,
  selectStaffRevealEntries,
  isCurrentDraftEvaluationRequest,
  canUserConfirmDraftChoice,
} from "@/ui-v2/_legacy/components/match/ChampionDraft";
import type { ScrimReportData } from "@/store/gameStore";

function champion(id: string, name: string) {
  return {
    id,
    key: 1,
    name,
    image: `/${id}.png`,
    tags: [],
    roleHints: [],
  };
}

function scrimReport(overrides: Partial<ScrimReportData>): ScrimReportData {
  return {
    date: "2026-04-28",
    week_key: "2026-W18",
    slot_index: 0,
    weekday: 2,
    team_id: "team-a",
    opponent_team_id: "team-b",
    status: "Played",
    won: true,
    focus: "DraftPrep",
    issue: null,
    severity: 0,
    quality: 72,
    player_champion_picks: [],
    post_decision: null,
    created_on: "2026-04-28T10:00:00Z",
    ...overrides,
  };
}

describe("ChampionDraft rival mastery knowledge", () => {
  it("rejects an evaluation response that belongs to a previous draft turn", () => {
    expect(isCurrentDraftEvaluationRequest(1, 2, "first-turn", "second-turn")).toBe(false);
    expect(isCurrentDraftEvaluationRequest(2, 2, "current-turn", "current-turn")).toBe(true);
  });

  it("allows a user to confirm a selected pick without a recommendation", () => {
    expect(canUserConfirmDraftChoice("off-meta-choice", true)).toBe(true);
  });

  it("caps staff reveal budget from 1 to 5 picks based only on meta discovery", () => {
    expect(calculateStaffRevealBudget(0.9)).toBe(1);
    expect(calculateStaffRevealBudget(0.975)).toBe(2);
    expect(calculateStaffRevealBudget(1.05)).toBe(3);
    expect(calculateStaffRevealBudget(1.125)).toBe(4);
    expect(calculateStaffRevealBudget(1.2)).toBe(5);
    expect(calculateStaffRevealBudget(2)).toBe(5);
  });

  it("does not backfill staff reveals when a revealed champion is banned or picked", () => {
    const reveals = selectStaffRevealEntries(
      [
        {
          champion: champion("kaisa", "Kai'Sa"),
          mastery: 95,
          playerName: "Noah",
          playerRole: "ADC",
          source: "staff",
        },
        {
          champion: champion("xayah", "Xayah"),
          mastery: 92,
          playerName: "Noah",
          playerRole: "ADC",
          source: "staff",
        },
        {
          champion: champion("zeri", "Zeri"),
          mastery: 90,
          playerName: "Noah",
          playerRole: "ADC",
          source: "staff",
        },
      ],
      2,
      new Set(["kaisa"]),
    );

    expect(reveals.map((entry) => entry.champion.id)).toEqual(["xayah"]);
  });

  it("does not promote another champion to insignia when the true signature is banned", () => {
    const result = selectRivalMasteryKnowledgeForPlayer(
      [
        {
          champion: champion("ezreal", "Ezreal"),
          mastery: 100,
          playerName: "Noah",
          playerRole: "ADC",
        },
        {
          champion: champion("kaisa", "Kai'Sa"),
          mastery: 92,
          playerName: "Noah",
          playerRole: "ADC",
        },
      ],
      new Set(["ezreal"]),
      new Set(),
      false,
    );

    expect(result.knownEntries).toEqual([]);
    expect(result.staffCandidates).toHaveLength(1);
    expect(result.staffCandidates[0]).toMatchObject({
      champion: expect.objectContaining({ id: "kaisa" }),
      source: "staff",
    });
  });

  it("marks non-signature revealed champions as scouting, not insignia", () => {
    const result = selectRivalMasteryKnowledgeForPlayer(
      [
        {
          champion: champion("ezreal", "Ezreal"),
          mastery: 100,
          playerName: "Noah",
          playerRole: "ADC",
        },
        {
          champion: champion("kaisa", "Kai'Sa"),
          mastery: 92,
          playerName: "Noah",
          playerRole: "ADC",
        },
      ],
      new Set(["ezreal"]),
      new Set(),
      true,
    );

    expect(result.knownEntries).toHaveLength(1);
    expect(result.knownEntries[0]).toMatchObject({
      champion: expect.objectContaining({ id: "kaisa" }),
      source: "scouting",
    });
  });

  it("turns recent scrim reports into comfort, preparation, and synergy draft signal", () => {
    const signal = calculateScrimDraftSignal(
      [
        scrimReport({
          player_champion_picks: [
            { player_id: "p1", champion_id: "Azir", role: "Mid" },
            { player_id: "p2", champion_id: "Sejuani", role: "Jungle" },
            { player_id: "p3", champion_id: "KaiSa", role: "ADC" },
          ],
          post_decision: "VodReview",
        }),
      ],
      "team-a",
      "team-b",
      [
        { playerId: "p1", championId: "azir" },
        { playerId: "p2", championId: "sejuani" },
      ],
    );

    expect(signal.comfort).toBe(2);
    expect(signal.preparation).toBe(2);
    expect(signal.synergy).toBe(1);
    expect(signal.reasons).toEqual([
      "recent champion reps",
      "scrimmed core together",
      "recent prep vs this opponent",
    ]);
  });

  it("prioritizes meta tier over mastery in ban recommendation scoring", () => {
    const highMetaLowMastery = computeBanRecommendationScore({
      enemyMastery: 62,
      metaScore: 20,
      tier: "S",
      roleHints: ["MID"],
      roleAlreadyCovered: false,
      enemyJungleLocked: false,
      isFlexThreat: false,
      isSpecialThreat: false,
      draftHashSeed: "seed-a",
    });

    const lowMetaHighMastery = computeBanRecommendationScore({
      enemyMastery: 95,
      metaScore: 7,
      tier: "D",
      roleHints: ["MID"],
      roleAlreadyCovered: false,
      enemyJungleLocked: false,
      isFlexThreat: false,
      isSpecialThreat: false,
      draftHashSeed: "seed-b",
    });

    expect(highMetaLowMastery).toBeGreaterThan(lowMetaHighMastery);
  });

  it("applies signature exception for Tier D when mastery is extreme", () => {
    const tierDNormal = computeBanRecommendationScore({
      enemyMastery: 90,
      metaScore: 7,
      tier: "D",
      roleHints: ["TOP"],
      roleAlreadyCovered: false,
      enemyJungleLocked: false,
      isFlexThreat: false,
      isSpecialThreat: false,
      draftHashSeed: "seed-c",
    });

    const tierDSignature = computeBanRecommendationScore({
      enemyMastery: 96,
      metaScore: 7,
      tier: "D",
      roleHints: ["TOP"],
      roleAlreadyCovered: false,
      enemyJungleLocked: false,
      isFlexThreat: false,
      isSpecialThreat: false,
      draftHashSeed: "seed-d",
    });

    expect(tierDSignature).toBeGreaterThan(tierDNormal);
  });

  it("deprioritizes jungle bans when enemy jungle is already locked and no flex threat", () => {
    const forcedJungleBan = computeBanRecommendationScore({
      enemyMastery: 85,
      metaScore: 16,
      tier: "A",
      roleHints: ["JUNGLE"],
      roleAlreadyCovered: false,
      enemyJungleLocked: true,
      isFlexThreat: false,
      isSpecialThreat: false,
      draftHashSeed: "seed-e",
    });

    const flexJungleThreat = computeBanRecommendationScore({
      enemyMastery: 85,
      metaScore: 16,
      tier: "A",
      roleHints: ["JUNGLE", "MID"],
      roleAlreadyCovered: false,
      enemyJungleLocked: true,
      isFlexThreat: true,
      isSpecialThreat: false,
      draftHashSeed: "seed-f",
    });

    expect(flexJungleThreat).toBeGreaterThan(forcedJungleBan);
  });
});
