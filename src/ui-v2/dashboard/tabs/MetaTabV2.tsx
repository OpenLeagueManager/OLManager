import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Search } from "lucide-react";

import type { GameStateData } from "@/store/gameStore";
import {
  formatStaffEffectPercent,
  getLolStaffEffectsForTeam,
} from "@/lib/teams/lolStaffEffects";
import { normalizeChampionKey } from "@/lib/champions/championIds";
import { resolveChampionTile } from "@/lib/champions/championImages";
import { ROLE_ICON_PATHS } from "@/lib/players/roleIcons";
import { delegateChampionTraining } from "@/services/playerService";

import { Card, CardContent, CardHeader, CardTitle } from "@/ui-v2/components/ui/card";
import { cn } from "@/ui-v2/lib/utils";

interface MetaTabV2Props {
  gameState: GameStateData;
  onViewChampion: (championKey: string) => void;
  onGameUpdate?: (state: GameStateData) => void;
}

type UiRole = "Top" | "Jungle" | "Mid" | "ADC" | "Support";

const ROLE_ORDER: Record<UiRole, number> = { Top: 1, Jungle: 2, Mid: 3, ADC: 4, Support: 5 };

const ROLE_ICON_URLS: Record<UiRole, string> = {
  Top: ROLE_ICON_PATHS.TOP,
  Jungle: ROLE_ICON_PATHS.JUNGLE,
  Mid: ROLE_ICON_PATHS.MID,
  ADC: ROLE_ICON_PATHS.ADC,
  Support: ROLE_ICON_PATHS.SUPPORT,
};

function normalizeKey(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]/g, "");
}

function normalizeRole(role: string): UiRole {
  const key = normalizeKey(role);
  if (key === "top") return "Top";
  if (key === "jungle") return "Jungle";
  if (key === "mid") return "Mid";
  if (key === "bot" || key === "adc") return "ADC";
  return "Support";
}

function championDisplayName(championId: string): string {
  if (normalizeChampionKey(championId) === "MonkeyKing") return "Wukong";
  return championId;
}

const TIER_ORDER: Array<"S" | "A" | "B" | "C" | "D"> = ["S", "A", "B", "C", "D"];
const TIER_SORT_WEIGHT: Record<string, number> = { S: 0, A: 1, B: 2, C: 3, D: 4 };

const TIER_BADGE_CLASS: Record<string, string> = {
  S: "bg-emerald-600 text-white",
  A: "bg-lime-500 text-white",
  B: "bg-amber-500 text-white",
  C: "bg-orange-500 text-white",
  D: "bg-red-500 text-white",
};

