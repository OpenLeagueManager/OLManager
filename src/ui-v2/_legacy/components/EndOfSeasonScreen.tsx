import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { compareStandingsByLolScore, GameStateData } from "@/store/gameStore";
import { useGameStore } from "@/store/gameStore";
import { Card, CardBody } from "@/ui-v2/_legacy/components/ui";
import PlayoffBracketBoard from "@/ui-v2/_legacy/components/playoffs/PlayoffBracketBoard";
import { Trophy, Award, Star, ArrowRight, Crown } from "lucide-react";

interface EndOfSeasonSummary {
  season: number;
  league_name: string;
  champion_id: string;
  champion_name: string;
  user_position: number;
  user_points: number;
  user_won: number;
  user_lost: number;
  user_maps_won: number;
  user_maps_lost: number;
  golden_boot_player: string;
  golden_boot_goals: number;
  poty_player: string;
  poty_rating: number;
  total_teams: number;
}

type AdvanceToNextSeasonResult = {
  action?: "fired";
  game: GameStateData;
  summary: EndOfSeasonSummary;
};

interface EndOfSeasonScreenProps {
  gameState: GameStateData;
  onGameUpdate: (g: GameStateData) => void;
}

function resolvePlayoffChampionTeamId(gameState: GameStateData): string | null {
  const activeLeague = gameState?.user_competition_id
    ? gameState.leagues.find(l => l.competition_id === gameState.user_competition_id)
    : gameState.leagues[0];
  const playoffFixtures = (activeLeague?.fixtures ?? [])
    .filter((fixture) => fixture.match_type === "Playoffs" && fixture.status === "Completed" && fixture.result)
    .sort((left, right) => right.matchday - left.matchday);

  const final = playoffFixtures[0];
  if (!final || !final.result) return null;
  const homeWins = typeof final.result.home_wins === "number" ? final.result.home_wins : 0;
  const awayWins = typeof final.result.away_wins === "number" ? final.result.away_wins : 0;
  if (homeWins === awayWins) return null;
  return homeWins > awayWins ? final.home_team_id : final.away_team_id;
}

