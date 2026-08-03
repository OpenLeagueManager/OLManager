import { useMemo, useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  ChevronRight,
  Repeat,
  ShoppingCart,
  User,
  Loader2,
  RefreshCw,
} from "lucide-react";

import type { GameStateData, PlayerSelectionOptions } from "@/store/gameStore";
import {
  buildActiveLineupIds,
  buildActiveLineupSlots,
  isPlayerOutOfPosition,
  LOL_ACTIVE_ROLES,
  LOL_ROLE_LABELS,
} from "@/lib/squad/helpers";
import { calculateLolOvr } from "@/lib/players/lolPlayerStats";
import { resolvePlayerPhoto } from "@/lib/players/playerPhotos";
import { resolvePlayerLolRole, type LolRoleTag } from "@/lib/players/lolIdentity";
import ContextMenu from "@/ui-v2/_legacy/components/ContextMenu";
import { calcAge, formatVal } from "@/lib/common/helpers";
import { PlayerAvatar } from "@/ui-v2/_legacy/components/ui/PlayerAvatar";

import { Card, CardContent, CardHeader, CardTitle } from "@/ui-v2/components/ui/card";

import { cn } from "@/ui-v2/lib/utils";
import { ROLE_ICON_PATHS } from "@/lib/players/roleIcons";

// ─── Types ──────────────────────────────────────────────────────

type SortKey = "pos" | "ovr" | "condition" | "fitness" | "morale" | "age";

const LOL_ROLE_ORDER: Record<LolRoleTag, number> = {
  TOP: 1,
  JUNGLE: 2,
  MID: 3,
  ADC: 4,
  SUPPORT: 5,
};

const ROLE_ICON_URLS: Record<string, string> = {
  TOP: ROLE_ICON_PATHS.TOP,
  JUNGLE: ROLE_ICON_PATHS.JUNGLE,
  MID: ROLE_ICON_PATHS.MID,
  ADC: ROLE_ICON_PATHS.ADC,
  SUPPORT: ROLE_ICON_PATHS.SUPPORT,
};

function clampBar(value: number): number {
  return Math.max(0, Math.min(100, value));
}

// ─── Props ──────────────────────────────────────────────────────

interface SquadTabV2Props {
  gameState: GameStateData;
  onGameUpdate: (g: GameStateData) => void;
  onSelectPlayer: (id: string, options?: PlayerSelectionOptions) => void;
}

// ─── Component ──────────────────────────────────────────────────

