export interface TeamColors {
  primary: string;
  secondary: string;
}

export interface FacilitiesData {
  main_hub_level?: number;
  training: number;
  medical: number;
  scouting: number;
  scrims_room_level?: number;
  analysis_room_level?: number;
  bootcamp_area_level?: number;
  recovery_suite_level?: number;
  content_studio_level?: number;
  scouting_lab_level?: number;
}

export interface SponsorshipData {
  sponsor_name: string;
  base_value: number;
  remaining_months: number;
  bonus_criteria: unknown[];
}

export type FinancialTransactionKind =
  | "Salary"
  | "StaffWage"
  | "FacilityUpkeep"
  | "FacilityUpgrade"
  | "TransferPurchase"
  | "TransferSale"
  | "ReleasePenalty"
  | "AcademyAcquisition"
  | "Sponsorship"
  | "MatchdayRevenue"
  | "PrizeMoney"
  | "BudgetRefresh"
  | "Other";

export interface FinancialTransactionData {
  id?: string;
  date: string;
  description: string;
  amount: number;
  kind: FinancialTransactionKind;
  balance_before?: number;
  balance_after?: number;
  source?: string;
  source_id?: string | null;
  correlation_id?: string | null;
}

export interface TeamSeasonRecord {
  season: number;
  league_position: number;
  played: number;
  won: number;
  drawn: number;
  lost: number;
  kills_for: number;
  kills_against: number;
  points?: number;
}

export interface TeamRolesData {
  captain: string | null;
  shotcaller: string | null;
}

export type TeamKind = "Main" | "Academy";

export type AcademyLifecycle = "Planned" | "Active";

export type ErlAssignmentRule = "Domestic" | "Fallback";

export interface ErlAssignmentData {
  erl_league_id: string;
  country_rule: ErlAssignmentRule;
  fallback_reason: string | null;
  reputation: number;
  creation_cost: number;
  created_at: string;
}

export interface AcademyMetadataData {
  lifecycle: AcademyLifecycle;
  erl_assignment: ErlAssignmentData;
  source_identity?: AcademySourceIdentityData;
  branding?: {
    current_name: string;
    current_short_name: string;
    current_logo_url: string | null;
  };
  acquisition?: {
    source_team_id: string;
    original_name: string;
    original_short_name: string;
    original_logo_url: string | null;
    acquisition_cost: number;
    acquired_at: string;
  };
}

export interface AcademySourceIdentityData {
  source_team_id: string;
  original_name: string;
  original_short_name: string;
  original_logo_url: string | null;
}

export interface AcademyAcquisitionOptionData {
  source_team_id: string;
  source_team_name: string;
  source_team_short_name: string;
  source_team_logo_url: string | null;
  erl_league_id: string;
  league_name: string;
  country: string;
  region: string;
  assignment_rule: ErlAssignmentRule;
  fallback_reason: string | null;
  reputation: number;
  development_level: number;
  acquisition_cost: number;
  rebrand_allowed: boolean;
  source_identity: AcademySourceIdentityData;
}

export interface AcademyAcquisitionOptionsResponseData {
  parent_team_id: string;
  acquisition_allowed: boolean;
  blocked_reason: string | null;
  options: AcademyAcquisitionOptionData[];
}

export interface AcquireAcademyTeamRequestData {
  parent_team_id: string;
  source_team_id: string;
  custom_name?: string | null;
  custom_short_name?: string | null;
  custom_logo_url?: string | null;
}

export type AcademyCreationOptionData = AcademyAcquisitionOptionData;

export type AcademyCreationOptionsResponseData = AcademyAcquisitionOptionsResponseData;

export interface CreateAcademyRequestData extends AcquireAcademyTeamRequestData {
  erl_league_id: string;
}

export interface LolTacticsData {
  strong_side: "Top" | "Mid" | "Bot";
  game_timing: "Unknown" | "Early" | "Mid" | "Late";
  jungle_style: "Ganker" | "Invader" | "Farmer" | "Enabler";
  jungle_pathing: "TopToBot" | "BotToTop";
  fight_plan: "FrontToBack" | "Pick" | "Dive" | "Siege";
  support_roaming: "Lane" | "RoamMid" | "RoamTop";
}

