use crate::domain::stats::StatsState;
use crate::game::Game;
use crate::live_match_manager::LiveMatchSession;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

/// Set during `assemble_world_from_modular_data` to the app's resource data directory.
/// Used by static/fn functions in `game_setup` that can't receive `app_handle` directly.
pub static RESOURCE_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Holds all mutable session state under a single lock to prevent deadlocks
/// and race conditions between independent mutexes.
/// Individual fields remain `Option` so they can be set independently
/// (e.g., save_id can exist without a loaded game).
pub struct Session {
    pub game: Option<Game>,
    pub stats: StatsState,
    pub live_match: Option<LiveMatchSession>,
    pub save_id: Option<String>,
}

/// Single-lock state manager. All fields are grouped under one
/// `Mutex<Session>` to prevent deadlocks that could occur when two
/// commands acquire four independent mutexes in different order.
pub struct StateManager {
    session: Mutex<Session>,
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(Session {
                game: None,
                stats: StatsState::default(),
                live_match: None,
                save_id: None,
            }),
        }
    }

    /// Execute a read-only operation on the session.
    pub fn with_session<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Session) -> R,
    {
        let lock = self.session.lock().unwrap();
        f(&lock)
    }

    /// Execute a read-write operation on the session.
    pub fn with_session_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Session) -> R,
    {
        let mut lock = self.session.lock().unwrap();
        f(&mut lock)
    }

    // ── Game ────────────────────────────────────────────────

    pub fn set_game(&self, game: Game) {
        let mut lock = self.session.lock().unwrap();
        lock.game = Some(game);
    }

    pub fn get_game<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&Game) -> R,
    {
        let lock = self.session.lock().unwrap();
        lock.game.as_ref().map(f)
    }

    pub fn clear_game(&self) {
        let mut lock = self.session.lock().unwrap();
        lock.game = None;
        lock.stats = StatsState::default();
    }

    // ── Stats ───────────────────────────────────────────────

    pub fn set_stats_state(&self, stats: StatsState) {
        let mut lock = self.session.lock().unwrap();
        lock.stats = stats;
    }

    pub fn get_stats_state<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&StatsState) -> R,
    {
        let lock = self.session.lock().unwrap();
        Some(f(&lock.stats))
    }

    pub fn with_stats_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut StatsState) -> R,
    {
        let mut lock = self.session.lock().unwrap();
        f(&mut lock.stats)
    }

    pub fn clear_stats_state(&self) {
        let mut lock = self.session.lock().unwrap();
        lock.stats = StatsState::default();
    }

    pub fn append_stats_state(&self, stats: StatsState) {
        let mut lock = self.session.lock().unwrap();
        lock.stats.append(stats);
    }

    // ── Save ID ─────────────────────────────────────────────

    pub fn set_save_id(&self, id: String) {
        let mut lock = self.session.lock().unwrap();
        lock.save_id = Some(id);
    }

    pub fn get_save_id(&self) -> Option<String> {
        let lock = self.session.lock().unwrap();
        lock.save_id.clone()
    }

    pub fn clear_save_id(&self) {
        let mut lock = self.session.lock().unwrap();
        lock.save_id = None;
    }

    // ── Live Match ──────────────────────────────────────────

    pub fn set_live_match(&self, session: LiveMatchSession) {
        let mut lock = self.session.lock().unwrap();
        lock.live_match = Some(session);
    }

    pub fn take_live_match(&self) -> Option<LiveMatchSession> {
        let mut lock = self.session.lock().unwrap();
        lock.live_match.take()
    }

    pub fn with_live_match<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut LiveMatchSession) -> R,
    {
        let mut lock = self.session.lock().unwrap();
        lock.live_match.as_mut().map(f)
    }
}

#[cfg(test)]
mod tests {
    use super::StateManager;
    use crate::clock::GameClock;
    use crate::domain::league::{Fixture, FixtureStatus, League, MatchType, StandingEntry};
    use crate::domain::manager::Manager;
    use crate::domain::player::{LolRole, Player, PlayerAttributes};
    use crate::domain::team::Team;
    use crate::game::Game;
    use crate::live_match_manager::{self, MatchMode};
    use chrono::{TimeZone, Utc};

    fn default_attrs() -> PlayerAttributes {
        PlayerAttributes {
            mechanics: 65,
            laning: 65,
            teamfighting: 65,
            macro_play: 65,
            consistency: 65,
            shotcalling: 50,
            champion_pool: 65,
            discipline: 65,
            mental_resilience: 65,
        }
    }

    fn make_player(id: &str, name: &str, team_id: &str, role: LolRole) -> Player {
        let mut player = Player::new(
            id.to_string(),
            name.to_string(),
            format!("Full {}", name),
            "1995-01-01".to_string(),
            "GB".to_string(),
            role,
            default_attrs(),
        );
        player.team_id = Some(team_id.to_string());
        player.morale = 70;
        player.condition = 90;
        player
    }