export function SquadTabV2({
  gameState,
  onGameUpdate,
  onSelectPlayer,
}: SquadTabV2Props) {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);
  useEffect(() => { setVisible(true); }, []);
  const myTeam = gameState.teams.find((tm) => tm.id === gameState.manager.team_id);
  const [sortKey, setSortKey] = useState<SortKey>("pos");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("asc");
  const [search, setSearch] = useState("");
  const [savingSlot, setSavingSlot] = useState<string | null>(null);
  const [swappingSlot, setSwappingSlot] = useState<number | null>(null);

  const roleSubIds: string[] = myTeam.role_sub_ids ?? [];

  const handleSetRoleSub = useCallback(async (roleIndex: number, playerId: string) => {
    const newSubs = [...roleSubIds];
    while (newSubs.length < 5) newSubs.push("");
    newSubs[roleIndex] = playerId;
    try {
      const updated = await invoke<GameStateData>("set_role_subs", { subIds: newSubs });
      onGameUpdate(updated);
    } catch (err) {
      console.error("[SquadTab] Failed to set role sub:", err);
    }
  }, [roleSubIds, onGameUpdate]);

  const handleToggleTransfer = useCallback(async (playerId: string) => {
    try {
      const updated = await invoke<GameStateData>("toggle_transfer_list", { playerId });
      onGameUpdate(updated);
    } catch { /* silent */ }
  }, [onGameUpdate]);

  const handleToggleLoan = useCallback(async (playerId: string) => {
    try {
      const updated = await invoke<GameStateData>("toggle_loan_list", { playerId });
      onGameUpdate(updated);
    } catch { /* silent */ }
  }, [onGameUpdate]);

  // ─── Derived data ────────────────────────────────────────────────
  if (!myTeam) {
    return (
      <div className="flex flex-1 items-center justify-center p-6">
        <Card>
          <CardContent className="py-12 text-center">
            <p className="font-heading text-sm font-bold uppercase tracking-wider text-muted-foreground">
              {t("common.unemployed", { defaultValue: "Sin equipo" })}
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  const roster = gameState.players.filter(
    (player) => player.team_id === myTeam.id,
  );
  const activeLineupIds = buildActiveLineupIds(
    roster,
    myTeam.active_lineup_ids ?? myTeam.starting_xi_ids ?? [],
  );
  const activeIds = new Set(activeLineupIds);
  const playersById = useMemo(
    () => new Map(roster.map((player) => [player.id, player])),
    [roster],
  );
  const activeLineupSlots = useMemo(
    () => buildActiveLineupSlots(LOL_ACTIVE_ROLES, activeLineupIds, playersById),
    [activeLineupIds, playersById],
  );

  // Auto-save lineup if computed differs from stored and all slots filled
  useEffect(() => {
    const storedIds = myTeam.active_lineup_ids ?? myTeam.starting_xi_ids ?? [];
    const storedStr = JSON.stringify(storedIds);
    const computedStr = JSON.stringify(activeLineupIds);
    if (activeLineupIds.every(Boolean) && storedStr !== computedStr) {
      invoke("set_active_lineup", { playerIds: activeLineupIds }).catch((e) =>
        console.warn("[SquadTab] auto-save lineup failed:", e),
      );
    }
  }, []); // only on mount

  const sortedRoster = useMemo(() => {
    const sorted = [...roster].sort((a, b) => {
      switch (sortKey) {
        case "pos":
          return (
            LOL_ROLE_ORDER[resolvePlayerLolRole(a)] - LOL_ROLE_ORDER[resolvePlayerLolRole(b)] ||
            calculateLolOvr(b) - calculateLolOvr(a)
          );
        case "ovr":
          return calculateLolOvr(a) - calculateLolOvr(b);
        case "condition":
          return a.condition - b.condition;
        case "fitness":
          return (a.fitness ?? 75) - (b.fitness ?? 75);
        case "morale":
          return a.morale - b.morale;
        case "age":
          return (
            calcAge(a.date_of_birth, gameState.clock.current_date) -
            calcAge(b.date_of_birth, gameState.clock.current_date)
          );
        default:
          return 0;
      }
    });
    return sortDir === "desc" ? sorted.reverse() : sorted;
  }, [gameState.clock.current_date, roster, sortDir, sortKey]);

  const toggleSort = (nextKey: SortKey): void => {
    if (sortKey === nextKey) {
      setSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
      return;
    }
    setSortKey(nextKey);
    setSortDir(nextKey === "ovr" ? "desc" : "asc");
  };

  const benchPlayers = sortedRoster.filter((p) => !activeIds.has(p.id));
  const benchFiltered = search.trim()
    ? benchPlayers.filter((p) =>
        p.match_name.toLowerCase().includes(search.toLowerCase()),
      )
    : benchPlayers;
  const hasRoster = roster.length > 0;

  const roleCounts = useMemo(() => {
    const counts: Record<string, number> = { TOP: 0, JUNGLE: 0, MID: 0, ADC: 0, SUPPORT: 0 };
    roster.forEach((p) => { const r = resolvePlayerLolRole(p); if (counts[r] !== undefined) counts[r]++; });
    return counts;
  }, [roster]);
  const lowestCondition = roster.length > 0 ? Math.min(...roster.map((p) => p.condition)) : 0;
  const lowestConditionPlayer = roster.find((p) => p.condition === lowestCondition);

  const handleSlotChange = useCallback(async (slotIndex: number, playerId: string) => {
    const newIds = [...activeLineupIds];
    newIds[slotIndex] = playerId;
    for (let i = 0; i < newIds.length; i++) {
      if (i !== slotIndex && newIds[i] === playerId) {
        newIds[i] = "";
      }
    }
    console.debug("[SquadTab] handleSlotChange", { slotIndex, playerId, newIds, oldIds: activeLineupIds });
    setSavingSlot(String(slotIndex));
    try {
      const updated = await invoke<GameStateData>("set_active_lineup", { playerIds: newIds });
      console.debug("[SquadTab] set_active_lineup response", updated);
      onGameUpdate(updated);
    } catch (err) {
      console.error("[SquadTab] Failed to set active lineup:", err);
    } finally {
      setSavingSlot(null);
    }
  }, [activeLineupIds, onGameUpdate]);

  // ─── Render ──────────────────────────────────────────────────────
  return (
    <div className={cn("flex h-full flex-col gap-4 overflow-y-auto p-6 scrollbar-v2", visible && "animate-fade-in-up")}>
      {hasRoster && (
        <>
          {/* ── Role distribution + alerts ── */}
          <div className="opacity-0 animate-fade-in-up" style={{ animationDelay: "25ms", animationFillMode: "forwards" }}>
            <div className="flex flex-wrap items-center gap-3">
              {(Object.entries(LOL_ROLE_ORDER) as Array<[LolRoleTag, number]>).map(([role]) => (
                <div key={role} className="flex items-center gap-2 rounded-lg border border-border bg-muted/30 px-3 py-1.5">
                  <img src={ROLE_ICON_URLS[role]} alt={role} className="size-4 object-contain" />
                  <span className="font-heading text-xs font-bold tabular-nums text-foreground">{roleCounts[role]}</span>
                  <span className="font-heading text-[10px] uppercase tracking-wider text-muted-foreground">{role}</span>
                </div>
              ))}
              {lowestCondition < 50 && lowestConditionPlayer && (
                <div className="flex items-center gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-1.5">
                  <AlertTriangle className="size-4 text-amber-400" />
                  <span className="font-heading text-[10px] uppercase tracking-wider text-amber-400">
                    {lowestConditionPlayer.match_name}: {lowestCondition}% energía
                  </span>
                </div>
              )}
            </div>
          </div>
        </>
      )}

      {/* ── Active Lineup ───────────────────────────────────────── */}
      <div className="opacity-0 animate-fade-in-up" style={{ animationDelay: "0ms", animationFillMode: "forwards" }}>
      <Card>
        <CardHeader className="space-y-3">
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="font-heading text-sm uppercase tracking-widest text-muted-foreground">
              {t("squad.activeLineup", { defaultValue: "Alineación titular" })}
            </CardTitle>
            {hasRoster && (
              <span className="text-xs tabular-nums text-muted-foreground/70">
                {activeLineupSlots.filter((s) => s.player).length} / {LOL_ACTIVE_ROLES.length}
              </span>
            )}
          </div>
        </CardHeader>
        <CardContent className="p-0">
          {!hasRoster ? (
            <div className="py-8 text-center font-heading text-sm uppercase tracking-wider text-muted-foreground">
              {t("squad.noPlayers", { defaultValue: "Sin jugadores" })}
            </div>
          ) : (
            <div className="divide-y divide-border/40" data-testid="active-lineup">
              {activeLineupSlots.map((slot) => {
                const player = slot.player;
                const roleLabel = LOL_ROLE_LABELS[slot.role];
                const ovr = player ? calculateLolOvr(player) : null;
                const photo = player
                  ? resolvePlayerPhoto(player.id, player.match_name, player.profile_image_url)
                  : null;
                const outOfPosition = player ? isPlayerOutOfPosition(player, slot.role) : false;

                if (!player) {
                  return (
                    <div key={slot.role} className="flex items-center gap-3 px-4 py-3 opacity-60">
                      <div className="flex size-8 shrink-0 items-center justify-center rounded-md border border-dashed border-muted-foreground/30 bg-muted/20">
                        <img src={ROLE_ICON_URLS[slot.role]} alt={roleLabel} className="size-4 object-contain opacity-40" />
                      </div>
                      <div className="flex-1">
                        <p className="font-heading text-sm font-bold text-muted-foreground/50">{roleLabel}</p>
                        <select
                          value=""
                          onChange={(e) => handleSlotChange(slot.index, e.target.value)}
                          className="mt-1 w-full max-w-[200px] rounded-md border border-border bg-muted pl-2 pr-8 py-1 text-xs text-foreground"
                        >
                          <option value="">{t("squad.noRoleAvailable", { defaultValue: "Sin jugador" })}</option>
                          {roster
                            .filter((p) => !activeLineupIds.includes(p.id) || p.id === "")
                            .sort((a, b) => {
                              const aMatch = resolvePlayerLolRole(a) === slot.role ? 0 : 1;
                              const bMatch = resolvePlayerLolRole(b) === slot.role ? 0 : 1;
                              return aMatch - bMatch || calculateLolOvr(b) - calculateLolOvr(a);
                            })
                            .map((p) => (
                              <option key={p.id} value={p.id}>
                                {p.match_name} — {resolvePlayerLolRole(p)} ({calculateLolOvr(p)})
                              </option>
                            ))}
                        </select>
                      </div>
                      {savingSlot === String(slot.index) && <Loader2 className="size-4 shrink-0 animate-spin text-muted-foreground" />}
                    </div>
                  );
                }

                const age = calcAge(player.date_of_birth, gameState.clock.current_date);
                const condition = player.condition;
                const fitness = player.fitness ?? 75;
                const morale = player.morale;
                const annualWage = player.wage;

                  return (
                    <div key={slot.role} onClick={(e) => { if (!(e.target as HTMLElement).closest("select,button")) onSelectPlayer(player.id); }} className={cn("flex cursor-pointer items-center gap-3 px-4 py-3 transition-colors hover:bg-muted/30",
                      condition < 30 && "border-l-2 border-l-red-500/60 bg-red-500/5"
                    )}>
                    <div className="flex size-8 shrink-0 items-center justify-center rounded-md border border-border bg-muted/50">
                      <img src={ROLE_ICON_URLS[slot.role]} alt={roleLabel} className="size-4 object-contain opacity-80" />
                    </div>
                    <button type="button" onClick={() => onSelectPlayer(player.id)} className="shrink-0">
                      <PlayerAvatar src={photo} alt={player.match_name} className="size-10 cursor-pointer" />
                    </button>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <button type="button" onClick={() => onSelectPlayer(player.id)} className="truncate text-left">
                          <p className="truncate font-heading text-base font-bold text-foreground hover:text-primary transition-colors">{player.match_name}</p>
                        </button>
                        {outOfPosition && (
                          <span className="shrink-0 text-amber-400" title={t("squad.outOfPositionTooltip", { defaultValue: "Fuera de rol" })}>
                            <AlertTriangle className="size-4" />
                          </span>
                        )}
                        {condition < 30 && (
                          <span className="shrink-0 text-red-400" title={t("squad.lowConditionTooltip", { defaultValue: "Energía crítica — necesita descanso" })}>
                            <AlertTriangle className="size-4" />
                          </span>
                        )}
                      </div>
                      <div className="flex items-center gap-2">
                        <select
                          value={player.id}
                          onChange={(e) => handleSlotChange(slot.index, e.target.value)}
                          disabled={savingSlot === String(slot.index)}
                          className="max-w-[180px] rounded-md border border-border bg-muted pl-2 pr-8 py-0.5 text-xs text-foreground"
                        >
                          {roster
                            .sort((a, b) => {
                              const aMatch = resolvePlayerLolRole(a) === slot.role ? 0 : 1;
                              const bMatch = resolvePlayerLolRole(b) === slot.role ? 0 : 1;
                              return aMatch - bMatch || calculateLolOvr(b) - calculateLolOvr(a);
                            })
                            .map((p) => (
                              <option key={p.id} value={p.id}>
                                {p.match_name} — {resolvePlayerLolRole(p)} ({calculateLolOvr(p)} OVR)
                              </option>
                            ))}
                        </select>
                        {savingSlot === String(slot.index) && <Loader2 className="size-3 shrink-0 animate-spin text-muted-foreground" />}
                      </div>
                    </div>
                    <div className="hidden w-12 shrink-0 text-center md:block">
                      <p className="font-heading text-xl font-black text-primary tabular-nums">{ovr}</p>
                      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">OVR</p>
                    </div>
                    <div className="hidden w-14 shrink-0 text-center md:block">
                      <span className="font-heading text-sm font-bold text-muted-foreground">{slot.role}</span>
                    </div>
                    <div className="hidden w-28 shrink-0 lg:block">
                      <div className="mb-0.5 flex items-center justify-between">
                        <span className="text-[10px] uppercase tracking-wider text-muted-foreground">Energía</span>
                        <span className="font-heading text-[11px] font-bold text-amber-400 tabular-nums">{condition}</span>
                      </div>
                      <div className="relative h-1.5 w-full overflow-hidden rounded-full">
                        <div className="absolute inset-0 rounded-full" style={{ background: 'linear-gradient(to right, #f59e0b, #22c55e)' }} />
                        <div
                          className="absolute inset-y-0 right-0 bg-muted transition-all duration-500"
                          style={{ width: `${100 - clampBar(condition)}%` }}
                        />
                      </div>
                    </div>
                    <div className="hidden w-28 shrink-0 lg:block">
                      <div className="mb-0.5 flex items-center justify-between">
                        <span className="text-[10px] uppercase tracking-wider text-muted-foreground">Moral</span>
                        <span className="font-heading text-[11px] font-bold text-emerald-400 tabular-nums">{morale}</span>
                      </div>
                      <div className="relative h-1.5 w-full overflow-hidden rounded-full">
                        <div className="absolute inset-0 rounded-full" style={{ background: 'linear-gradient(to right, #3b82f6, #38bdf8)' }} />
                        <div
                          className="absolute inset-y-0 right-0 bg-muted transition-all duration-500"
                          style={{ width: `${100 - clampBar(morale)}%` }}
                        />
                      </div>
                    </div>
                    <div className="hidden w-28 shrink-0 lg:block">
                      <div className="mb-0.5 flex items-center justify-between">
                        <span className="text-[10px] uppercase tracking-wider text-muted-foreground">Fitness</span>
                        <span className="font-heading text-[11px] font-bold text-green-400 tabular-nums">{fitness}</span>
                      </div>
                      <div className="relative h-1.5 w-full overflow-hidden rounded-full">
                        <div className="absolute inset-0 rounded-full" style={{ background: 'linear-gradient(to right, #14b8a6, #2dd4bf 25%, #5eead4 50%)' }} />
                        <div
                          className="absolute inset-y-0 right-0 bg-muted transition-all duration-500"
                          style={{ width: `${100 - clampBar(fitness)}%` }}
                        />
                      </div>
                    </div>
                    <div className="hidden w-12 shrink-0 text-center lg:block">
                      <p className="font-heading text-sm font-bold text-foreground tabular-nums">{age}</p>
                      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">Edad</p>
                    </div>
                    <div className="hidden w-20 shrink-0 text-right lg:block">
                      <p className="font-heading text-sm font-bold text-foreground tabular-nums">{formatVal(annualWage)}</p>
                      <p className="text-[10px] uppercase tracking-wider text-muted-foreground">/año</p>
                    </div>
                    <div className="flex items-center gap-1">
                      {condition < 30 && (
                        <button type="button"
                          onClick={(e) => {
                            e.stopPropagation();
                            // Prefer the designated sub for this role; fall back to best OVR bench player
                            const designatedSubId = roleSubIds[slot.index];
                            const designatedSub = designatedSubId ? benchPlayers.find((bp) => bp.id === designatedSubId) : null;
                            const replacement = designatedSub ?? benchPlayers
                              .filter((bp) => resolvePlayerLolRole(bp) === slot.role)
                              .sort((a, b) => calculateLolOvr(b) - calculateLolOvr(a))[0];
                            if (replacement) handleSlotChange(slot.index, replacement.id);
                          }}
                          title={t("squad.quickSwap", { defaultValue: "Cambiar por suplente" })}
                          className="flex size-7 items-center justify-center rounded-md border border-amber-500/40 bg-amber-500/10 text-amber-400 transition-colors hover:bg-amber-500/20"
                        >
                          <RefreshCw className="size-3.5" />
                        </button>
                      )}
                      <button type="button" onClick={(e) => { e.stopPropagation(); handleToggleTransfer(player.id); }}
                        title={player.transfer_listed ? t("squad.removeFromTransferList", { defaultValue: "Remove from transfers list" }) : t("squad.addToTransferList", { defaultValue: "Add to transfers list" })}
                        className={cn("flex size-7 items-center justify-center rounded-md border transition-colors",
                          player.transfer_listed ? "border-red-500/30 bg-red-500/10 text-red-400" : "border-border text-muted-foreground/50 hover:border-red-500/30 hover:text-red-400"
                        )}>
                        <ShoppingCart className="size-3.5" />
                      </button>
                      <button type="button" onClick={(e) => { e.stopPropagation(); handleToggleLoan(player.id); }}
                        title={player.loan_listed ? t("squad.removeFromLoanList", { defaultValue: "Remove from loans list" }) : t("squad.addToLoanList", { defaultValue: "Add to loans list" })}
                        className={cn("flex size-7 items-center justify-center rounded-md border transition-colors",
                          player.loan_listed ? "border-blue-500/30 bg-blue-500/10 text-blue-400" : "border-border text-muted-foreground/50 hover:border-blue-500/30 hover:text-blue-400"
                        )}>
                        <Repeat className="size-3.5" />
                      </button>
                      <button type="button" onClick={() => onSelectPlayer(player.id)} className="shrink-0 text-muted-foreground/50 hover:text-primary">
                        <ChevronRight className="size-4" />
                      </button>
                    </div>
                    {/* Sub slot for this role */}
                    <div className="mt-1 ml-11 flex items-center gap-2">
                      {(() => {
                        const subId = roleSubIds[slot.index];
                        const sub = subId ? roster.find((p) => p.id === subId) : null;
                        return (
                          <>
                            <span className="text-[10px] uppercase tracking-wider text-muted-foreground/60 w-6">SUB</span>
                            <select
                              value={subId ?? ""}
                              onChange={(e) => handleSetRoleSub(slot.index, e.target.value)}
                              className="max-w-[160px] rounded-md border border-border/60 bg-muted/40 pl-1.5 pr-6 py-0.5 text-[11px] text-foreground"
                            >
                              <option value="">{t("squad.noSub", { defaultValue: "Sin suplente" })}</option>
                              {benchPlayers
                                .filter((bp) => !activeLineupIds.includes(bp.id))
                                .sort((a, b) => {
                                  const aMatch = resolvePlayerLolRole(a) === slot.role ? 0 : 1;
                                  const bMatch = resolvePlayerLolRole(b) === slot.role ? 0 : 1;
                                  return aMatch - bMatch || calculateLolOvr(b) - calculateLolOvr(a);
                                })
                                .map((bp) => (
                                  <option key={bp.id} value={bp.id}>
                                    {bp.match_name} — {resolvePlayerLolRole(bp)} ({calculateLolOvr(bp)})
                                  </option>
                                ))}
                            </select>
                            {sub && sub.condition < 30 && (
                              <span className="text-[10px] text-red-400" title={t("squad.subLowCondition", { defaultValue: "Suplente también tiene la energía baja" })}>
                                <AlertTriangle className="size-3" />
                              </span>
                            )}
                          </>
                        );
                      })()}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>
      </div>

      {/* ── Bench / Substitutes ─────────────────────────────────── */}
      <div className="flex-1 min-h-0 opacity-0 animate-fade-in-up" style={{ animationDelay: "50ms", animationFillMode: "forwards" }}>
      <Card className="flex h-full flex-col">
        <CardHeader className="space-y-3">
          <div className="flex items-center justify-between gap-3">
            <CardTitle className="font-heading text-sm uppercase tracking-widest text-muted-foreground">
              {t("squad.benchSubstitutes", {
                defaultValue: "Suplentes / Banca",
              })}
            </CardTitle>
            {hasRoster && (
              <span className="text-xs tabular-nums text-muted-foreground/70">
                {benchPlayers.length} / {roster.length}
              </span>
            )}
          </div>
          {/* Search + Sort */}
          {hasRoster && (
            <div className="flex flex-wrap items-center gap-2">
              <input
                type="text"
                placeholder={t("common.search", { defaultValue: "Buscar jugador..." })}
                onChange={(e) => setSearch(e.target.value)}
                className="h-8 flex-1 min-w-[140px] max-w-[200px] rounded-md border border-border bg-muted/30 px-2.5 text-xs text-foreground outline-none placeholder:text-muted-foreground/40"
              />
              {(
                [
                  ["pos", t("squad.pos", { defaultValue: "Posición" })],
                  ["ovr", t("common.ovr", { defaultValue: "OVR" })],
                  [
                    "condition",
                    t("common.condition", { defaultValue: "Energía" }),
                  ],
                  [
                    "fitness",
                    t("common.fitness", { defaultValue: "Fitness" }),
                  ],
                  ["morale", t("common.morale", { defaultValue: "Moral" })],
                  ["age", t("common.age", { defaultValue: "Edad" })],
                ] as Array<[SortKey, string]>
              ).map(([key, label]) => (
                <button
                  key={key}
                  type="button"
                  onClick={() => toggleSort(key)}
                  className={cn(
                    "rounded-md border px-2.5 py-1 font-heading text-xs font-bold uppercase tracking-wide transition-colors",
                    sortKey === key
                      ? "border-primary bg-primary/10 text-primary"
                      : "border-border bg-card text-muted-foreground hover:border-primary/50 hover:text-foreground",
                  )}
                >
                  {label}
                  {sortKey === key && (
                    <span className="ml-1">{sortDir === "asc" ? "↑" : "↓"}</span>
                  )}
                </button>
              ))}
            </div>
          )}
        </CardHeader>
        <CardContent className="flex-1 overflow-y-auto p-0">
          {benchFiltered.length === 0 && hasRoster ? (
            <div className="p-8 text-center font-heading text-sm uppercase tracking-wider text-muted-foreground">
              {t("squad.allStarting", {
                defaultValue: "Todos los jugadores en la alineación titular",
              })}
            </div>
          ) : !hasRoster ? (
            <div className="p-8 text-center font-heading text-sm uppercase tracking-wider text-muted-foreground">
              {t("squad.noPlayers", { defaultValue: "Sin jugadores" })}
            </div>
          ) : (
            <div className="divide-y divide-border/40">
              {benchFiltered.map((player) => {
                const role = resolvePlayerLolRole(player);
                const ovr = calculateLolOvr(player);
                const photo = resolvePlayerPhoto(
                  player.id,
                  player.match_name,
                  player.profile_image_url,
                );
                const age = calcAge(
                  player.date_of_birth,
                  gameState.clock.current_date,
                );
                const annualWage = player.wage;
                const condition = player.condition;
                const fitness = player.fitness ?? 75;
                const morale = player.morale;

                const contextItems = [
                  {
                    label: t("squad.viewProfile", {
                      defaultValue: "Ver perfil",
                    }),
                    icon: <User className="size-4" />,
                    onClick: () => onSelectPlayer(player.id),
                  },
                  {
                    label: "",
                    icon: undefined,
                    onClick: () => {},
                    divider: true,
                  },
                  {
                    label: player.transfer_listed
                      ? t("squad.removeFromTransferList", {
                          defaultValue: "Quitar de transferibles",
                        })
                      : t("squad.addToTransferList", {
                          defaultValue: "Añadir a transferibles",
                        }),
                    icon: <ShoppingCart className="size-4" />,
                    onClick: async () => {
                      try {
                        const updated = await invoke<GameStateData>(
                          "toggle_transfer_list",
                          { playerId: player.id },
                        );
                        onGameUpdate(updated);
                      } catch {
                        /* silent */
                      }
                    },
                  },
                  {
                    label: player.loan_listed
                      ? t("squad.removeFromLoanList", {
                          defaultValue: "Quitar de cesión",
                        })
                      : t("squad.addToLoanList", {
                          defaultValue: "Añadir a cesión",
                        }),
                    icon: <Repeat className="size-4" />,
                    onClick: async () => {
                      try {
                        const updated = await invoke<GameStateData>(
                          "toggle_loan_list",
                          { playerId: player.id },
                        );
                        onGameUpdate(updated);
                      } catch {
                        /* silent */
                      }
                    },
                  },
                ];

                return (
                  <ContextMenu items={contextItems} key={player.id}>
                    <button
                      type="button"
                      onClick={() => onSelectPlayer(player.id)}
                      className="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-muted/30"
                    >
                      {/* Role icon */}
                      <div className="flex size-8 shrink-0 items-center justify-center rounded-md border border-border bg-muted/50">
                        <img
                          src={ROLE_ICON_URLS[role]}
                          alt={role}
                          className="size-4 object-contain opacity-80"
                        />
                      </div>

                      {/* Photo */}
                      <PlayerAvatar
                        src={photo}
                        alt={player.match_name}
                        className="size-10"
                      />

                      {/* Name + full_name */}
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-1.5">
                          <p className="truncate font-heading text-base font-bold text-foreground">
                            {player.match_name}
                          </p>
                          {player.transfer_listed && (
                            <span className="shrink-0 rounded bg-red-500/10 px-1 py-0.5 text-[9px] font-heading font-bold uppercase tracking-wider text-red-400">
                              {t("common.transferListed", { defaultValue: "TR" })}
                            </span>
                          )}
                          {player.loan_listed && (
                            <span className="shrink-0 rounded bg-blue-500/10 px-1 py-0.5 text-[9px] font-heading font-bold uppercase tracking-wider text-blue-400">
                              {t("common.loanListed", { defaultValue: "CD" })}
                            </span>
                          )}
                        </div>
                        <p className="truncate text-xs text-muted-foreground">
                          {player.full_name}
                        </p>
                      </div>

                      {/* OVR — visible md+ */}
                      <div className="hidden w-12 shrink-0 text-center md:block">
                        <p className="font-heading text-xl font-black text-primary tabular-nums">
                          {ovr}
                        </p>
                        <p className="text-[10px] uppercase tracking-wider text-muted-foreground">
                          OVR
                        </p>
                      </div>

                      {/* Role badge — visible md+ */}
                      <div className="hidden w-14 shrink-0 text-center md:block">
                        <span className="font-heading text-sm font-bold text-muted-foreground">
                          {role}
                        </span>
                      </div>

                      {/* Condition bar — visible lg+ */}
                      <div className="hidden w-28 shrink-0 lg:block">
                        <div className="mb-0.5 flex items-center justify-between">
                          <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
                            {t("common.condition", { defaultValue: "Energía" })}
                          </span>
                          <span className="font-heading text-[11px] font-bold text-amber-400 tabular-nums">
                            {condition}
                          </span>
                        </div>
                        <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                          <div
                            className={cn(
                              "h-full rounded-full transition-all",
                              condition <= 0
                                ? "bg-amber-400/30"
                                : "bg-amber-400",
                            )}
                            style={{
                              width: `${clampBar(condition)}%`,
                            }}
                          />
                        </div>
                      </div>

                      {/* Morale bar — visible lg+ */}
                      <div className="hidden w-28 shrink-0 lg:block">
                        <div className="mb-0.5 flex items-center justify-between">
                          <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
                            {t("common.morale", { defaultValue: "Moral" })}
                          </span>
                          <span className="font-heading text-[11px] font-bold text-emerald-400 tabular-nums">
                            {morale}
                          </span>
                        </div>
                        <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                          <div
                            className={cn(
                              "h-full rounded-full transition-all",
                              morale <= 0
                                ? "bg-emerald-400/30"
                                : "bg-emerald-400",
                            )}
                            style={{
                              width: `${clampBar(morale)}%`,
                            }}
                          />
                        </div>
                      </div>

                      {/* Fitness bar — visible lg+ */}
                      <div className="hidden w-28 shrink-0 lg:block">
                        <div className="mb-0.5 flex items-center justify-between">
                          <span className="text-[10px] uppercase tracking-wider text-muted-foreground">
                            {t("common.fitness", { defaultValue: "Fitness" })}
                          </span>
                          <span className="font-heading text-[11px] font-bold text-green-400 tabular-nums">
                            {fitness}
                          </span>
                        </div>
                        <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                          <div
                            className={cn(
                              "h-full rounded-full transition-all",
                              fitness <= 0
                                ? "bg-green-400/30"
                                : "bg-green-400",
                            )}
                            style={{
                              width: `${clampBar(fitness)}%`,
                            }}
                          />
                        </div>
                      </div>

                      {/* Age — visible lg+ */}
                      <div className="hidden w-12 shrink-0 text-center lg:block">
                        <p className="font-heading text-sm font-bold text-foreground tabular-nums">
                          {age}
                        </p>
                        <p className="text-[10px] uppercase tracking-wider text-muted-foreground">
                          {t("common.age", { defaultValue: "Edad" })}
                        </p>
                      </div>

                      {/* Wage — visible lg+ */}
                      <div className="hidden w-20 shrink-0 text-right lg:block">
                        <p className="font-heading text-sm font-bold text-foreground tabular-nums">
                          {formatVal(annualWage)}
                        </p>
                        <p className="text-[10px] uppercase tracking-wider text-muted-foreground">
                          {t("common.per_year_with_slash", {
                            defaultValue: "/año",
                          })}
                        </p>
                      </div>

                      {/* Actions */}
                      <div className="flex items-center gap-1 shrink-0">
                        <button type="button" onClick={(e) => { e.stopPropagation(); handleToggleTransfer(player.id); }}
                          title={player.transfer_listed ? t("squad.removeFromTransferList", { defaultValue: "Remove from transfers list" }) : t("squad.addToTransferList", { defaultValue: "Add to transfers list" })}
                          className={cn("flex size-7 items-center justify-center rounded-md border transition-colors",
                            player.transfer_listed ? "border-red-500/30 bg-red-500/10 text-red-400" : "border-border text-muted-foreground/50 hover:border-red-500/30 hover:text-red-400"
                          )}>
                          <ShoppingCart className="size-3.5" />
                        </button>
                        <button type="button" onClick={(e) => { e.stopPropagation(); handleToggleLoan(player.id); }}
                          title={player.loan_listed ? t("squad.removeFromLoanList", { defaultValue: "Remove from loans list" }) : t("squad.addToLoanList", { defaultValue: "Add to loans list" })}
                          className={cn("flex size-7 items-center justify-center rounded-md border transition-colors",
                            player.loan_listed ? "border-blue-500/30 bg-blue-500/10 text-blue-400" : "border-border text-muted-foreground/50 hover:border-blue-500/30 hover:text-blue-400"
                          )}>
                          <Repeat className="size-3.5" />
                        </button>
                        <ChevronRight className="size-4 text-muted-foreground/50" />
                      </div>
                    </button>
                  </ContextMenu>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>
      </div>
    </div>
  );
}
