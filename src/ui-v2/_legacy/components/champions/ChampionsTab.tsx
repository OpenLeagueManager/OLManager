import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Sparkles, Clock3, Search } from "lucide-react";
import type { GameStateData } from "@/store/gameStore";
import championsSeed from "../../../../../assets/simulation/champions.json";
import { setPlayerChampionTrainingTarget, delegateChampionTraining } from "@/services/playerService";
import { calculateLolOvr } from "@/lib/players/lolPlayerStats";
import { formatStaffEffectPercent, getLolStaffEffectsForTeam } from "@/lib/teams/lolStaffEffects";
import { resolvePlayerPhoto } from "@/lib/players/playerPhotos";
import { ROLE_ICON_PATHS } from "@/lib/players/roleIcons";
import { t } from "i18next";
import { resolvePlayerCurrentLolRole } from "@/lib/players/lolIdentity";
import { normalizeChampionKey } from "@/lib/champions/championIds";
import { resolveChampionTile } from "@/lib/champions/championImages";

interface ChampionsTabProps {
  gameState: GameStateData;
  onGameUpdate: (state: GameStateData) => void;
  onViewChampion: (championKey: string) => void;
}

type ChampionRolesMap = Record<string, string[]>;
type UiRole = "Top" | "Jungle" | "Mid" | "ADC" | "Support";

const CHAMPION_ROLES: ChampionRolesMap =
  ((championsSeed as { data?: { roles?: ChampionRolesMap } }).data?.roles ?? {}) as ChampionRolesMap;

const ROLE_ORDER: Record<UiRole, number> = {
  Top: 1,
  Jungle: 2,
  Mid: 3,
  ADC: 4,
  Support: 5,
};

/**
 * Maps UiRole to local icon paths (using ROLE_ICON_PATHS which expects uppercase keys)
 */
const ROLE_ICON_URLS: Record<UiRole, string> = {
  Top: ROLE_ICON_PATHS.TOP,
  Jungle: ROLE_ICON_PATHS.JUNGLE,
  Mid: ROLE_ICON_PATHS.MID,
  ADC: ROLE_ICON_PATHS.ADC,
  Support: ROLE_ICON_PATHS.SUPPORT,
};

const CHAMPIONS_BY_ROLE = Object.entries(CHAMPION_ROLES).reduce<Record<UiRole, string[]>>(
  (acc, [champion, roles]) => {
    roles.forEach((role) => {
      const mapped = role === "Bot" ? "ADC" : role;
      if (!["Top", "Jungle", "Mid", "ADC", "Support"].includes(mapped)) return;
      const key = mapped as UiRole;
      if (!acc[key]) acc[key] = [];
      acc[key].push(champion);
    });
    return acc;
  },
  { Top: [], Jungle: [], Mid: [], ADC: [], Support: [] },
);

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

function toUiRole(role: ReturnType<typeof resolvePlayerCurrentLolRole>): UiRole {
  if (role === "TOP") return "Top";
  if (role === "JUNGLE") return "Jungle";
  if (role === "MID") return "Mid";
  if (role === "ADC") return "ADC";
  return "Support";
}

function championDisplayName(championId: string): string {
  if (normalizeChampionKey(championId) === "MonkeyKing") return "Wukong";
  return championId;
}

type SoloQTier = "Challenger" | "Grandmaster" | "Master";

const SOLOQ_POINTS_BASELINE = 3000;
const SOLOQ_POINTS_MIN = 3000;
const SOLOQ_POINTS_MAX = 7000;
const SOLOQ_GRANDMASTER_LP_CUTOFF = 800;
const SOLOQ_CHALLENGER_LP_CUTOFF = 1300;
const SCHEDULE_TRAINING_DAYS: Record<string, number[]> = {
  Intense: [0, 1, 2, 3, 4, 5],
  Balanced: [0, 1, 3, 4],
  Light: [1, 3],
};