export interface TeamData {
  id: string;
  name: string;
  short_name: string;
  country: string;
  city: string;
  stadium_name?: string;
  stadium_capacity?: number;
  finance: number;
  manager_id: string | null;
  reputation: number;
  wage_budget: number;
  transfer_budget: number;
  season_income: number;
  season_expenses: number;
  financial_ledger?: FinancialTransactionData[];
  installations?: FacilitiesData;
  draft_strategy: string;
  lol_tactics?: LolTacticsData;
  training_focus: string;
  training_intensity: string;
  training_schedule: string;
  weekly_scrim_opponent_ids?: string[];
  weekly_scrim_plan_team_ids?: string[][];
  scrim_weekly_objective?: ScrimFocus | null;
  logo_url?: string | null;
  /** Competition/league this team belongs to (multi-league system). */
  competition_id?: string | null;
  scrim_weekly_slots?: number;
  scrim_reputation?: number;
  scrim_weekly_cancellations?: number;
  scrim_loss_streak?: number;
  scrim_weekly_played?: number;
  scrim_weekly_wins?: number;
  scrim_weekly_losses?: number;
  scrim_slot_results?: ScrimSlotResultData[];
  scrim_reports?: ScrimReportData[];
  founded_year: number;
  colors: TeamColors;
  facilities?: FacilitiesData;
  sponsorship?: SponsorshipData | null;
  /** Preferred LoL terminology. Serialized by current saves/API responses. */
  active_lineup_ids?: string[];
  /** @deprecated Compatibility for older saves/API payloads. Use active_lineup_ids. */
  starting_xi_ids?: string[];
  team_roles?: TeamRolesData;
  form: string[];
  history: TeamSeasonRecord[];
  team_kind?: TeamKind;
  parent_team_id?: string | null;
  academy_team_id?: string | null;
  academy?: AcademyMetadataData | null;
}

export function resolveActiveLineupIds(team: Pick<TeamData, "active_lineup_ids" | "starting_xi_ids">): string[] {
  return team.active_lineup_ids ?? team.starting_xi_ids ?? [];
}

export type MatchOutcome = "Win" | "Loss";

export type TeamSide = "Blue" | "Red";

export type LegacyFootballRole = "Goalkeeper" | "Defender" | "Midfielder" | "Forward" | "UNKNOWN";
export type LolRole = "TOP" | "JUNGLE" | "MID" | "ADC" | "SUPPORT" | LegacyFootballRole;

export type MatchEndReason = "NexusDestroyed" | "Surrender";

type LegacyCompatibilityValue = unknown;

export interface PlayerSeasonStats {
  games_played?: number;
  wins?: number;
  losses?: number;
  kills?: number;
  deaths?: number;
  assists: number;
  cs?: number;
  gold_earned?: number;
  damage_to_champions?: number;
  vision_score?: number;
  wards_placed?: number;
  wards_cleared?: number;
  time_played_seconds?: number;
  /**
   * Temporary compatibility layer for Block 1.
   * Allows legacy callers/tests to keep compiling while the LoL-first contract
   * is propagated through the rest of the app in later migration blocks.
   */
  [legacyField: string]: LegacyCompatibilityValue;
}

export interface PlayerMatchStatsRecord {
  fixture_id: string;
  season?: number;
  date: string;
  competition: string;
  matchday: number;
  player_id?: string;
  team_id?: string;
  opponent_team_id: string;
  side?: TeamSide;
  result?: MatchOutcome;
  role?: LolRole;
  champion_id?: string | null;
  game_duration_seconds?: number;
  kills?: number;
  deaths?: number;
  assists: number;
  cs?: number;
  gold_earned?: number;
  damage_to_champions?: number;
  vision_score?: number;
  wards_placed?: number;
  wards_cleared?: number;
  rating: number;
  opponent_name?: string;
  /** Temporary compatibility layer during the domain migration. */
  [legacyField: string]: LegacyCompatibilityValue;
}

