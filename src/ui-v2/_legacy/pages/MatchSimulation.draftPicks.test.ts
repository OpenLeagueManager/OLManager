import { describe, expect, it } from "vitest";
import { draftPicksByPlayerRole } from "@/ui-v2/_legacy/pages/MatchSimulation";

describe("draftPicksByPlayerRole", () => {
  it("credits champions to player ids by role instead of snapshot array order", () => {
    const picks = draftPicksByPlayerRole(
      [
        { id: "support", role: "SUPPORT" },
        { id: "top", role: "TOP" },
        { id: "adc", role: "ADC" },
        { id: "mid", role: "MID" },
        { id: "jungle", role: "JUNGLE" },
      ],
      [
        { role: "TOP", championId: "Gnar" },
        { role: "JUNGLE", championId: "Vi" },
        { role: "MID", championId: "Azir" },
        { role: "ADC", championId: "Jinx" },
        { role: "SUPPORT", championId: "Rell" },
      ],
    );

    expect(picks).toEqual([
      { playerId: "top", championId: "Gnar" },
      { playerId: "jungle", championId: "Vi" },
      { playerId: "mid", championId: "Azir" },
      { playerId: "adc", championId: "Jinx" },
      { playerId: "support", championId: "Rell" },
    ]);
  });
});