    fn make_team(id: &str, name: &str) -> Team {
        Team::new(
            id.to_string(),
            name.to_string(),
            name[..3].to_string(),
            "England".to_string(),
            "London".to_string(),
            "Stadium".to_string(),
            40_000,
        )
    }

    fn make_squad(team_id: &str) -> Vec<Player> {
        let mut players = Vec::new();

        for idx in 0..2 {
            players.push(make_player(
                &format!("{}_support{}", team_id, idx),
                &format!("Support{}", idx),
                team_id,
                LolRole::Support,
            ));
        }

        for idx in 0..7 {
            players.push(make_player(
                &format!("{}_top{}", team_id, idx),
                &format!("Top{}", idx),
                team_id,
                LolRole::Top,
            ));
        }

        for idx in 0..7 {
            players.push(make_player(
                &format!("{}_mid{}", team_id, idx),
                &format!("Mid{}", idx),
                team_id,
                LolRole::Mid,
            ));
        }

        for idx in 0..6 {
            players.push(make_player(
                &format!("{}_jungle{}", team_id, idx),
                &format!("Jungle{}", idx),
                team_id,
                LolRole::Jungle,
            ));
        }

        players
    }

    fn make_game_with_fixture() -> Game {
        let clock = GameClock::new(Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap());
        let mut manager = Manager::new(
            "mgr1".to_string(),
            "Test".to_string(),
            "Manager".to_string(),
            "1980-01-01".to_string(),
            "England".to_string(),
        );
        manager.hire("team1".to_string());

        let team1 = make_team("team1", "Test FC");
        let team2 = make_team("team2", "Rival FC");

        let mut players = make_squad("team1");
        players.extend(make_squad("team2"));

        let fixture = Fixture {
            id: "fix1".to_string(),
            matchday: 1,
            date: "2025-06-15".to_string(),
            home_team_id: "team1".to_string(),
            away_team_id: "team2".to_string(),
            match_type: MatchType::League,
            best_of: 1,
            status: FixtureStatus::Scheduled,
            result: None,
        };

        let league = League {
            id: "league1".to_string(),
            name: "Test League".to_string(),
            season: 1,
            competition_id: None,
            fixtures: vec![fixture],
            standings: vec![
                StandingEntry::new("team1".to_string()),
                StandingEntry::new("team2".to_string()),
            ],
            logo: None,
            league_kind: crate::domain::league::LeagueKind::Main,
            split_index: 0,
            tier: 0,
            active: false,
        };

        let mut game = Game::new(clock, manager, vec![team1, team2], players, vec![], vec![]);
        game.leagues = vec![league];
        game
    }

    #[test]
    fn game_lifecycle_supports_set_get_and_clear() {
        let state = StateManager::new();
        assert_eq!(state.get_game(|game| game.teams.len()), None);

        state.set_game(make_game_with_fixture());

        assert_eq!(
            state.get_game(|game| game.manager.team_id.clone()),
            Some(Some("team1".to_string()))
        );

        state.clear_game();

        assert_eq!(state.get_game(|game| game.teams.len()), None);
    }

    #[test]
    fn save_id_lifecycle_supports_set_get_and_clear() {
        let state = StateManager::new();
        assert_eq!(state.get_save_id(), None);

        state.set_save_id("save-1".to_string());
        assert_eq!(state.get_save_id(), Some("save-1".to_string()));

        state.clear_save_id();
        assert_eq!(state.get_save_id(), None);
    }

    #[test]
    fn live_match_lifecycle_supports_mutation_and_take() {
        let state = StateManager::new();
        let game = make_game_with_fixture();
        let session =
            live_match_manager::create_live_match(&game, 0, MatchMode::Spectator, false).unwrap();

        assert!(state.take_live_match().is_none());

        state.set_live_match(session);

        let mode = state
            .with_live_match(|live_match| {
                live_match.mode = MatchMode::Instant;
                live_match.mode
            })
            .unwrap();
        assert_eq!(mode, MatchMode::Instant);

        let taken = state.take_live_match().unwrap();
        assert_eq!(taken.mode, MatchMode::Instant);
        assert!(state.take_live_match().is_none());
        assert!(state.with_live_match(|_| ()).is_none());
    }

    #[test]
    fn unified_session_can_access_multiple_fields() {
        let state = StateManager::new();
        state.set_game(make_game_with_fixture());
        state.set_save_id("save-99".to_string());

        // Read multiple fields under the same lock via with_session
        let (game_len, save_id) =
            state.with_session(|s| (s.game.as_ref().map(|g| g.teams.len()), s.save_id.clone()));
        assert_eq!(game_len, Some(2));
        assert_eq!(save_id, Some("save-99".to_string()));
    }
}