export interface PlayerMatchHistoryEntryData {
  fixtureId: string;
  date: string;
  competition: string;
  matchday: number;
  opponentTeamId: string;
  opponentName: string;
  side: TeamSide;
  result: MatchOutcome;
  role: LolRole;
  championId?: string | null;
  gameDurationSeconds: number;
  kills: number;
  deaths: number;
  assists: number;
  cs: number;
  goldEarned: number;
  damageToChampions: number;
  visionScore: number;
  wardsPlaced: number;
}

export interface TeamMatchStatsRecord {
  fixture_id: string;
  season?: number;
  matchday: number;
  date: string;
  competition: string;
  team_id: string;
  opponent_team_id: string;
  side?: TeamSide;
  result?: MatchOutcome;
  game_duration_seconds?: number;
  kills?: number;
  deaths?: number;
  gold_earned?: number;
  damage_to_champions?: number;
  towers_destroyed?: number;
  inhibitors_destroyed?: number;
  dragons_taken?: number;
  barons_taken?: number;
  heralds_taken?: number;
  void_grubs_taken?: number;
  /** Temporary compatibility layer during the domain migration. */
  [legacyField: string]: LegacyCompatibilityValue;
}

export interface CareerEntry {
  season: number;
  team_id: string;
  team_name: string;
  appearances: number;
  kills: number;
  assists: number;
  avg_rating: number;
}

export type PlayerAttributes = Record<string, number>;

export interface PlayerData {
  id: string;
  match_name: string;
  full_name: string;
  date_of_birth: string;
  nationality: string;
  nationality_code?: string;
  competitive_region?: string;
  birth_country?: string | null;
  profile_image_url?: string | null;
  position: LolRole;
  natural_position: LolRole;
  alternate_positions: LolRole[];
  training_focus: string | null;
  attributes: PlayerAttributes;
  condition: number;
  fitness?: number;
  morale: number;
  team_id: string | null;
  contract_end: string | null;
  wage: number;
  market_value: number;
  stats: PlayerSeasonStats;
  career: CareerEntry[];
  transfer_listed: boolean;
  loan_listed: boolean;
  transfer_offers: TransferOfferData[];
  traits: string[];
  potential_base?: number;
  potential_revealed?: number | null;
  potential_research_started_on?: string | null;
  potential_research_eta_days?: number | null;
  champion_training_target?: string | null;
  champion_training_targets?: string[];
  lol_ovr?: number;
  soloq_lp?: number;
}

export interface ScrimSlotResultData {
  week_key: string;
  slot_index: number;
  weekday: number;
  opponent_team_id: string;
  won: boolean;
  simulated_on: string;
}

export type ScrimStatus = "Pending" | "Accepted" | "Rejected" | "Cancelled" | "Played";
export type ScrimFocus = "DraftPrep" | "ChampionPool" | "EarlyGame" | "Teamfighting" | "Macro" | "Mental";
export type ScrimIssue = "DraftGap" | "LanePressure" | "ObjectiveSetup" | "TeamfightExecution" | "ChampionComfort" | "Tilt";
export type PostScrimDecision = "ContinuePlan" | "VodReview" | "MentalReset" | "TargetedDrills" | "PushThrough" | "DayOff";

export interface ScrimChampionPickData {
  player_id: string;
  champion_id: string;
  role: string;
}

export interface ScrimReportData {
  date: string;
  week_key: string;
  slot_index: number;
  weekday: number;
  team_id: string;
  opponent_team_id: string;
  status: ScrimStatus;
  won: boolean | null;
  focus: ScrimFocus;
  issue: ScrimIssue | null;
  severity: number;
  quality: number;
  player_champion_picks: ScrimChampionPickData[];
  post_decision: PostScrimDecision | null;
  created_on: string;
}

export interface ChampionMasteryEntryData {
  player_id: string;
  champion_id: string;
  mastery: number;
  last_active_on: string;
}

export interface ChampionMetaEntryData {
  champion_id: string;
  role: string;
  tier: "S" | "A" | "B" | "C" | "D" | string;
}

export interface ChampionPatchNoteData {
  champion_id: string;
  role: string;
  change: "Buff" | "Nerf";
}