function hashText(value: string): number {
  let hash = 0;
  for (let i = 0; i < value.length; i += 1) {
    hash = (hash * 31 + value.charCodeAt(i)) >>> 0;
  }
  return hash;
}

function daysBetween(startIso: string, endIso: string): number {
  const start = new Date(startIso).getTime();
  const end = new Date(endIso).getTime();
  if (!Number.isFinite(start) || !Number.isFinite(end)) return 0;
  return Math.max(0, Math.floor((end - start) / (24 * 60 * 60 * 1000)));
}

function computeSoloQ(
  player: GameStateData["players"][number],
  gameState: GameStateData,
  masterySignal: number,
  focus: string | null | undefined,
  intensity: string,
  schedule: string,
): {
  tier: SoloQTier;
  lp: number;
  delta: number;
} {
  const ovr = calculateLolOvr(player);
  const dayIndex = daysBetween(gameState.clock.start_date, gameState.clock.current_date);
  const baseline = 3520 + (ovr - 76) * 52 + ((hashText(player.id) % 121) - 60);

  let points = baseline;
  const focusMult = getFocusMultiplier(focus);
  const intensityMult = intensityMultiplier(intensity);
  for (let day = 1; day <= dayIndex; day += 1) {
    const currentIso = addDays(gameState.clock.start_date, day);
    if (!isSoloQDay(currentIso, schedule)) continue;
    const baseGain = 10 + ((ovr - 75) * 0.8) + (masterySignal * 0.08);
    const gain = Math.round(baseGain * intensityMult * focusMult);
    points += Math.max(-20, Math.min(30, gain));
    points = Math.max(SOLOQ_POINTS_MIN, Math.min(SOLOQ_POINTS_MAX, points));
  }

  const lp = Math.max(0, Math.round(points - SOLOQ_POINTS_BASELINE));

  let yesterdayDelta = 0;
  if (dayIndex > 0) {
    const yesterdayIso = addDays(gameState.clock.start_date, dayIndex);
    if (isSoloQDay(yesterdayIso, schedule)) {
      const baseGain = 10 + ((ovr - 75) * 0.8) + (masterySignal * 0.08);
      yesterdayDelta = Math.max(-20, Math.min(30, Math.round(baseGain * intensityMult * focusMult)));
    }
  }

  if (lp >= SOLOQ_CHALLENGER_LP_CUTOFF) {
    return { tier: "Challenger", lp, delta: yesterdayDelta };
  }
  if (lp >= SOLOQ_GRANDMASTER_LP_CUTOFF) {
    return { tier: "Grandmaster", lp, delta: yesterdayDelta };
  }
  return { tier: "Master", lp, delta: yesterdayDelta };
}

function soloQTierClass(tier: SoloQTier): string {
  if (tier === "Challenger") return "text-yellow-300";
  if (tier === "Grandmaster") return "text-red-300";
  return "text-fuchsia-300";
}

function soloQEmblemUrl(tier: SoloQTier): string {
  if (tier === "Challenger") {
    return "/ladder-icons/challenger.webp";
  }
  if (tier === "Grandmaster") {
    return "/ladder-icons/grandmaster.webp";
  }
  return "/ladder-icons/master.webp";
}

function soloQMasteryMultiplier(tier: SoloQTier): number {
  if (tier === "Challenger") return 1.2;
  if (tier === "Grandmaster") return 1.0;
  return 0.8;
}

function getFocusMultiplier(focus: string | null | undefined): number {
  if (!focus) return 0.85;
  if (focus === "ChampionPoolPractice") return 1.25;
  if (focus === "IndividualCoaching") return 1.0;
  if (focus === "Scrims") return 0.85;
  if (focus === "MacroSystems") return 0.75;
  if (focus === "VODReview") return 0.7;
  return 0.85;
}

