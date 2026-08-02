import { describe, expect, it } from "vitest";
import championRelationships from "../../../data/champions/champion-relationships.v1.json";
import { getChampionCounterValue } from "./championRelationships";

describe("champion relationships", () => {
  it("uses the versioned relationship dataset from the data submodule", () => {
    expect(championRelationships).toMatchObject({
      schemaVersion: 1,
      dataset: "champion-relationships",
      provenance: { status: "legacy_baseline" },
    });
    expect(championRelationships.roles).toHaveLength(170);
    expect(championRelationships.counters).toHaveLength(2464);
    expect(championRelationships.synergies).toHaveLength(670);
  });

  it("keeps the legacy baseline foundational, with directed counters and canonical undirected synergies", () => {
    expect(getChampionCounterValue("Aatrox", "Cho'Gath")).toBe(1);
    expect(getChampionCounterValue("Cho'Gath", "Aatrox")).not.toBe(1);
    expect(championRelationships.counters.every((relationship) => relationship.directionality === "directed" && relationship.stability === "foundational" && relationship.patch_validity === null)).toBe(true);
    expect(championRelationships.synergies.every((relationship) => relationship.directionality === "undirected" && relationship.source < relationship.target && relationship.stability === "foundational")).toBe(true);
  });

  it("records all 22 legacy synergy reductions", () => {
    expect(championRelationships.migration.synergy_duplicate_reduction).toMatchObject({
      strategy: "maximum_value",
      duplicate_pairs_reduced: 22,
      duplicate_entries_removed: 22,
    });
    expect(championRelationships.migration.synergy_duplicate_reduction.audit).toHaveLength(22);
  });
});