export interface ChampionPatchStateData {
  current_patch: number;
  current_patch_label?: string;
  patch_year?: number;
  patch_index_in_year?: number;
  last_patch_date: string | null;
  /** IPC contains only entries whose champion has already been discovered. */
  hidden_meta: ChampionMetaEntryData[];
  patch_notes: ChampionPatchNoteData[];
  discovered_champion_ids: string[];
  rng_seed?: number;
}

/**
 * Champion data from the backend - represents a League of Legends champion
 */
export interface ChampionData {
  id: number;
  name: string;
  champion_key: string;
  roles_json: string;
  counterpicks_json: string | null;
  synergies_json: string | null;
  image_tile_url: string | null;
  image_splash_url: string | null;
  meta_tier: string;
}

export interface TransferOfferData {
  id: string;
  from_team_id: string;
  destination_team_id?: string | null;
  fee: number;
  wage_offered: number;
  last_manager_fee: number | null;
  negotiation_round: number;
  suggested_counter_fee: number | null;
  suggested_counter_wage?: number | null;
  suggested_counter_years?: number | null;
  wage_negotiation_status?: "NotStarted" | "Pending" | "Agreed" | "Rejected";
  contract_years_offered?: number;
  wage_negotiation_round?: number;
  players_included?: { player_id: string }[];
  status: "Pending" | "Accepted" | "Rejected" | "Withdrawn";
  date: string;
}

export interface StaffData {
  id: string;
  /** Esports handle (e.g. "Zetz"). Always sent by the backend; optional here so
   *  test fixtures that predate the field still satisfy the type. */
  nickname?: string;
  first_name: string;
  last_name: string;
  date_of_birth: string;
  nationality: string;
  profile_image_url?: string | null;
  role: "AssistantManager" | "Coach" | "Scout" | "Physio";
  attributes: {
    coaching: number;
    judging_ability: number;
    judging_potential: number;
    physiotherapy: number;
  };
  team_id: string | null;
  specialization: string | null;
  wage: number;
  contract_end: string | null;
}

export interface MessageAction {
  id: string;
  label: string;
  action_type:
  | "Acknowledge"
  | "Dismiss"
  | { NavigateTo: { route: string } }
  | { ChooseOption: { options: MessageActionOption[] } };
  resolved: boolean;
  label_key?: string;
}

export interface MessageActionOption {
  id: string;
  label: string;
  description: string;
  label_key?: string;
  description_key?: string;
}

export interface ScoutReportData {
  player_id: string;
  player_name: string;
  position: string;
  nationality: string;
  dob: string;
  team_name: string | null;
  pace: number | null;
  shooting: number | null;
  passing: number | null;
  dribbling: number | null;
  defending: number | null;
  physical: number | null;
  mechanics?: number | null;
  laning?: number | null;
  teamfighting?: number | null;
  macro?: number | null;
  champion_pool?: number | null;
  discipline?: number | null;
  condition: number | null;
  morale: number | null;
  avg_rating: number | null;
  rating_key: string;
  potential_key: string;
  confidence_key: string;
}

export interface DelegatedRenewalCaseMessageData {
  player_id: string;
  player_name: string;
  status: string;
  agreed_wage?: number | null;
  agreed_years?: number | null;
  note_key?: string;
  note_params?: Record<string, string>;
}

export interface DelegatedRenewalReportMessageData {
  success_count: number;
  failure_count: number;
  stalled_count: number;
  cases: DelegatedRenewalCaseMessageData[];
}

export interface PlayerSelectionOptions {
  openRenewal?: boolean;
}

export interface MessageContext {
  team_id: string | null;
  player_id: string | null;
  fixture_id: string | null;
  match_result: MatchResult | null;
  scout_report?: ScoutReportData;
  delegated_renewal_report?: DelegatedRenewalReportMessageData;
}

export interface MessageData {
  id: string;
  subject: string;
  body: string;
  sender: string;
  sender_role: string;
  date: string;
  read: boolean;
  category: string;
  priority: string;
  actions: MessageAction[];
  context: MessageContext;
  subject_key?: string;
  body_key?: string;
  sender_key?: string;
    sender_role_key?: string;
    i18n_params?: Record<string, string>;
    sender_icon?: string;
  }

  export interface ManagerCareerStats {
  matches_managed: number;
  /** @deprecated Legacy test fixture alias. Use matches_managed. */
  matches?: number;
  wins: number;
  losses: number;
  trophies: number;
  best_finish: number | null;
}

