import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowDown,
  ArrowUp,
  ArrowUpDown,
  EyeOff,
  GraduationCap,
  Info,
  Loader2,
  Search,
  ShoppingCart,
  Sparkles,
  Star,
  TrendingUp,
  Users,
} from "lucide-react";

import { invoke } from "@tauri-apps/api/core";
import type { GameStateData, PlayerData, AcademyAcquisitionOptionData } from "@/store/gameStore";
import { findAcademyTeamForParent, getTeamAcademyRoster } from "@/store/academySelectors";
import {
  acquireAcademyTeam,
  getAcademyAcquisitionOptions,
  promoteAcademyPlayer,
} from "@/services/academyService";
import { calcAge } from "@/lib/common/helpers";
import { resolvePlayerLolRole } from "@/lib/players/lolIdentity";
import { resolveTeamLogo } from "@/lib/teams/teamLogos";
import { resolvePlayerPhoto } from "@/lib/players/playerPhotos";
import { ROLE_ICON_PATHS } from "@/lib/players/roleIcons";

import { Badge } from "@/ui-v2/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/ui-v2/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/ui-v2/components/ui/table";
import { cn } from "@/ui-v2/lib/utils";

interface YouthTabV2Props {
  gameState: GameStateData;
  onSelectPlayer?: (id: string) => void;
  onSelectTeam?: (id: string) => void;
  onGameUpdate?: (state: GameStateData) => void;
}

type DraftRole = "TOP" | "JUNGLE" | "MID" | "ADC" | "SUPPORT";

const ROLE_ORDER: Record<DraftRole, number> = {
  TOP: 1,
  JUNGLE: 2,
  MID: 3,
  ADC: 4,
  SUPPORT: 5,
};

const ROLE_ICON_MAP: Record<string, string> = {
  TOP: ROLE_ICON_PATHS.TOP,
  JUNGLE: ROLE_ICON_PATHS.JUNGLE,
  MID: ROLE_ICON_PATHS.MID,
  ADC: ROLE_ICON_PATHS.ADC,
  SUPPORT: ROLE_ICON_PATHS.SUPPORT,
};

type SortKey = "name" | "pos" | "age" | "ovr" | "potential" | "condition";

function getLolOvr(player: PlayerData): number {
  const attrs = player.attributes;
  const avg =
    (Number(attrs.mechanics ?? 0) +
      Number(attrs.laning ?? 0) +
      Number(attrs.teamfighting ?? 0) +
      Number(attrs.macro_play ?? 0) +
      Number(attrs.consistency ?? 0) +
      Number(attrs.shotcalling ?? 0) +
      Number(attrs.champion_pool ?? 0) +
      Number(attrs.discipline ?? 0) +
      Number(attrs.mental_resilience ?? 0)) /
    9;
  return Math.max(1, Math.min(99, Math.round(avg)));
}

