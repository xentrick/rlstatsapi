use std::collections::HashSet;

use crate::events::{
    GoalScoredData, MatchEndedData, StatsEvent, UpdateStateData,
    UpdateStatePlayer,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    UpdateState,
    BallHit,
    ClockUpdatedSeconds,
    CountdownBegin,
    CrossbarHit,
    GoalReplayEnd,
    GoalReplayStart,
    GoalReplayWillEnd,
    GoalScored,
    MatchCreated,
    MatchInitialized,
    MatchDestroyed,
    MatchEnded,
    MatchPaused,
    MatchUnpaused,
    PodiumStart,
    ReplayCreated,
    RoundStarted,
    StatfeedEvent,
    Unknown,
}

impl From<&StatsEvent> for EventKind {
    fn from(value: &StatsEvent) -> Self {
        match value {
            StatsEvent::UpdateState(_) => Self::UpdateState,
            StatsEvent::BallHit(_) => Self::BallHit,
            StatsEvent::ClockUpdatedSeconds(_) => Self::ClockUpdatedSeconds,
            StatsEvent::CountdownBegin(_) => Self::CountdownBegin,
            StatsEvent::CrossbarHit(_) => Self::CrossbarHit,
            StatsEvent::GoalReplayEnd(_) => Self::GoalReplayEnd,
            StatsEvent::GoalReplayStart(_) => Self::GoalReplayStart,
            StatsEvent::GoalReplayWillEnd(_) => Self::GoalReplayWillEnd,
            StatsEvent::GoalScored(_) => Self::GoalScored,
            StatsEvent::MatchCreated(_) => Self::MatchCreated,
            StatsEvent::MatchInitialized(_) => Self::MatchInitialized,
            StatsEvent::MatchDestroyed(_) => Self::MatchDestroyed,
            StatsEvent::MatchEnded(_) => Self::MatchEnded,
            StatsEvent::MatchPaused(_) => Self::MatchPaused,
            StatsEvent::MatchUnpaused(_) => Self::MatchUnpaused,
            StatsEvent::PodiumStart(_) => Self::PodiumStart,
            StatsEvent::ReplayCreated(_) => Self::ReplayCreated,
            StatsEvent::RoundStarted(_) => Self::RoundStarted,
            StatsEvent::StatfeedEvent(_) => Self::StatfeedEvent,
            StatsEvent::Unknown(_) => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    kinds: HashSet<EventKind>,
    player_name: Option<String>,
    player_primary_id: Option<String>,
    team_num: Option<i64>,
    match_guid: Option<String>,
}

impl EventFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn include_kind(mut self, kind: EventKind) -> Self {
        self.kinds.insert(kind);
        self
    }

    pub fn include_kinds<I>(mut self, kinds: I) -> Self
    where
        I: IntoIterator<Item = EventKind>,
    {
        self.kinds.extend(kinds);
        self
    }

    pub fn with_player_name(mut self, player_name: impl Into<String>) -> Self {
        self.player_name = Some(player_name.into());
        self
    }

    pub fn with_player_primary_id(
        mut self,
        player_primary_id: impl Into<String>,
    ) -> Self {
        self.player_primary_id = Some(player_primary_id.into());
        self
    }

    pub fn with_team_num(mut self, team_num: i64) -> Self {
        self.team_num = Some(team_num);
        self
    }

    pub fn with_match_guid(mut self, match_guid: impl Into<String>) -> Self {
        self.match_guid = Some(match_guid.into());
        self
    }

    pub fn matches(&self, event: &StatsEvent) -> bool {
        if !self.kinds.is_empty()
            && !self.kinds.contains(&EventKind::from(event))
        {
            return false;
        }

        if let Some(expected_guid) = self.match_guid.as_deref() {
            let matches_guid = event_match_guid(event)
                .is_some_and(|guid| guid == expected_guid);
            if !matches_guid {
                return false;
            }
        }

        if let Some(expected_team) = self.team_num {
            if !event_has_team(event, expected_team) {
                return false;
            }
        }

        if let Some(expected_name) = self.player_name.as_deref() {
            if !event_has_player_name(event, expected_name) {
                return false;
            }
        }

        if let Some(expected_primary_id) = self.player_primary_id.as_deref() {
            if !event_has_player_primary_id(event, expected_primary_id) {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSnapshot {
    pub match_guid: Option<String>,
    pub frame: Option<i64>,
    pub time_seconds: Option<i64>,
    pub name: String,
    pub primary_id: Option<String>,
    pub team_num: Option<i64>,
    pub score: Option<i64>,
    pub goals: Option<i64>,
    pub shots: Option<i64>,
    pub assists: Option<i64>,
    pub saves: Option<i64>,
    pub touches: Option<i64>,
    pub demos: Option<i64>,
    pub speed: Option<f64>,
    pub boost: Option<i64>,
    pub b_boosting: Option<bool>,
    pub b_supersonic: Option<bool>,
}

impl PlayerSnapshot {
    fn from_update(
        update: &UpdateStateData,
        player: &UpdateStatePlayer,
    ) -> Self {
        Self {
            match_guid: update.match_guid.clone(),
            frame: update.game.frame,
            time_seconds: update.game.time_seconds,
            name: player.name.clone().unwrap_or_default(),
            primary_id: player.primary_id.clone(),
            team_num: player.team_num,
            score: player.score,
            goals: player.goals,
            shots: player.shots,
            assists: player.assists,
            saves: player.saves,
            touches: player.touches,
            demos: player.demos,
            speed: player.effective_speed(),
            boost: player.effective_boost(),
            b_boosting: player.effective_boosting(),
            b_supersonic: player.effective_supersonic(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerTracker {
    player_name: String,
    latest: Option<PlayerSnapshot>,
}

impl PlayerTracker {
    pub fn by_name(player_name: impl Into<String>) -> Self {
        Self {
            player_name: player_name.into(),
            latest: None,
        }
    }

    pub fn latest(&self) -> Option<&PlayerSnapshot> {
        self.latest.as_ref()
    }

    pub fn update_from_event(
        &mut self,
        event: &StatsEvent,
    ) -> Option<PlayerSnapshot> {
        let StatsEvent::UpdateState(update) = event else {
            return None;
        };

        let player = update.players.iter().find(|player| {
            player.name.as_deref().is_some_and(|name| {
                name.eq_ignore_ascii_case(&self.player_name)
            })
        })?;

        let snapshot = PlayerSnapshot::from_update(update, player);
        let changed = self.latest.as_ref() != Some(&snapshot);
        self.latest = Some(snapshot.clone());

        if changed { Some(snapshot) } else { None }
    }
}

#[derive(Debug, Clone)]
pub enum MatchSignal {
    GoalScored(GoalScoredData),
    MatchConcluded(MatchEndedData),
}

pub fn to_match_signal(event: &StatsEvent) -> Option<MatchSignal> {
    match event {
        StatsEvent::GoalScored(data) => {
            Some(MatchSignal::GoalScored(data.clone()))
        }
        StatsEvent::MatchEnded(data) => {
            Some(MatchSignal::MatchConcluded(data.clone()))
        }
        _ => None,
    }
}

pub fn winner_team_num(event: &StatsEvent) -> Option<i64> {
    match event {
        StatsEvent::MatchEnded(data) => Some(data.winner_team_num),
        _ => None,
    }
}

fn event_match_guid(event: &StatsEvent) -> Option<&str> {
    match event {
        StatsEvent::UpdateState(data) => data.match_guid.as_deref(),
        StatsEvent::BallHit(data) => data.match_guid.as_deref(),
        StatsEvent::ClockUpdatedSeconds(data) => data.match_guid.as_deref(),
        StatsEvent::CountdownBegin(data) => data.match_guid.as_deref(),
        StatsEvent::CrossbarHit(data) => data.match_guid.as_deref(),
        StatsEvent::GoalReplayEnd(data) => data.match_guid.as_deref(),
        StatsEvent::GoalReplayStart(data) => data.match_guid.as_deref(),
        StatsEvent::GoalReplayWillEnd(data) => data.match_guid.as_deref(),
        StatsEvent::GoalScored(data) => data.match_guid.as_deref(),
        StatsEvent::MatchCreated(data) => data.match_guid.as_deref(),
        StatsEvent::MatchInitialized(data) => data.match_guid.as_deref(),
        StatsEvent::MatchDestroyed(data) => data.match_guid.as_deref(),
        StatsEvent::MatchEnded(data) => data.match_guid.as_deref(),
        StatsEvent::MatchPaused(data) => data.match_guid.as_deref(),
        StatsEvent::MatchUnpaused(data) => data.match_guid.as_deref(),
        StatsEvent::PodiumStart(data) => data.match_guid.as_deref(),
        StatsEvent::ReplayCreated(data) => data.match_guid.as_deref(),
        StatsEvent::RoundStarted(data) => data.match_guid.as_deref(),
        StatsEvent::StatfeedEvent(data) => data.match_guid.as_deref(),
        StatsEvent::Unknown(data) => data
            .data
            .get("MatchGuid")
            .or_else(|| data.data.get("matchGuid"))
            .or_else(|| data.data.get("match_guid"))
            .and_then(|value| value.as_str()),
    }
}

fn event_has_team(event: &StatsEvent, expected_team: i64) -> bool {
    match event {
        StatsEvent::UpdateState(data) => {
            data.players
                .iter()
                .any(|player| player.team_num == Some(expected_team))
                || data
                    .game
                    .teams
                    .iter()
                    .any(|team| team.team_num == Some(expected_team))
        }
        StatsEvent::GoalScored(data) => {
            data.scorer.team_num == expected_team
                || data
                    .assister
                    .as_ref()
                    .is_some_and(|assister| assister.team_num == expected_team)
                || data.ball_last_touch.player.team_num == expected_team
        }
        StatsEvent::BallHit(data) => data
            .players
            .iter()
            .any(|player| player.team_num == expected_team),
        StatsEvent::StatfeedEvent(data) => {
            data.main_target.team_num == expected_team
                || data
                    .secondary_target
                    .as_ref()
                    .is_some_and(|target| target.team_num == expected_team)
        }
        StatsEvent::MatchEnded(data) => data.winner_team_num == expected_team,
        _ => false,
    }
}

fn event_has_player_name(event: &StatsEvent, expected_name: &str) -> bool {
    match event {
        StatsEvent::UpdateState(data) => data.players.iter().any(|player| {
            player
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(expected_name))
        }),
        StatsEvent::GoalScored(data) => {
            data.scorer.name.eq_ignore_ascii_case(expected_name)
                || data.assister.as_ref().is_some_and(|assister| {
                    assister.name.eq_ignore_ascii_case(expected_name)
                })
                || data
                    .ball_last_touch
                    .player
                    .name
                    .eq_ignore_ascii_case(expected_name)
        }
        StatsEvent::BallHit(data) => data
            .players
            .iter()
            .any(|player| player.name.eq_ignore_ascii_case(expected_name)),
        StatsEvent::StatfeedEvent(data) => {
            data.main_target.name.eq_ignore_ascii_case(expected_name)
                || data.secondary_target.as_ref().is_some_and(|target| {
                    target.name.eq_ignore_ascii_case(expected_name)
                })
        }
        _ => false,
    }
}

fn event_has_player_primary_id(
    event: &StatsEvent,
    expected_primary_id: &str,
) -> bool {
    match event {
        StatsEvent::UpdateState(data) => data.players.iter().any(|player| {
            player
                .primary_id
                .as_deref()
                .is_some_and(|primary_id| primary_id == expected_primary_id)
        }),
        _ => false,
    }
}