export interface ManagerCareerEntry {
  team_id: string;
  team_name: string;
  start_date: string;
  end_date: string | null;
  matches: number;
  wins: number;
  losses: number;
  best_league_position: number | null;
}

export interface FixtureData {
  id: string;
  matchday: number;
  date: string;
  home_team_id: string;
  away_team_id: string;
  match_type: "League" | "Friendly" | "PreseasonTournament" | "Playoffs";
  best_of?: number;
  status: "Scheduled" | "InProgress" | "Completed";
  result: MatchResult | null;
}

export interface MatchResult {
  home_wins?: number;
  away_wins?: number;
  ended_by?: MatchEndReason;
  game_duration_seconds?: number;
  report?: CompactMatchReportData | null;
  /**
   * Temporary compatibility layer for legacy scoreline consumers.
   * These keys are intentionally not modeled explicitly anymore.
   */
  [legacyField: string]: LegacyCompatibilityValue;
}

export interface CompactMatchEventData {
  minute: number;
  event_type: string;
  side: "Home" | "Away";
  player_id: string | null;
  secondary_player_id: string | null;
}

export interface CompactTeamMatchStatsData {
  possession_pct: number;
  kills: number;
  deaths: number;
  gold_earned: number;
  damage_dealt: number;
  objectives: number;
}

export interface CompactMatchReportData {
  total_minutes: number;
  home_stats: CompactTeamMatchStatsData;
  away_stats: CompactTeamMatchStatsData;
  events: CompactMatchEventData[];
}

export interface StandingData {
  team_id: string;
  played: number;
  won: number;
  lost: number;
  maps_won?: number;
  maps_lost?: number;
  /** @deprecated Compatibility alias. Use maps_won. */
  kills_for?: number;
  /** @deprecated Compatibility alias. Use maps_lost. */
  kills_against?: number;
  /** @deprecated Compatibility alias while old fixture tests are migrated. Use maps_won. */
  goals_for?: number;
  /** @deprecated Compatibility alias while old fixture tests are migrated. Use maps_lost. */
  goals_against?: number;
  points: number;
}

export function getStandingMapsWon(standing: StandingData): number {
  return standing.maps_won ?? standing.kills_for ?? standing.goals_for ?? 0;
}

export function getStandingMapsLost(standing: StandingData): number {
  return standing.maps_lost ?? standing.kills_against ?? standing.goals_against ?? 0;
}

export function getStandingKillsFor(standing: StandingData): number {
  return getStandingMapsWon(standing);
}

export function getStandingKillsAgainst(standing: StandingData): number {
  return getStandingMapsLost(standing);
}

export function getStandingKillDiff(standing: StandingData): number {
  return getStandingMapsWon(standing) - getStandingMapsLost(standing);
}

// ---------------------------------------------------------------------------
// League/Competition selection types (multi-league system)
// ---------------------------------------------------------------------------

export interface CompetitionSummary {
  id: string;
  name: string;
  region: string;
  logo: string | null;
  tier: number;
  legacy: boolean;
  active: boolean;
  team_count: number;
  teams: TeamSummary[];
}

export interface TeamSummary {
  id: string;
  name: string;
  short_name: string;
  logo_url: string | null;
  country: string;
  city?: string | null;
  finance?: number | null;
  reputation?: number | null;
  colors?: TeamColors | null;
  ovr: number | null;
  player_count?: number | null;
}

export interface LeagueSelectionData {
  competitions: CompetitionSummary[];
}

// ---------------------------------------------------------------------------

export function compareStandingsByLolScore(left: StandingData, right: StandingData): number {
  return (
    right.points - left.points ||
    getStandingKillDiff(right) - getStandingKillDiff(left) ||
    getStandingMapsWon(right) - getStandingMapsWon(left)
  );
}

export interface LeagueData {
  id: string;
  name: string;
  season: number;
  fixtures: FixtureData[];
  standings: StandingData[];
  competition_id?: string | null;
  logo?: string | null;
  league_kind?: "Main" | "Academy";
  tier?: number;
  active?: boolean;
}