export function YouthTabV2({ gameState, onSelectPlayer, onSelectTeam, onGameUpdate }: YouthTabV2Props) {
  const { t } = useTranslation();
  const myTeam = gameState.teams.find((team) => team.id === gameState.manager.team_id);
  const academyTeam = useMemo(
    () => findAcademyTeamForParent(gameState.teams, myTeam?.id),
    [gameState.teams, myTeam?.id],
  );

  const [promotingPlayerId, setPromotingPlayerId] = useState<string | null>(null);
  const [transferListingPlayerId, setTransferListingPlayerId] = useState<string | null>(null);
  const [acquisitionOptions, setAcquisitionOptions] = useState<AcademyAcquisitionOptionData[]>([]);
  const [acquisitionBlockedReason, setAcquisitionBlockedReason] = useState<string | null>(null);
  const [acquisitionLoading, setAcquisitionLoading] = useState(false);
  const [acquiringSourceId, setAcquiringSourceId] = useState<string | null>(null);
  const [academyCustomName, setAcademyCustomName] = useState("");
  const [academyCustomShortName, setAcademyCustomShortName] = useState("");
  const [academyCustomLogoUrl, setAcademyCustomLogoUrl] = useState("");

  useEffect(() => {
    let cancelled = false;
    if (academyTeam || !myTeam?.id) {
      setAcquisitionOptions([]);
      setAcquisitionBlockedReason(null);
      setAcquisitionLoading(false);
      return;
    }

    setAcquisitionLoading(true);
    getAcademyAcquisitionOptions(myTeam.id)
      .then((response) => {
        if (cancelled) return;
        setAcquisitionOptions(response.options ?? []);
        setAcquisitionBlockedReason(response.blocked_reason ?? null);
      })
      .catch(() => {
        if (cancelled) return;
        setAcquisitionOptions([]);
        setAcquisitionBlockedReason("youthAcademy.loadOptionsError");
      })
      .finally(() => {
        if (cancelled) return;
        setAcquisitionLoading(false);
      });

    return () => { cancelled = true; };
  }, [academyTeam, myTeam?.id]);

  const youthPlayers = useMemo(
    () =>
      (myTeam ? getTeamAcademyRoster(gameState.teams, gameState.players, myTeam.id) : [])
        .map((player) => {
          const role = resolvePlayerLolRole(player) as DraftRole;
          const ovr = getLolOvr(player);
          const age = calcAge(player.date_of_birth, gameState.clock.current_date);
          const potential = player.potential_revealed ?? null;
          return { ...player, role, age, ovr, potential };
        })
        .sort((a, b) => {
          const byRole = ROLE_ORDER[a.role] - ROLE_ORDER[b.role];
          if (byRole !== 0) return byRole;
          const byOvr = b.ovr - a.ovr;
          if (byOvr !== 0) return byOvr;
          return (a.match_name || a.full_name).localeCompare(b.match_name || b.full_name);
        }),
    [gameState.clock.current_date, gameState.players, gameState.teams, myTeam],
  );

  const avgOvr = youthPlayers.length > 0
    ? Math.round(youthPlayers.reduce((sum, player) => sum + player.ovr, 0) / youthPlayers.length)
    : 0;

  const revealedPotentials = youthPlayers
    .map((player) => player.potential)
    .filter((value): value is number => typeof value === "number");
  const avgPotential =
    revealedPotentials.length > 0
      ? Math.round(revealedPotentials.reduce((sum, value) => sum + value, 0) / revealedPotentials.length)
      : null;
  const highPotential = revealedPotentials.filter((value) => value >= 75).length;

  const youthCoach = gameState.staff.filter(
    (staff) => staff.team_id === myTeam?.id && staff.specialization === "Youth",
  );

  const [sortKey, setSortKey] = useState<SortKey>("pos");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("asc");

  const toggleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir(key === "ovr" || key === "potential" || key === "condition" || key === "age" ? "desc" : "asc");
    }
  };

  const sortedPlayers = useMemo(() => {
    return [...youthPlayers].sort((a, b) => {
      const dir = sortDir === "asc" ? 1 : -1;
      switch (sortKey) {
        case "name":
          return dir * (a.match_name || a.full_name).localeCompare(b.match_name || b.full_name);
        case "pos":
          return dir * ((ROLE_ORDER[a.role] ?? 99) - (ROLE_ORDER[b.role] ?? 99));
        case "age":
          return dir * (a.age - b.age);
        case "ovr":
          return dir * (a.ovr - b.ovr);
        case "potential": {
          const pa = a.potential ?? -1;
          const pb = b.potential ?? -1;
          return dir * (pa - pb);
        }
        case "condition":
          return dir * ((a.condition ?? 0) - (b.condition ?? 0));
        default:
          return 0;
      }
    });
  }, [youthPlayers, sortKey, sortDir]);

  const [searchQuery, setSearchQuery] = useState("");
  const filteredPlayers = useMemo(
    () => sortedPlayers.filter((p) => {
      if (!searchQuery.trim()) return true;
      const q = searchQuery.toLowerCase();
      return (
        (p.match_name || "").toLowerCase().includes(q) ||
        (p.full_name || "").toLowerCase().includes(q) ||
        (p.position || "").toLowerCase().includes(q)
      );
    }),
    [sortedPlayers, searchQuery],
  );

  function renderSortIcon(key: SortKey) {
    if (sortKey !== key) return <ArrowUpDown className="size-3 opacity-40" />;
    return sortDir === "asc" ? <ArrowUp className="size-3" /> : <ArrowDown className="size-3" />;
  }

  const KPI_CLASS = "tabular-nums font-heading text-2xl font-bold text-foreground";

  if (!myTeam) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6">
        <Users className="size-12 text-muted-foreground/40" />
        <p className="text-sm text-muted-foreground">{t("youthAcademy.noYouthPlayers")}</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary">
          {academyTeam ? (
            (() => {
              const academyLogo = academyTeam.logo_url
                ?? academyTeam.academy?.branding?.current_logo_url
                ?? academyTeam.academy?.acquisition?.original_logo_url
                ?? academyTeam.academy?.source_identity?.original_logo_url
                ?? myTeam?.logo_url;
              const logo = resolveTeamLogo(academyTeam.name, academyLogo);
              return logo ? (
                <img src={logo} alt={academyTeam.name} className="size-8 object-contain" />
              ) : (
                <GraduationCap className="size-5" />
              );
            })()
          ) : (
            <GraduationCap className="size-5" />
          )}
        </div>
        <div>
          <h2 className="font-heading text-lg font-bold uppercase tracking-wider text-foreground">
            {t("youthAcademy.title")}
          </h2>
          <div className="mt-0.5 flex items-center gap-2">
            <Badge variant="secondary" className="text-[10px]">
              {youthPlayers.length} {t("youthAcademy.academyPlayers")}
            </Badge>
            {academyTeam && (
              <Badge variant="outline" className="text-[10px] text-muted-foreground">
                {academyTeam.name}
              </Badge>
            )}
          </div>
        </div>
      </div>

      {/* KPI Cards */}
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Card>
          <CardContent className="flex flex-col items-center py-4">
            <Users className="mb-1 size-5 text-muted-foreground" />
            <p className={KPI_CLASS}>{youthPlayers.length}</p>
            <p className="mt-0.5 text-[10px] uppercase tracking-widest text-muted-foreground">
              {t("youthAcademy.academyPlayersStartingMayus")}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="flex flex-col items-center py-4">
            <Star className="mb-1 size-5 text-primary" />
            <p className={KPI_CLASS}>{avgOvr}</p>
            <div className="mt-1 h-1 w-20 overflow-hidden rounded-full bg-muted">
              <div className="h-full rounded-full bg-primary" style={{ width: `${avgOvr}%` }} />
            </div>
            <p className="mt-0.5 text-[10px] uppercase tracking-widest text-muted-foreground">
              {t("youthAcademy.avgOvr")}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="flex flex-col items-center py-4">
            <TrendingUp className="mb-1 size-5 text-emerald-400" />
            {avgPotential != null ? (
              <>
                <p className={KPI_CLASS}>{avgPotential}</p>
                <div className="mt-1 h-1 w-20 overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-emerald-400"
                    style={{ width: `${avgPotential}%` }}
                  />
                </div>
              </>
            ) : (
              <>
                <p className="inline-flex items-center gap-1 font-heading text-2xl tabular-nums text-muted-foreground/50">
                  ?? <Info className="size-4 text-muted-foreground/30" />
                </p>
                <div className="mt-1 h-1 w-20 overflow-hidden rounded-full bg-muted" />
              </>
            )}
            <p className="mt-0.5 text-[10px] uppercase tracking-widest text-muted-foreground">
              {t("youthAcademy.avgPotential")}
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="flex flex-col items-center py-4">
            <Sparkles className="mb-1 size-5 text-primary" />
            <p className={KPI_CLASS}>{highPotential}</p>
            {highPotential > 0 && (
              <Badge variant="default" className="mt-0.5 text-[10px]">
                {t("youthAcademy.highPotentialBadge", "Talento")}
              </Badge>
            )}
            <p className="mt-0.5 text-[10px] uppercase tracking-widest text-muted-foreground">
              {t("youthAcademy.highPotential")}
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Youth Coach Banner */}
      {youthCoach.length > 0 && (
        <Card>
          <CardContent className="flex items-center gap-2 py-2.5">
            <GraduationCap className="size-3.5 text-primary" />
            <span className="text-xs text-muted-foreground">{t("youthAcademy.youthCoach")}</span>
            {youthCoach.map((staff) => (
              <Badge key={staff.id} variant="secondary" className="text-[10px]">
                {staff.first_name} {staff.last_name} ({staff.attributes.coaching})
              </Badge>
            ))}
          </CardContent>
        </Card>
      )}

      {/* Academy Acquisition */}
      {!academyTeam && (
        <Card className="flex min-h-0 flex-1 flex-col">
          <CardHeader className="shrink-0">
            <CardTitle className="font-heading text-sm uppercase tracking-widest text-muted-foreground">
              {t("youthAcademy.academyCardTitle")}
            </CardTitle>
          </CardHeader>
          <CardContent className="flex min-h-0 flex-1 flex-col">
            <div className="flex min-h-0 flex-1 flex-col gap-3">
              <p className="shrink-0 text-sm text-muted-foreground">
                {acquisitionLoading
                  ? t("youthAcademy.acquisitionLoading")
                  : acquisitionOptions.length > 0
                    ? t("youthAcademy.acquisitionIntro")
                    : acquisitionBlockedReason === "youthAcademy.loadOptionsError"
                      ? t("youthAcademy.loadOptionsError")
                      : acquisitionBlockedReason ?? t("youthAcademy.acquisitionNoOptions")}
              </p>

              {acquisitionOptions.length > 0 && (
                <div className="grid shrink-0 gap-2 md:grid-cols-3">
                  <input
                    value={academyCustomName}
                    onChange={(e) => setAcademyCustomName(e.target.value)}
                    placeholder={t("youthAcademy.placeholderCustomName")}
                    className="h-8 rounded-md border border-border bg-muted/30 px-2.5 text-xs text-foreground placeholder:text-muted-foreground/50"
                  />
                  <input
                    value={academyCustomShortName}
                    onChange={(e) => setAcademyCustomShortName(e.target.value)}
                    placeholder={t("youthAcademy.placeholderCustomShortName")}
                    className="h-8 rounded-md border border-border bg-muted/30 px-2.5 text-xs text-foreground placeholder:text-muted-foreground/50"
                  />
                  <input
                    value={academyCustomLogoUrl}
                    onChange={(e) => setAcademyCustomLogoUrl(e.target.value)}
                    placeholder={t("youthAcademy.placeholderCustomLogoUrl")}
                    className="h-8 rounded-md border border-border bg-muted/30 px-2.5 text-xs text-foreground placeholder:text-muted-foreground/50"
                  />
                </div>
              )}

              {acquisitionOptions.length > 0 && (
                <div className="grid min-h-0 flex-1 gap-2 overflow-y-auto md:grid-cols-2 scrollbar-v2">
                  {acquisitionOptions.map((option) => {
                    const optionLogo = option.source_team_logo_url ?? resolveTeamLogo(option.source_team_name);
                    return (
                      <div
                        key={option.source_team_id}
                        onClick={() => onSelectTeam?.(option.source_team_id)}
                        className="flex cursor-pointer items-center gap-3 rounded-lg border border-border bg-muted/20 p-3 transition-colors hover:bg-muted/40"
                      >
                        <div className="flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-border bg-muted">
                          {optionLogo ? (
                            <img
                              src={optionLogo}
                              alt={option.source_team_name}
                              className="size-8 object-contain"
                              loading="lazy"
                            />
                          ) : (
                            <span className="text-xs font-heading font-bold text-muted-foreground">
                              {option.source_team_short_name}
                            </span>
                          )}
                        </div>
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-sm font-semibold text-foreground">{option.source_team_name}</p>
                          <p className="truncate text-xs text-muted-foreground">
                            {option.league_name} · {option.country} · €{option.acquisition_cost.toLocaleString()}
                          </p>
                        </div>
                        <button
                          type="button"
                          disabled={acquiringSourceId === option.source_team_id}
                          onClick={async () => {
                            if (!myTeam?.id) return;
                            try {
                              setAcquiringSourceId(option.source_team_id);
                              const updated = await acquireAcademyTeam({
                                parent_team_id: myTeam.id,
                                source_team_id: option.source_team_id,
                                custom_name: academyCustomName.trim() || undefined,
                                custom_short_name: academyCustomShortName.trim() || undefined,
                                custom_logo_url: academyCustomLogoUrl.trim() || undefined,
                              });
                              onGameUpdate?.(updated);
                            } finally {
                              setAcquiringSourceId(null);
                            }
                          }}
                          className="shrink-0 rounded-md border border-primary/30 bg-primary/10 px-3 py-1.5 text-xs font-heading uppercase tracking-wider text-primary transition-all hover:bg-primary/20 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          {acquiringSourceId === option.source_team_id ? (
                            <span className="inline-flex items-center gap-1">
                              <Loader2 className="size-3 animate-spin" />
                              {t("youthAcademy.fundingAcademy")}
                            </span>
                          ) : (
                            t("youthAcademy.fundAcademy")
                          )}
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}

              {acquisitionOptions.length === 0 && !acquisitionLoading && (
                <button
                  type="button"
                  disabled
                  className="rounded-md border border-border bg-muted/30 px-3 py-1.5 text-xs text-muted-foreground/50"
                >
                  {t("youthAcademy.fundAcademy")}
                </button>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Roster */}
      {youthPlayers.length > 0 && (
        <Card className="flex min-h-0 flex-1 flex-col">
          <CardHeader className="flex-row items-center justify-between space-y-0 shrink-0">
            <CardTitle className="font-heading text-sm uppercase tracking-widest text-muted-foreground">
              {academyTeam ? t("youthAcademy.academyRosterLinked") : t("youthAcademy.academyNotLinked")}
            </CardTitle>
            {youthPlayers.length > 0 && (
            <div className="relative">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground/50" />
              <input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("youthAcademy.searchPlaceholder", "Buscar...")}
                className="h-7 w-40 rounded-md border border-border bg-muted/30 pl-7 pr-2 text-xs text-foreground placeholder:text-muted-foreground/50"
              />
            </div>
          )}
        </CardHeader>
        <CardContent className="flex-1 p-0">
            <div className="overflow-x-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="w-12" />
                    <TableHead className="cursor-pointer select-none" onClick={() => toggleSort("name")}>
                      <span className="inline-flex items-center gap-1">
                        {t("youthAcademy.player")}
                        {renderSortIcon("name")}
                      </span>
                    </TableHead>
                    <TableHead className="cursor-pointer select-none text-center" onClick={() => toggleSort("pos")}>
                      <span className="inline-flex items-center gap-1 justify-center">
                        {t("youthAcademy.pos")}
                        {renderSortIcon("pos")}
                      </span>
                    </TableHead>
                    <TableHead className="cursor-pointer select-none text-center" onClick={() => toggleSort("age")}>
                      <span className="inline-flex items-center gap-1 justify-center">
                        {t("youthAcademy.age")}
                        {renderSortIcon("age")}
                      </span>
                    </TableHead>
                    <TableHead className="cursor-pointer select-none text-center" onClick={() => toggleSort("ovr")}>
                      <span className="inline-flex items-center gap-1 justify-center">
                        {t("youthAcademy.ovr")}
                        {renderSortIcon("ovr")}
                      </span>
                    </TableHead>
                    <TableHead className="cursor-pointer select-none text-center" onClick={() => toggleSort("potential")}>
                      <span className="inline-flex items-center gap-1 justify-center">
                        {t("youthAcademy.potential")}
                        {renderSortIcon("potential")}
                      </span>
                    </TableHead>
                    <TableHead className="cursor-pointer select-none text-center" onClick={() => toggleSort("condition")}>
                      <span className="inline-flex items-center gap-1 justify-center">
                        {t("youthAcademy.condition")}
                        {renderSortIcon("condition")}
                      </span>
                    </TableHead>
                    <TableHead className="text-center">{t("common.actions")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {filteredPlayers.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={8} className="py-8 text-center text-sm text-muted-foreground">
                        {t("youthAcademy.noStaffMatch", "No players match your search")}
                      </TableCell>
                    </TableRow>
                  ) : (
                    filteredPlayers.map((player) => {
                      const photo = resolvePlayerPhoto(player.id, player.match_name, player.profile_image_url);
                      return (
                        <TableRow
                          key={player.id}
                          onClick={() => onSelectPlayer?.(player.id)}
                          className="cursor-pointer"
                        >
                          <TableCell>
                            {photo ? (
                              <img
                                src={photo}
                                alt={player.match_name}
                                className="size-8 rounded-full object-cover"
                              />
                            ) : (
                              <div className="flex size-8 items-center justify-center rounded-full bg-muted text-xs font-heading font-bold text-muted-foreground">
                                {player.match_name?.charAt(0)?.toUpperCase() ?? "?"}
                              </div>
                            )}
                          </TableCell>
                          <TableCell>
                            <div>
                              <p className="text-sm font-medium text-foreground">{player.match_name || player.id}</p>
                              <p className="text-xs text-muted-foreground">{player.full_name}</p>
                            </div>
                          </TableCell>
                          <TableCell className="text-center">
                            <img
                              src={ROLE_ICON_MAP[player.position] ?? ROLE_ICON_MAP.TOP}
                              alt={player.position}
                              className="mx-auto size-5 object-contain"
                              title={player.position}
                            />
                          </TableCell>
                          <TableCell className="text-center tabular-nums text-foreground">{player.age}</TableCell>
                          <TableCell className="text-center">
                            <span className="font-heading font-bold tabular-nums text-foreground">{player.ovr}</span>
                          </TableCell>
                          <TableCell className="text-center">
                            {player.potential != null ? (
                              <span className="font-heading font-bold tabular-nums text-primary">{player.potential}</span>
                            ) : (
                              <span
                                className="inline-flex items-center gap-1 text-muted-foreground/60"
                                title={t("youthAcademy.potentialHiddenHint")}
                              >
                                <EyeOff className="size-3.5" />
                                <span className="text-[10px] font-heading font-bold">{t("youthAcademy.hidden")}</span>
                              </span>
                            )}
                          </TableCell>
                          <TableCell className="text-center">
                            <div className="flex items-center justify-center gap-1.5">
                              <div className="h-1.5 w-10 overflow-hidden rounded-full bg-muted">
                                <div
                                  className={cn(
                                    "h-full rounded-full transition-all",
                                    (player.condition ?? 0) >= 70
                                      ? "bg-emerald-400"
                                      : (player.condition ?? 0) >= 40
                                        ? "bg-amber-400"
                                        : "bg-red-400",
                                  )}
                                  style={{ width: `${player.condition ?? 0}%` }}
                                />
                              </div>
                              <span className="tabular-nums text-xs text-muted-foreground">
                                {player.condition}%
                              </span>
                            </div>
                          </TableCell>
                          <TableCell className="text-center">
                            <div className="flex items-center justify-center gap-1">
                              <button
                                type="button"
                                disabled={transferListingPlayerId === player.id}
                                onClick={async (e) => {
                                  e.stopPropagation();
                                  try {
                                    setTransferListingPlayerId(player.id);
                                    const updated = await invoke<GameStateData>("toggle_transfer_list", { playerId: player.id });
                                    onGameUpdate?.(updated);
                                  } finally {
                                    setTransferListingPlayerId(null);
                                  }
                                }}
                                className={cn(
                                  "flex size-7 items-center justify-center rounded-md border transition-colors",
                                  player.transfer_listed
                                    ? "border-red-500/30 bg-red-500/10 text-red-400"
                                    : "border-border text-muted-foreground/50 hover:border-red-500/30 hover:text-red-400",
                                )}
                                title={
                                  player.transfer_listed
                                    ? t("youthAcademy.removeFromTransferList", { defaultValue: "Quitar de transferibles" })
                                    : t("youthAcademy.addToTransferList", { defaultValue: "Añadir a transferibles" })
                                }
                              >
                                {transferListingPlayerId === player.id ? (
                                  <Loader2 className="size-3.5 animate-spin" />
                                ) : (
                                  <ShoppingCart className="size-3.5" />
                                )}
                              </button>
                              <button
                                type="button"
                                disabled={promotingPlayerId === player.id}
                                onClick={async (e) => {
                                  e.stopPropagation();
                                  try {
                                    setPromotingPlayerId(player.id);
                                    const updated = await promoteAcademyPlayer(player.id);
                                    onGameUpdate?.(updated);
                                  } finally {
                                    setPromotingPlayerId(null);
                                  }
                                }}
                                className={cn(
                                  "rounded-md border px-2.5 py-1 text-xs font-heading uppercase tracking-wider transition-all disabled:opacity-50 disabled:cursor-not-allowed",
                                  promotingPlayerId === player.id
                                    ? "border-border bg-muted/30 text-muted-foreground"
                                    : "border-primary/30 bg-primary/10 text-primary hover:bg-primary/20",
                                )}
                              >
                                {promotingPlayerId === player.id ? (
                                  <span className="inline-flex items-center gap-1">
                                    <Loader2 className="size-3 animate-spin" />
                                    {t("youthAcademy.promoting")}
                                  </span>
                                ) : (
                                  t("youthAcademy.promote")
                                )}
                              </button>
                            </div>
                          </TableCell>
                        </TableRow>
                      );
                    })
                  )}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
