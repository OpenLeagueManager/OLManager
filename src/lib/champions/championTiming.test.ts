import { describe, expect, it } from "vitest";
import championTimings from "../../../data/champions/champion-timings.v1.json";
import championCatalog from "../../../assets/simulation/champions.json";
import { computeTeamTimingFit, getChampionTiming } from "./championTiming";

describe("champion timings", () => {
  it("uses the versioned dataset schema from the data submodule", () => {
    expect(championTimings).toMatchObject({
      schemaVersion: 2,
      dataset: "champion-timings",
      defaultTiming: "Unknown",
      data: expect.any(Object),
    });
    expect(
      Object.values(championTimings.data).every(
        (entry) => typeof entry.timing === "string" && ["Early", "Mid", "Late", "Unknown"].includes(entry.timing),
      ),
    ).toBe(true);
    expect(Object.keys(championTimings.data)).toHaveLength(172);
    expect(championTimings.data.Heimerdinger.roles).toEqual(["Mid", "Support"]);
    expect(championTimings.data.Quinn.roles).toEqual(["Top"]);
  });

  it("keeps visual catalog roles and derived, non-official disclosures intact", () => {
    for (const [championId, roles] of Object.entries(championCatalog.data.roles)) {
      expect(championTimings.data[championId as keyof typeof championTimings.data].roles).toEqual(roles);
    }

    expect(championTimings.methodology.classification).toMatch(/derived/i);
    expect(championTimings.methodology.classification).toMatch(/(?:not|never) an official/i);
    for (const entry of Object.values(championTimings.data)) {
      expect(entry.evidence).toMatch(/derived/i);
      expect(entry.evidence).toMatch(/not an official/i);
    }
  });

  it("falls back to Unknown when a champion has no timing entry", () => {
    expect(getChampionTiming(null)).toBe("Unknown");
    expect(getChampionTiming("MissingChampion")).toBe("Unknown");
    expect(computeTeamTimingFit({ championIds: ["MissingChampion"], preference: "Early" })).toBe(0);
  });
});