export function MetaTabV2({ gameState, onViewChampion, onGameUpdate }: MetaTabV2Props) {
  const { t } = useTranslation();
  const [metaRoleFilter, setMetaRoleFilter] = useState<"ALL" | UiRole>("ALL");
  const [delegating, setDelegating] = useState(false);

  const managerTeamId = gameState.manager.team_id;
  const patch = gameState.champion_patch;
  const staffEffects = getLolStaffEffectsForTeam(gameState, managerTeamId);

  const ownPlayers = useMemo(
    () =>
      gameState.players
        .filter((player) => player.team_id === managerTeamId)
        .sort((a, b) => a.match_name.localeCompare(b.match_name)),
    [gameState.players, managerTeamId],
  );

  async function handleDelegateTraining() {
    setDelegating(true);
    try {
      const updated = await delegateChampionTraining();
      onGameUpdate?.(updated);
    } finally {
      setDelegating(false);
    }
  }

  const discoveredSet = useMemo(
    () => new Set((patch?.discovered_champion_ids ?? []).map(normalizeKey)),
    [patch?.discovered_champion_ids],
  );

  const discoveredMeta = useMemo(() => {
    const all = (patch?.hidden_meta ?? []).filter((entry) =>
      discoveredSet.has(normalizeKey(entry.champion_id)),
    );
    if (metaRoleFilter !== "ALL") {
      return all.filter((entry) => normalizeRole(entry.role) === metaRoleFilter);
    }

    const dedupBestByChampion = new Map<string, (typeof all)[number]>();
    all.forEach((entry) => {
      const key = normalizeKey(entry.champion_id);
      const previous = dedupBestByChampion.get(key);
      if (!previous) {
        dedupBestByChampion.set(key, entry);
        return;
      }
      const previousWeight = TIER_SORT_WEIGHT[(previous.tier || "C").toUpperCase()] ?? 99;
      const currentWeight = TIER_SORT_WEIGHT[(entry.tier || "C").toUpperCase()] ?? 99;
      if (currentWeight < previousWeight) dedupBestByChampion.set(key, entry);
    });

    return [...dedupBestByChampion.values()];
  }, [patch?.hidden_meta, discoveredSet, metaRoleFilter]);

  const tierRows = useMemo(() => {
    const rows: Record<string, typeof discoveredMeta> = { S: [], A: [], B: [], C: [], D: [] };
    discoveredMeta.forEach((entry) => {
      const tier = (entry.tier || "C").toUpperCase();
      if (rows[tier]) rows[tier].push(entry);
    });
    TIER_ORDER.forEach((tier) => {
      rows[tier].sort((a, b) => a.champion_id.localeCompare(b.champion_id));
    });
    return rows;
  }, [discoveredMeta]);

  const discoveredPct = useMemo(() => {
    const totalChampionKeys = new Set((gameState.champions ?? []).map((champion) => normalizeKey(champion.champion_key)));
    if (totalChampionKeys.size === 0) return 0;
    const discoveredCount = [...discoveredSet].filter((key) => totalChampionKeys.has(key)).length;
    return Math.round((discoveredCount / totalChampionKeys.size) * 100);
  }, [discoveredSet, gameState.champions]);

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6 scrollbar-v2">
        <Card>
          <CardHeader className="flex-row items-center justify-between space-y-0">
            <CardTitle className="font-heading text-sm uppercase tracking-widest text-muted-foreground">
              {t("meta.patchMeta")}
            </CardTitle>
            <div className="flex items-center gap-3 text-xs text-muted-foreground">
              <span>{formatStaffEffectPercent(staffEffects.metaDiscovery)} discovery</span>
              <span className="text-border">·</span>
              <span>{formatStaffEffectPercent(staffEffects.development)} mastery</span>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <p className="text-xs uppercase tracking-widest text-muted-foreground">
                  {patch?.current_patch_label || t("meta.unknownPatch")}
                </p>
                {patch?.last_patch_date && (
                  <p className="mt-0.5 text-xs text-muted-foreground">
                    {t("meta.updated")} {patch.last_patch_date}
                  </p>
                )}
              </div>
              <div className="flex items-center gap-2">
                <Search className="size-3.5 text-muted-foreground" />
                <span className="font-heading text-sm tabular-nums text-primary">{discoveredPct}%</span>
                <div className="h-1.5 w-24 overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-primary transition-all duration-700"
                    style={{ width: `${discoveredPct}%` }}
                  />
                </div>
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-1.5">
              <button
                type="button"
                onClick={() => setMetaRoleFilter("ALL")}
                className={cn(
                  "rounded-md border px-2.5 py-1 text-xs font-heading uppercase tracking-wider transition-colors",
                  metaRoleFilter === "ALL"
                    ? "border-primary bg-primary text-primary-foreground"
                    : "border-border text-muted-foreground hover:border-muted-foreground/50",
                )}
              >
                {t("common.all")}
              </button>
              {(Object.keys(ROLE_ORDER) as UiRole[]).map((role) => (
                <button
                  key={role}
                  type="button"
                  onClick={() => setMetaRoleFilter(role)}
                  className={cn(
                    "flex size-7 items-center justify-center rounded-md border transition-colors",
                    metaRoleFilter === role
                      ? "border-primary bg-primary/10"
                      : "border-border hover:border-muted-foreground/50",
                  )}
                  title={role}
                >
                  <img src={ROLE_ICON_URLS[role]} alt={role} className="size-3.5 object-contain" />
                </button>
              ))}
            </div>

            <div className="space-y-1">
              {TIER_ORDER.map((tier) => (
                <div key={tier} className="grid grid-cols-[40px_1fr] overflow-hidden rounded-lg border border-border">
                  <div
                    className={cn(
                      "flex items-center justify-center text-base font-heading font-bold",
                      TIER_BADGE_CLASS[tier],
                    )}
                  >
                    {tier}
                  </div>
                  <div className="min-h-[60px] bg-muted/30 p-2">
                    {tierRows[tier].length === 0 ? (
                      <p className="text-xs italic text-muted-foreground/50">—</p>
                    ) : (
                      <div className="flex flex-wrap gap-1.5">
                        {tierRows[tier].map((entry) => (
                          <button
                            type="button"
                            key={`${tier}-${entry.champion_id}-${entry.role}`}
                            onClick={() => onViewChampion(entry.champion_id)}
                            className="group relative cursor-pointer"
                          >
                            <div className="size-[52px] overflow-hidden rounded-md border border-border transition-all duration-300 group-hover:-translate-y-0.5 group-hover:border-primary">
                              <img
                                src={resolveChampionTile(entry.champion_id) ?? ""}
                                alt={championDisplayName(entry.champion_id)}
                                className="size-full object-cover"
                                loading="lazy"
                                title={`${championDisplayName(entry.champion_id)} · ${entry.role}`}
                              />
                            </div>
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>

        <Card className="flex min-h-0 flex-1 flex-col">
          <CardHeader className="flex-row items-center justify-between space-y-0 shrink-0">
            <CardTitle className="font-heading text-sm uppercase tracking-widest text-muted-foreground">
              {t("meta.discoveryStats")}
            </CardTitle>
            <span className="font-heading text-xs tabular-nums text-primary">{discoveredPct}% {t("meta.complete")}</span>
          </CardHeader>
          <CardContent className="flex min-h-0 flex-1 flex-col">
            <div className="flex flex-1 flex-col justify-between gap-2">
              {TIER_ORDER.map((tier) => {
                const discovered = tierRows[tier].length;
                return (
                  <div key={tier}>
                    <div className="mb-1 flex items-center justify-between">
                      <div className="flex items-center gap-2">
                        <span className={cn("inline-flex size-5 items-center justify-center rounded text-[10px] font-heading font-bold", TIER_BADGE_CLASS[tier])}>{tier}</span>
                        <span className="text-xs text-muted-foreground">
                          {discovered} discovered
                        </span>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </CardContent>
        </Card>

        <Card className="flex min-h-0 flex-1 flex-col">
          <CardHeader className="flex-row items-center justify-between space-y-0 shrink-0">
            <CardTitle className="font-heading text-sm uppercase tracking-widest text-muted-foreground">
              {t("champions.masteryTrainingTitle")}
            </CardTitle>
            <button
              type="button"
              onClick={handleDelegateTraining}
              disabled={delegating}
              className="rounded-md border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs font-heading uppercase tracking-wider text-primary transition-all hover:bg-primary/20 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {delegating ? t("champions.delegating") : t("champions.delegateToCoach")}
            </button>
          </CardHeader>
          <CardContent className="flex-1 overflow-y-auto scrollbar-v2">
            {ownPlayers.length === 0 ? (
              <p className="py-8 text-center text-sm text-muted-foreground">
                {t("meta.noPlayers", { defaultValue: "No players on your team." })}
              </p>
            ) : (
              <div className="space-y-2">
                {ownPlayers.map((player) => {
                  const legacyTarget = player.champion_training_target
                    ? [player.champion_training_target]
                    : [];
                  const targets = Array.from(
                    new Set([...legacyTarget, ...(player.champion_training_targets ?? [])]),
                  ).filter((value): value is string => typeof value === "string" && value.length > 0);

                  return (
                    <div
                      key={player.id}
                      className="flex items-center justify-between gap-3 rounded-lg border border-border bg-muted/20 px-3 py-2"
                    >
                      <span className="truncate text-sm font-semibold text-foreground">
                        {player.match_name}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        {targets.length === 0
                          ? t("champions.noTarget")
                          : targets.map(championDisplayName).join(", ")}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
          </CardContent>
        </Card>
    </div>
  );
}