export default function EndOfSeasonScreen({ gameState, onGameUpdate }: EndOfSeasonScreenProps) {
  const { t } = useTranslation();
  const setShowFiredModal = useGameStore((s) => s.setShowFiredModal);
  const [loading, setLoading] = useState(false);
  const [summary, setSummary] = useState<EndOfSeasonSummary | null>(null);
  const [step, setStep] = useState<"review" | "done">("review");

  const league = gameState?.user_competition_id
    ? gameState.leagues.find(l => l.competition_id === gameState.user_competition_id)
        ?? gameState.leagues[0]
    : gameState.leagues[0];
  const userTeamId = gameState.manager.team_id;
  const userTeam = gameState.teams.find(t => t.id === userTeamId);

  // Compute standings for display
  const standings = league
    ? [...league.standings].sort(compareStandingsByLolScore)
    : [];

  const userStandingIdx = standings.findIndex(s => s.team_id === userTeamId);
  const userStanding = standings[userStandingIdx];
  const userPosition = userStandingIdx + 1;
  const standingsChampionId = standings[0]?.team_id ?? null;
  const playoffChampionId = resolvePlayoffChampionTeamId(gameState);
  const championTeamId = playoffChampionId ?? standingsChampionId;
  const hasPlayoffs = (league?.fixtures ?? []).some((fixture) => fixture.match_type === "Playoffs");
  const championName = gameState.teams.find(t => t.id === championTeamId)?.name || "";
  const isChampion = championTeamId === userTeamId;

  const handleAdvance = async () => {
    if (loading) return;
    setLoading(true);
    try {
      const result = await invoke<AdvanceToNextSeasonResult>("advance_to_next_season");
      if (result.action === "fired") {
        onGameUpdate(result.game);
        setShowFiredModal(true);
        return;
      }
      setSummary(result.summary);
      onGameUpdate(result.game);
      setStep("done");
    } catch (err) {
      console.error("Failed to advance season:", err);
    } finally {
      setLoading(false);
    }
  };

  const posLabel = (pos: number) => {
    if (pos === 1) return t("common.place.1");
    if (pos === 2) return t("common.place.2");
    if (pos === 3) return t("common.place.3");
    return t("common.place.other", { n: pos });
  };

  return (
    <div className="w-[92%] max-w-[2000px] mx-auto py-8 px-4">
      {step === "review" && (
        <>
          {/* Hero */}
          <div className="text-center mb-8">
            <div className={`w-20 h-20 mx-auto rounded-2xl flex items-center justify-center mb-4 ${isChampion
                ? "bg-gradient-to-br from-accent-400 to-accent-600 shadow-lg shadow-accent-500/30"
                : "bg-gradient-to-br from-navy-700 to-navy-800"
              }`}>
              {isChampion ? <Crown className="w-10 h-10 text-white" /> : <Trophy className="w-10 h-10 text-gray-300" />}
            </div>
            <h1 className="text-3xl font-heading font-bold text-gray-900 dark:text-gray-100 uppercase tracking-wide">
              {t('endOfSeason.seasonComplete')}
            </h1>
            <p className="text-lg text-gray-500 dark:text-gray-400 mt-1">
              {league?.name} — Season {league?.season}
            </p>
            {isChampion && (
              <p className="text-xl font-heading font-bold text-accent-500 mt-2 uppercase tracking-wider animate-pulse">
                {t('endOfSeason.champions')}
              </p>
            )}
          </div>

          {/* User team summary */}
          <Card accent={isChampion ? "accent" : "primary"} className="mb-6">
            <CardBody>
              <div className="text-center">
                <p className="text-xs font-heading font-bold uppercase tracking-widest text-gray-400 dark:text-gray-500 mb-1">
                  {userTeam?.name}
                </p>
                <p className="text-xs font-heading font-bold uppercase tracking-widest text-primary-500 mb-4">
                  {t("endOfSeason.regularPhaseSummary")}
                </p>
                <div className="flex items-center justify-center gap-6 mb-4">
                  <div>
                    <p className="text-4xl font-heading font-bold text-gray-900 dark:text-gray-100">{posLabel(userPosition)}</p>
                    <p className="text-xs text-gray-400 dark:text-gray-500 font-heading uppercase">{t('endOfSeason.finalPosition')}</p>
                  </div>
                  <div className="w-px h-12 bg-gray-200 dark:bg-navy-600" />
                  <div>
                    <p className="text-4xl font-heading font-bold text-green-500">{userStanding?.won || 0}</p>
                    <p className="text-xs text-gray-400 dark:text-gray-500 font-heading uppercase">{t('common.won')}</p>
                  </div>
                  <div className="w-px h-12 bg-gray-200 dark:bg-navy-600" />
                  <div>
                    <p className="text-4xl font-heading font-bold text-red-500">{userStanding?.lost || 0}</p>
                    <p className="text-xs text-gray-400 dark:text-gray-500 font-heading uppercase">{t('common.lost')}</p>
                  </div>
                </div>
              </div>
            </CardBody>
          </Card>

          {hasPlayoffs ? (
            <div className="mb-6">
              <PlayoffBracketBoard
                league={league!}
                teams={gameState.teams}
                title={`${t("schedule.playoffs")} · Bracket`}
              />
            </div>
          ) : (
            <Card className="mb-6">
              <CardBody>
                <h3 className="font-heading font-bold text-sm uppercase tracking-wider text-gray-500 dark:text-gray-400 mb-3 flex items-center gap-2">
                  <Trophy className="w-4 h-4 text-accent-500" /> {t('endOfSeason.regularPhaseStandings')}
                </h3>
                <div className="divide-y divide-gray-100 dark:divide-navy-600">
                  {standings.slice(0, 5).map((entry, idx) => {
                    const teamName = gameState.teams.find(t => t.id === entry.team_id)?.name || "";
                    const isUser = entry.team_id === userTeamId;
                    return (
                      <div key={entry.team_id} className={`flex items-center py-2.5 gap-3 ${isUser ? "bg-primary-50/50 dark:bg-primary-500/5 -mx-2 px-2 rounded-lg" : ""}`}>
                        <span className={`font-heading font-bold text-sm w-6 text-center ${idx === 0 ? "text-accent-500" : "text-gray-400"}`}>{idx + 1}</span>
                        <span className={`flex-1 text-sm font-semibold ${isUser ? "text-primary-600 dark:text-primary-400" : "text-gray-800 dark:text-gray-200"}`}>{teamName}</span>
                        <span className="text-xs text-green-500 font-heading font-bold tabular-nums w-12 text-center">{entry.won}{t('common.won')}</span>
                        <span className="text-xs text-red-500 font-heading font-bold tabular-nums w-12 text-center">{entry.lost}{t('common.lost')}</span>
                      </div>
                    );
                  })}
                </div>
              </CardBody>
            </Card>
          )}

          {/* Champion + awards */}
          {!isChampion && (
            <Card className="mb-6">
              <CardBody>
                <div className="flex items-center gap-3">
                  <Crown className="w-6 h-6 text-accent-500" />
                  <div>
                    <p className="text-sm font-heading font-bold text-gray-800 dark:text-gray-200 uppercase tracking-wider">{t('endOfSeason.leagueChampions')}</p>
                    <p className="text-lg font-heading font-bold text-accent-500">{championName}</p>
                  </div>
                </div>
              </CardBody>
            </Card>
          )}

          {/* Action */}
          <div className="text-center">
            <button
              onClick={handleAdvance}
              disabled={loading}
              className="px-8 py-4 bg-primary-500 text-white rounded-xl font-heading font-bold text-lg uppercase tracking-wider hover:bg-primary-600 transition-all shadow-lg shadow-primary-500/20 hover:shadow-xl hover:shadow-primary-500/30 disabled:opacity-50 flex items-center gap-3 mx-auto"
            >
              {loading ? t('endOfSeason.processing') : t('endOfSeason.startNextSeason')}
              <ArrowRight className="w-5 h-5" />
            </button>
            <p className="text-xs text-gray-400 dark:text-gray-500 mt-3">
              {t('endOfSeason.statsArchived')}
            </p>
          </div>
        </>
      )}

      {step === "done" && summary && (
        <div className="text-center">
          <div className="w-20 h-20 mx-auto rounded-2xl bg-gradient-to-br from-primary-500 to-primary-600 flex items-center justify-center mb-4 shadow-lg shadow-primary-500/30">
            <Star className="w-10 h-10 text-white" />
          </div>
          <h1 className="text-3xl font-heading font-bold text-gray-900 dark:text-gray-100 uppercase tracking-wide mb-2">
            {t('endOfSeason.newSeason', { n: summary.season + 1 })}
          </h1>
          <p className="text-gray-500 dark:text-gray-400 mb-8">
            {t('endOfSeason.newScheduleReleased')}
          </p>

          {/* Season summary awards */}
          {(summary.golden_boot_player || summary.poty_player) && (
            <div className="flex gap-4 justify-center mb-8 flex-wrap">
              {summary.golden_boot_player && (
                <Card className="w-64">
                  <CardBody>
                    <div className="flex items-center gap-2 mb-1">
                      <Award className="w-4 h-4 text-accent-500" />
                      <span className="text-xs font-heading font-bold uppercase tracking-wider text-gray-400">{t('endOfSeason.goldenBoot')}</span>
                    </div>
                    <p className="text-sm font-heading font-bold text-gray-800 dark:text-gray-200">{summary.golden_boot_player}</p>
                    <p className="text-xs text-gray-500">{t('endOfSeason.nGoals', { count: summary.golden_boot_goals })}</p>
                  </CardBody>
                </Card>
              )}
              {summary.poty_player && (
                <Card className="w-64">
                  <CardBody>
                    <div className="flex items-center gap-2 mb-1">
                      <Star className="w-4 h-4 text-purple-500" />
                      <span className="text-xs font-heading font-bold uppercase tracking-wider text-gray-400">{t('endOfSeason.playerOfYear')}</span>
                    </div>
                    <p className="text-sm font-heading font-bold text-gray-800 dark:text-gray-200">{summary.poty_player}</p>
                    <p className="text-xs text-gray-500">{t('endOfSeason.avgRating', { rating: summary.poty_rating.toFixed(2) })}</p>
                  </CardBody>
                </Card>
              )}
            </div>
          )}

          <button
            onClick={() => {
              // Game state is already updated via onGameUpdate, just force re-render
              // by calling onGameUpdate again with the current state
              if (gameState) onGameUpdate(gameState);
            }}
            className="px-8 py-3 bg-primary-500 text-white rounded-xl font-heading font-bold uppercase tracking-wider hover:bg-primary-600 transition-all shadow-lg shadow-primary-500/20"
          >
            {t('endOfSeason.continueDashboard')}
          </button>
        </div>
      )}
    </div>
  );
}