export type SeasonPhase = "Preseason" | "InSeason" | "PostSeason";

export type TransferWindowStatus = "Closed" | "Open" | "DeadlineDay";
export type DayPhase = "Morning" | "ScrimBlock" | "ReviewBlock" | "TrainingBlock" | "Evening";

export interface TransferWindowContextData {
  status: TransferWindowStatus;
  opens_on: string | null;
  closes_on: string | null;
  days_until_opens: number | null;
  days_remaining: number | null;
}

export interface SeasonContextData {
  phase: SeasonPhase;
  season_start: string | null;
  season_end: string | null;
  days_until_season_start: number | null;
  transfer_window: TransferWindowContextData;
}

export interface NewsMatchScore {
  home_team_id: string;
  away_team_id: string;
  home_goals: number;
  away_goals: number;
}

export interface NewsArticle {
  id: string;
  headline: string;
  body: string;
  source: string;
  date: string;
  category: string;
  team_ids: string[];
  player_ids: string[];
  match_score: NewsMatchScore | null;
  read: boolean;
  headline_key?: string;
  body_key?: string;
  source_key?: string;
  i18n_params?: Record<string, string>;
}

export type SocialAuthorType =
  | "Team"
  | "Player"
  | "Fan"
  | "Analyst"
  | "Journalist"
  | "MemeAccount"
  | "Manager";

export type SocialSentiment =
  | "Hype"
  | "Calm"
  | "Worried"
  | "Angry"
  | "Meltdown"
  | "Copium";

export type SocialPostCategory =
  | "MatchResult"
  | "Banter"
  | "PlayerReaction"
  | "FanOpinion"
  | "MediaTake"
  | "Meme"
  | "ManagerPost";

export interface SocialPostData {
  id: string;
  date: string;
  author_name: string;
  author_handle: string;
  author_type: SocialAuthorType;
  body: string;
  likes: number;
  reposts: number;
  replies: number;
  sentiment: SocialSentiment;
  category: SocialPostCategory;
  tags: string[];
  team_ids: string[];
  player_ids: string[];
  fixture_id: string | null;
  media_url?: string | null;
  read: boolean;
}

export interface SocialAccountData {
  id: string;
  language: string;
  display_name: string;
  handle: string;
  author_type: SocialAuthorType;
  profile_image_url?: string | null;
  favorite_team_ids: string[];
  active: boolean;
}

export interface SocialTemplateData {
  id: string;
  language: string;
  slot: string;
  author_id?: string | null;
  conditions_json: string;
  variants: string[];
  tags: string[];
  weight: number;
  active: boolean;
}

export interface BoardObjective {
  id: string;
  description: string;
  target: number;
  objective_type: string;
  met: boolean;
}

export interface ScoutingAssignment {
  id: string;
  scout_id: string;
  player_id: string;
  days_remaining: number;
}

export interface GameStateData {
  clock: {
    current_date: string;
    start_date: string;
  };
  day_phase?: DayPhase;
  manager: {
    id: string;
    nickname?: string | null;
    first_name: string;
    last_name: string;
    date_of_birth: string;
    nationality: string;
    avatar_path?: string | null;
    reputation: number;
    satisfaction: number;
    fan_approval: number;
    team_id: string | null;
    career_stats: ManagerCareerStats;
    career_history: ManagerCareerEntry[];
  };
  teams: TeamData[];
  players: PlayerData[];
  staff: StaffData[];
  messages: MessageData[];
  news: NewsArticle[];
  social_posts?: SocialPostData[];
  social_accounts?: SocialAccountData[];
  social_templates?: SocialTemplateData[];
  /** Multi-league support. The first element is the player's active league. */
  leagues: LeagueData[];
  user_competition_id?: string | null;
  academy_league?: LeagueData | null;
  scouting_assignments: ScoutingAssignment[];
  board_objectives: BoardObjective[];
  season_context?: SeasonContextData;
  champion_masteries?: ChampionMasteryEntryData[];
  champion_patch?: ChampionPatchStateData;
  champions?: ChampionData[];
}
