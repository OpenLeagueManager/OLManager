use rand::Rng;

use crate::engine::event::{EventType, MatchEvent};
use crate::engine::report::MatchReport;
use crate::engine::types::{Side, Zone};

use super::{LiveMatchState, MatchPhase, MinuteResult};

impl LiveMatchState {
    pub(super) fn start_match<R: Rng>(&mut self, _rng: &mut R) -> MinuteResult {
        self.phase = MatchPhase::Live;
        self.current_minute = 0;
        self.ball_zone = Zone::Midfield;
        self.possession = Side::Home;

        let kickoff = MatchEvent::new(0, EventType::KickOff, Side::Home, Zone::Midfield);
        self.events.push(kickoff.clone());

        MinuteResult {
            minute: 0,
            phase: self.phase,
            events: vec![kickoff],
            home_score: self.home_score,
            away_score: self.away_score,
            possession: self.possession,
            ball_zone: self.ball_zone,
            is_finished: false,
        }
    }

    pub(super) fn play_minute<R: Rng>(&mut self, rng: &mut R) -> MinuteResult {
        if self.phase == MatchPhase::Finished {
            return self.make_result(true);
        }

        if self.phase != MatchPhase::Live {
            self.phase = MatchPhase::Live;
        }

        self.current_minute = self.current_minute.saturating_add(1);
        let minute = self.current_minute;
        let mut minute_events = Vec::new();

        // Time limit: if Nexus hasn't been destroyed by minute 60, end the match.
        if minute > 60 {
            self.phase = MatchPhase::Finished;
            let win_side = if self.home_score > self.away_score {
                Some(Side::Home)
            } else if self.away_score > self.home_score {
                Some(Side::Away)
            } else {
                None
            };
            // Emit a nexus-destroyed-like event for the leading side, or just finish.
            if let Some(side) = win_side {
                minute_events.push(MatchEvent::new(
                    minute,
                    EventType::NexusDestroyed,
                    side,
                    Zone::Midfield,
                ));
            }
            return MinuteResult {
                minute,
                phase: self.phase,
                events: minute_events,
                home_score: self.home_score,
                away_score: self.away_score,
                possession: self.possession,
                ball_zone: self.ball_zone,
                is_finished: true,
            };
        }

        self.step_lol_map(minute, rng, &mut minute_events);

        MinuteResult {
            minute,
            phase: self.phase,
            events: minute_events,
            home_score: self.home_score,
            away_score: self.away_score,
            possession: self.possession,
            ball_zone: self.ball_zone,
            is_finished: self.phase == MatchPhase::Finished,
        }
    }

    pub(super) fn make_result(&self, _is_finished: bool) -> MinuteResult {
        MinuteResult {
            minute: self.current_minute,
            phase: self.phase,
            events: Vec::new(),
            home_score: self.home_score,
            away_score: self.away_score,
            possession: self.possession,
            ball_zone: self.ball_zone,
            is_finished: true,
        }
    }

    /// Run the match to completion using the given RNG and return the match report.
    pub fn run_to_completion<R: Rng>(mut self, rng: &mut R) -> MatchReport {
        loop {
            let result = self.step_minute(rng);
            if result.is_finished {
                break;
            }
        }
        self.into_report()
    }
}
