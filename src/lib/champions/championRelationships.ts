import championRelationships from "../../../data/champions/champion-relationships.v1.json";

interface ChampionRelationshipsDataset {
  schemaVersion: 1;
  dataset: "champion-relationships";
  counters: Array<{ source: string; target: string; value: number }>;
}

function canonicalChampionId(value: string): string {
  const normalized = value.toLowerCase().replace(/[^a-z0-9]/g, "");
  return normalized === "wukong" || normalized === "monkeyking" ? "monkeyking" : normalized;
}

const COUNTER_VALUES = new Map(
  (championRelationships as ChampionRelationshipsDataset).counters.map((relationship) => [
    `${relationship.source}::${relationship.target}`,
    relationship.value,
  ]),
);

export function getChampionCounterValue(allyChampionId: string, enemyChampionId: string): number {
  return COUNTER_VALUES.get(`${canonicalChampionId(allyChampionId)}::${canonicalChampionId(enemyChampionId)}`) ?? 0;
}