function expectedGainBadge(slotIndex: number, focus: string | null | undefined): {
  label: string;
  className: string;
  baseMult: number;
} {
  const priorityWeight = [1.0, 0.65, 0.4][slotIndex] ?? 0.35;
  const focusMult = getFocusMultiplier(focus);
  if (slotIndex === 0) return { label: t("champions.high"), className: "text-gray-500 dark:text-gray-400", baseMult: priorityWeight * focusMult };
  if (slotIndex === 1) return { label: t("champions.moderate"), className: "text-gray-500 dark:text-gray-400", baseMult: priorityWeight * focusMult };
  return { label: t("champions.low"), className: "text-gray-500 dark:text-gray-400", baseMult: priorityWeight * focusMult };
}

function addDays(iso: string, days: number): string {
  const date = new Date(iso);
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString();
}

function weekdayFromIso(iso: string): number {
  const date = new Date(iso);
  return (date.getUTCDay() + 6) % 7;
}

function isSoloQDay(dateIso: string, schedule: string): boolean {
  const activeDays = SCHEDULE_TRAINING_DAYS[schedule] ?? SCHEDULE_TRAINING_DAYS.Balanced;
  return activeDays.includes(weekdayFromIso(dateIso));
}

function intensityMultiplier(intensity: string): number {
  if (intensity === "High") return 1.25;
  if (intensity === "Low") return 0.75;
  return 1.0;
}

const TIER_ORDER: Array<"S" | "A" | "B" | "C" | "D"> = ["S", "A", "B", "C", "D"];
const TIER_SORT_WEIGHT: Record<string, number> = { S: 0, A: 1, B: 2, C: 3, D: 4 };

export default function ChampionsTab({ gameState, onGameUpdate, onViewChampion }: ChampionsTabProps) {
  const { t } = useTranslation();
  const [submittingKey, setSubmittingKey] = useState<string | null>(null);
  const [metaRoleFilter, setMetaRoleFilter] = useState<"ALL" | UiRole>("ALL");
  const [delegating, setDelegating] = useState(false);
  const managerTeamId = gameState.manager.team_id;
  const patch = gameState.champion_patch;
  const staffEffects = getLolStaffEffectsForTeam(gameState, managerTeamId);

  const managerTeam = useMemo(
    () => gameState.teams.find((team) => team.id === managerTeamId) ?? null,
    [gameState.teams, managerTeamId],
  );

  const ownPlayers = useMemo(() => {
    if (!managerTeamId) return [];
    const roleOf = (player: GameStateData["players"][number]) => {
      return toUiRole(resolvePlayerCurrentLolRole(player, managerTeam));
    };

    return gameState.players
      .filter((player) => player.team_id === managerTeamId)
      .sort((a, b) => {
        const roleDiff = ROLE_ORDER[roleOf(a)] - ROLE_ORDER[roleOf(b)];
        if (roleDiff !== 0) return roleDiff;
        return a.match_name.localeCompare(b.match_name);
      });
  }, [gameState.players, managerTeam, managerTeamId]);

  const masteryMap = useMemo(() => {
    const map = new Map<string, number>();
    (gameState.champion_masteries ?? []).forEach((entry) => {
      map.set(`${entry.player_id}:${normalizeKey(entry.champion_id)}`, entry.mastery);
    });
    return map;
  }, [gameState.champion_masteries]);

  const masterySignalByPlayer = useMemo(() => {
    const bucket = new Map<string, number[]>();
    (gameState.champion_masteries ?? []).forEach((entry) => {
      const list = bucket.get(entry.player_id) ?? [];
      list.push(Number(entry.mastery ?? 25));
      bucket.set(entry.player_id, list);
    });

    const signal = new Map<string, number>();
    bucket.forEach((values, playerId) => {
      const top = [...values].sort((a, b) => b - a).slice(0, 3);
      const avg = top.length > 0 ? top.reduce((sum, value) => sum + value, 0) / top.length : 25;
      signal.set(playerId, Math.max(0, avg - 60));
    });
    return signal;
  }, [gameState.champion_masteries]);

  const discoveredSet = useMemo(
    () => new Set((patch?.discovered_champion_ids ?? []).map(normalizeKey)),
    [patch?.discovered_champion_ids],
  );

  const discoveredTierByChampion = useMemo(() => {
    const map = new Map<string, string>();
    (patch?.hidden_meta ?? []).forEach((entry) => {
      const championKey = normalizeKey(entry.champion_id);
      if (!discoveredSet.has(championKey)) return;
      const tier = (entry.tier || "C").toUpperCase();
      const previous = map.get(championKey);

      if (!previous) {
        map.set(championKey, tier);
        return;
      }

      const previousWeight = TIER_SORT_WEIGHT[previous] ?? 99;
      const currentWeight = TIER_SORT_WEIGHT[tier] ?? 99;
      if (currentWeight < previousWeight) {
        map.set(championKey, tier);
      }
    });
    return map;
  }, [patch?.hidden_meta, discoveredSet]);

  const discoveredTierByRoleChampion = useMemo(() => {
    const map = new Map<string, string>();
    (patch?.hidden_meta ?? []).forEach((entry) => {
      const championKey = normalizeKey(entry.champion_id);
      if (!discoveredSet.has(championKey)) return;
      const role = normalizeRole(entry.role);
      const tier = (entry.tier || "C").toUpperCase();
      const key = `${role}:${championKey}`;
      const previous = map.get(key);

      if (!previous) {
        map.set(key, tier);
        return;
      }

      const previousWeight = TIER_SORT_WEIGHT[previous] ?? 99;
      const currentWeight = TIER_SORT_WEIGHT[tier] ?? 99;
      if (currentWeight < previousWeight) {
        map.set(key, tier);
      }
    });
    return map;
  }, [patch?.hidden_meta, discoveredSet]);

  const discoveredMeta = useMemo(() => {
    const all = (patch?.hidden_meta ?? []).filter((entry) => discoveredSet.has(normalizeKey(entry.champion_id)));
    if (metaRoleFilter !== "ALL") {
      return all.filter((entry) => normalizeRole(entry.role) === metaRoleFilter);
    }

    const dedupBestByChampion = new Map<string, typeof all[number]>();
    all.forEach((entry) => {
      const key = normalizeKey(entry.champion_id);
      const previous = dedupBestByChampion.get(key);
      if (!previous) {
        dedupBestByChampion.set(key, entry);
        return;
      }

      const previousWeight = TIER_SORT_WEIGHT[(previous.tier || "C").toUpperCase()] ?? 99;
      const currentWeight = TIER_SORT_WEIGHT[(entry.tier || "C").toUpperCase()] ?? 99;
      if (currentWeight < previousWeight) {
        dedupBestByChampion.set(key, entry);
      }
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

  async function handleTrainingTargetChange(
    playerId: string,
    slotIndex: number,
    championId: string,
  ) {
    const key = `${playerId}:${slotIndex}`;
    setSubmittingKey(key);
    try {
      const updated = await setPlayerChampionTrainingTarget(
        playerId,
        slotIndex,
        championId.length > 0 ? championId : null,
      );
      onGameUpdate(updated);
    } finally {
      setSubmittingKey(null);
    }
  }

  async function handleDelegateTraining() {
    setDelegating(true);
    try {
      const updated = await delegateChampionTraining();
      onGameUpdate(updated);
    } finally {
      setDelegating(false);
    }
  }

  return (
    <div className="space-y-6">
      <section className="rounded-2xl border border-navy-600 bg-navy-700 p-5">
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div>
            <p className="text-xs uppercase tracking-widest text-gray-400 font-heading">
              {t("champions.patchLabel", "Patch")}
            </p>
            <h2 className="mt-1 text-2xl font-heading font-bold text-white">
              {patch?.current_patch_label || t("champions.patchFallback")}
            </h2>
            <p className="mt-1 text-sm text-gray-300">
              {patch?.last_patch_date
                ? t("champions.patchLastDate", {
                  defaultValue: "Último update: {{date}}",
                  date: patch.last_patch_date,
                })
                : t("champions.patchPending")}
            </p>
          </div>

          <div className="min-w-56 rounded-xl border border-navy-600 bg-navy-800/30 px-4 py-3">
            <div className="flex items-center justify-between text-xs text-gray-300">
              <span className="inline-flex items-center gap-1"><Search className="h-3.5 w-3.5" />
                {t("champions.discoveryProgress")}</span>
              <span className="font-semibold text-primary-500 dark:text-primary-300">{discoveredPct}%</span>
            </div>
            <div className="mt-2 h-2 rounded-full bg-navy-700">
              <div className="h-2 rounded-full bg-primary-500" style={{ width: `${discoveredPct}%` }} />
            </div>
            <p className="mt-2 text-xs text-gray-400">
              {t("champions.staffMetaImpact", "Scout read")}: {formatStaffEffectPercent(staffEffects.metaDiscovery)} · {t("champions.staffMasteryImpact", "mastery learning")}: {formatStaffEffectPercent(staffEffects.development)}
            </p>
          </div>
        </div>

        <div className="mt-5 rounded-xl border border-navy-600 bg-navy-800/30 p-4">
          <div className="mb-3 flex items-center justify-between gap-3">
            <div className="flex items-center gap-2 text-sm text-gray-300">
              <Sparkles className="h-4 w-4" />
              <span className="font-heading uppercase tracking-wider">{t("champions.metaTitle")}</span>
            </div>
            <div className="flex items-center gap-1 rounded-lg border border-navy-600 bg-navy-900/20 p-1">
              <button
                type="button"
                onClick={() => setMetaRoleFilter("ALL")}
                className={`rounded-md border px-2 py-1 text-xs font-heading transition-colors ${metaRoleFilter === "ALL" ? "border-primary-500 bg-primary-500 text-white" : "border-navy-600 text-gray-300 hover:border-navy-500"}`}
              >
                {t("common.all")}
              </button>
              {(Object.keys(ROLE_ORDER) as UiRole[]).map((role) => (
                <button
                  key={role}
                  type="button"
                  onClick={() => setMetaRoleFilter(role)}
                  className={`flex h-8 w-8 items-center justify-center rounded-md border bg-navy-900/70 p-0 transition-colors ${metaRoleFilter === role ? "border-primary-500 bg-primary-500/10" : "border-navy-600 hover:border-navy-500"}`}
                  title={role}
                >
                  <img src={ROLE_ICON_URLS[role]} alt={role} className="h-4 w-4 object-contain" />
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-1">
            {TIER_ORDER.map((tier) => (
              <div key={tier} className="grid grid-cols-[56px_1fr] overflow-hidden rounded-lg border border-navy-600">
                <div className="flex items-center justify-center text-xl font-heading font-bold text-gray-200">
                  {tier}
                </div>
                <div className="min-h-[70px] bg-transparent p-2">
                  {tierRows[tier].length === 0 ? (
                    <p className="text-xs text-gray-500 italic">—</p>
                  ) : (
                    <div className="flex flex-wrap gap-2">
                      {tierRows[tier].map((entry) => (
                        <button
                          type="button"
                          key={`${tier}-${entry.champion_id}-${entry.role}`}
                          onClick={() => onViewChampion(entry.champion_id)}
                          className="relative group cursor-pointer"
                        >
                          <div className="h-14 w-24 rounded-md border border-navy-500/80 bg-navy-800 transition-all duration-300 group-hover:-translate-y-0.5 group-hover:border-yellow-300 overflow-hidden">
                            <img
                              src={resolveChampionTile(entry.champion_id) ?? ""}
                              alt={championDisplayName(entry.champion_id)}
                              className="h-full w-full object-cover"
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
        </div>
      </section>

      <section className="rounded-2xl border border-gray-200 dark:border-navy-600 bg-white dark:bg-navy-800 p-4">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <Clock3 className="h-4 w-4 text-amber-500" />
            <h3 className="font-heading font-bold uppercase tracking-wide text-gray-800 dark:text-gray-100">
              {t("champions.masteryTrainingTitle", "Mastery training")}
            </h3>
          </div>
          <button
            type="button"
            onClick={handleDelegateTraining}
            disabled={delegating}
            className="rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-1.5 text-xs font-heading uppercase tracking-wide text-amber-300 transition-all hover:bg-amber-500/20 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {delegating
              ? t("champions.delegating", "Delegating...")
              : t("champions.delegateToCoach", "Delegate to Assistant Coach")}
          </button>
        </div>

        <div className="space-y-3">
          {ownPlayers.map((player) => {
            const role = toUiRole(resolvePlayerCurrentLolRole(player, managerTeam));
            const roleChampions = CHAMPIONS_BY_ROLE[role] ?? [];
            const sortedRoleChampions = [...roleChampions].sort((a, b) => {
              const aKey = normalizeKey(a);
              const bKey = normalizeKey(b);
              const aMastery = masteryMap.get(`${player.id}:${aKey}`) ?? 25;
              const bMastery = masteryMap.get(`${player.id}:${bKey}`) ?? 25;
              if (aMastery !== bMastery) return bMastery - aMastery;

              const aTier = discoveredTierByRoleChampion.get(`${role}:${aKey}`)
                ?? discoveredTierByChampion.get(aKey)
                ?? "";
              const bTier = discoveredTierByRoleChampion.get(`${role}:${bKey}`)
                ?? discoveredTierByChampion.get(bKey)
                ?? "";
              const aTierWeight = TIER_SORT_WEIGHT[aTier] ?? 99;
              const bTierWeight = TIER_SORT_WEIGHT[bTier] ?? 99;
              if (aTierWeight !== bTierWeight) return aTierWeight - bTierWeight;

              return a.localeCompare(b);
            });

            const targetsRaw = player.champion_training_targets ?? [];
            const legacy = player.champion_training_target ?? "";
            const targets = [
              targetsRaw[0] ?? legacy,
              targetsRaw[1] ?? "",
              targetsRaw[2] ?? "",
            ];
            const effectiveFocus = player.training_focus ?? managerTeam?.training_focus ?? null;
            const effectiveIntensity = managerTeam?.training_intensity ?? "Medium";
            const effectiveSchedule = managerTeam?.training_schedule ?? "Balanced";
            const soloQ = computeSoloQ(
              player,
              gameState,
              masterySignalByPlayer.get(player.id) ?? 0,
              effectiveFocus,
              effectiveIntensity,
              effectiveSchedule,
            );
            const soloQMult = soloQMasteryMultiplier(soloQ.tier);

            return (
              <div
                key={player.id}
                className="rounded-xl border border-navy-600 bg-transparent p-3 transition-all duration-300 hover:border-navy-500"
              >
                <div className="mb-2 flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div className="h-10 w-10 overflow-hidden rounded-lg bg-navy-800">
                      {resolvePlayerPhoto(player.id, player.match_name, player.profile_image_url) ? (
                        <img
                          src={resolvePlayerPhoto(player.id, player.match_name, player.profile_image_url) ?? ""}
                          alt={player.match_name}
                          className="h-full w-full object-cover"
                        />
                      ) : (
                        <div className="h-full w-full bg-linear-to-br from-navy-600 to-navy-800" />
                      )}
                    </div>
                    <div>
                      <p className="text-sm font-semibold text-gray-100">{player.match_name}</p>
                      <div className="mt-1 flex h-6 w-6 items-center justify-center rounded-md border border-navy-600 bg-navy-900/70">
                        <img src={ROLE_ICON_URLS[role]} alt={role} className="h-3.5 w-3.5 object-contain" />
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center gap-3">
                    <div className="text-right">
                      <p className={`text-xs font-heading uppercase tracking-wide ${soloQTierClass(soloQ.tier)}`}>
                        {soloQ.tier}
                      </p>
                      <p className="text-xs text-gray-100 font-semibold">
                        {soloQ.lp} LP
                        <span className={`ml-1 ${soloQ.delta >= 0 ? "text-emerald-300" : "text-rose-300"}`}>
                          {soloQ.delta >= 0 ? `+${soloQ.delta}` : soloQ.delta}
                        </span>
                      </p>
                      <p className="text-2xs text-gray-300">x{soloQMult.toFixed(1)} mastery</p>
                    </div>
                    <img
                      src={soloQEmblemUrl(soloQ.tier)}
                      alt={soloQ.tier}
                      className="h-16 w-16 object-contain drop-shadow-[0_0_10px_rgba(0,0,0,0.5)]"
                    />
                  </div>
                </div>

                <div className="grid gap-3 md:grid-cols-3">
                  {targets.map((target, slotIndex) => {
                    const masteryValue = target
                      ? masteryMap.get(`${player.id}:${normalizeKey(target)}`) ?? 25
                      : 25;
                    const gainHint = expectedGainBadge(slotIndex, effectiveFocus);
                    const slotTitle = slotIndex === 0
                      ? t("champions.priorityHigh")
                      : slotIndex === 1
                        ? t("champions.priorityMedium")
                        : t("champions.priorityLow");
                    const slotDesc = slotIndex === 0
                      ? t("champions.priorityHighDesc")
                      : slotIndex === 1
                        ? t("champions.priorityMediumDesc")
                        : t("champions.priorityLowDesc");

                    return (
                      <div
                        key={`${player.id}-slot-${slotIndex}`}
                        className="rounded-xl border-2 border-navy-600 bg-navy-800/30 p-3 text-left transition-all hover:border-navy-500"
                      >
                        <div className="mb-2 flex items-start justify-between gap-2">
                          <div>
                            <p className="text-2xs font-heading uppercase tracking-wider text-gray-400">
                              P{slotIndex + 1}
                            </p>
                            <p className="font-heading text-xs font-bold uppercase tracking-wider text-gray-200">
                              {slotTitle}
                            </p>
                          </div>
                          <p className={`text-2xs font-heading uppercase tracking-wide ${gainHint.className}`}>
                            {t("champions.gain")} {gainHint.label}
                          </p>
                        </div>
                        <p className="mb-2 text-xs text-gray-400">{slotDesc}</p>

                        <select
                          value={target}
                          disabled={submittingKey === `${player.id}:${slotIndex}`}
                          onChange={(event) => {
                            void handleTrainingTargetChange(player.id, slotIndex, event.target.value);
                          }}
                          className="w-full rounded-md border border-navy-600 bg-navy-700/80 text-sm text-gray-100 px-2 py-1.5"
                        >
                          <option value="">{t("champions.noTarget")}</option>
                          {sortedRoleChampions.map((champion) => {
                            const championKey = normalizeKey(champion);
                            const mastery = masteryMap.get(`${player.id}:${championKey}`) ?? 25;
                            const discoveredTier = discoveredTierByRoleChampion.get(`${role}:${championKey}`)
                              ?? discoveredTierByChampion.get(championKey)
                              ?? "?";
                            return (
                              <option key={`${player.id}-${slotIndex}-${champion}`} value={champion}>
                                {`${championDisplayName(champion)} · M${mastery} · Tier ${discoveredTier}`}
                              </option>
                            );
                          })}
                        </select>
                        <div className="mt-1 h-1.5 rounded-full bg-navy-700">
                          <div
                            className="h-1.5 rounded-full bg-primary-500 transition-all duration-500"
                            style={{ width: `${Math.min(100, masteryValue)}%` }}
                          />
                        </div>
                        <div className="mt-2 flex flex-wrap gap-2">
                          <span className="text-2xs font-heading uppercase tracking-wider text-gray-400">
                            {t("champions.mastery")} {masteryValue}
                          </span>
                          <span className="text-2xs font-heading uppercase tracking-wider text-gray-400">
                            {t("training.effectiveFocus")} x{gainHint.baseMult.toFixed(2)}
                          </span>
                          <span className="text-2xs font-heading uppercase tracking-wider text-gray-400">
                            SoloQ x{soloQMult.toFixed(1)}
                          </span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      </section>
    </div>
  );
}

