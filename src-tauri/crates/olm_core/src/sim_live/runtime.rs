use super::*;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Create a SimLiveSession from an init request, without storing it.
/// Used by WebSocket handler where the session lives on the task stack.
pub fn init_session_from_request(request: &SimLiveInitRequest) -> Result<SimLiveSession, String> {
    if request.session_id.trim().is_empty() {
        return Err("sessionId is required".to_string());
    }
    let mut state = create_initial_state(
        &request.seed,
        &request.snapshot,
        &request.champion_by_player_id,
        &request.champion_profiles_by_id,
        &request.champion_ultimates_by_id,
        request.ai_mode,
    );
    ensure_runtime_state_defaults(&mut state);
    let runtime_state = decode_runtime_state(state)?;
    Ok(SimLiveSession {
        id: request.session_id.clone(),
        seed: request.seed.clone(),
        state: runtime_state,
        tick_index: 0,
        wave_spawn_at: MINION_FIRST_WAVE_AT,
        next_minion_id: 1,
        snapshot: request.snapshot.clone(),
        champion_by_player_id: request.champion_by_player_id.clone(),
        champion_profiles_by_id: request.champion_profiles_by_id.clone(),
        champion_ultimates_by_id: request.champion_ultimates_by_id.clone(),
        lane_combat_state_by_champion: HashMap::new(),
        ai_mode: request.ai_mode,
        policy: request.policy.clone(),
    })
}

pub fn init(
    store: &SimLiveStoreState,
    request: SimLiveInitRequest,
) -> Result<SimLiveStateResponse, String> {
    let session = init_session_from_request(&request)?;
    let state = encode_runtime_state(&session.state)?;
    let mut sessions = store
        .sessions
        .lock()
        .map_err(|_| "lol sim v2 session store lock poisoned".to_string())?;
    sessions.insert(session.id.clone(), session);

    Ok(SimLiveStateResponse {
        session_id: request.session_id,
        state,
    })
}

pub fn tick(
    store: &SimLiveStoreState,
    request: SimLiveTickRequest,
) -> Result<SimLiveStateResponse, String> {
    let mut sessions = store
        .sessions
        .lock()
        .map_err(|_| "lol sim v2 session store lock poisoned".to_string())?;

    let session = sessions
        .get_mut(&request.session_id)
        .ok_or_else(|| format!("lol sim v2 session not found: {}", request.session_id))?;

    let mut runtime = session.state.clone();
    runtime.lane_combat_state_by_champion = session.lane_combat_state_by_champion.clone();
    runtime.ai_mode = session.ai_mode;
    runtime.policy = session.policy.clone();

    let speed = request.speed.max(0.0);
    runtime.speed = speed;
    if runtime.winner.is_some() {
        runtime.running = false;
    } else {
        runtime.running = request.running;
    }

    if !runtime.running {
        session.lane_combat_state_by_champion = runtime.lane_combat_state_by_champion.clone();
        session.state = runtime;
        return Ok(SimLiveStateResponse {
            session_id: session.id.clone(),
            state: encode_runtime_state(&session.state)?,
        });
    }

    let dt = request.dt_sec.clamp(0.0, 0.05) * speed;
    if dt <= 0.0 {
        session.lane_combat_state_by_champion = runtime.lane_combat_state_by_champion.clone();
        session.state = runtime;
        return Ok(SimLiveStateResponse {
            session_id: session.id.clone(),
            state: encode_runtime_state(&session.state)?,
        });
    }

    runtime.time_sec += dt;
    session.tick_index = session.tick_index.saturating_add(1);

    spawn_waves_if_due(&mut runtime, session);
    move_champions(&mut runtime, dt);
    maybe_deploy_rift_herald_charge(&mut runtime);
    place_wards(&mut runtime);
    process_sweepers(&mut runtime);
    move_minions(&mut runtime, dt);
    resolve_minion_combat(&mut runtime);
    resolve_champion_combat(&mut runtime);
    resolve_structure_combat(&mut runtime);
    tick_neutral_timers(&mut runtime);
    cleanup_tick(&mut runtime);
    if runtime.winner.is_some() {
        runtime.running = false;
    }

    session.lane_combat_state_by_champion = runtime.lane_combat_state_by_champion.clone();
    session.state = runtime;

    Ok(SimLiveStateResponse {
        session_id: session.id.clone(),
        state: encode_runtime_state(&session.state)?,
    })
}

pub fn reset(
    store: &SimLiveStoreState,
    request: SimLiveResetRequest,
) -> Result<SimLiveStateResponse, String> {
    let mut sessions = store
        .sessions
        .lock()
        .map_err(|_| "lol sim v2 session store lock poisoned".to_string())?;

    let session = sessions
        .get_mut(&request.session_id)
        .ok_or_else(|| format!("lol sim v2 session not found: {}", request.session_id))?;

    session.seed = request.seed;
    let mut state = create_initial_state(
        &session.seed,
        &session.snapshot,
        &session.champion_by_player_id,
        &session.champion_profiles_by_id,
        &session.champion_ultimates_by_id,
        request.ai_mode,
    );
    session.ai_mode = request.ai_mode;
    if let Some(policy) = request.policy {
        session.policy = policy;
    }
    session.tick_index = 0;
    ensure_runtime_state_defaults(&mut state);
    session.state = decode_runtime_state(state)?;
    session.wave_spawn_at = MINION_FIRST_WAVE_AT;
    session.next_minion_id = 1;
    session.lane_combat_state_by_champion.clear();

    Ok(SimLiveStateResponse {
        session_id: session.id.clone(),
        state: encode_runtime_state(&session.state)?,
    })
}

pub fn dispose(
    store: &SimLiveStoreState,
    request: SimLiveDisposeRequest,
) -> Result<SimLiveDisposeResponse, String> {
    let mut sessions = store
        .sessions
        .lock()
        .map_err(|_| "lol sim v2 session store lock poisoned".to_string())?;
    let removed = sessions.remove(&request.session_id).is_some();

    Ok(SimLiveDisposeResponse {
        session_id: request.session_id,
        disposed: removed,
    })
}

pub fn run_to_completion(
    _store: &SimLiveStoreState,
    request: SimLiveRunToCompletionRequest,
) -> Result<SimLiveRunToCompletionResponse, String> {
    let local_store = SimLiveStoreState::default();

    let session_id = format!(
        "lol-sim-v2-afk-{}-{}",
        request.seed,
        next_run_to_completion_suffix()
    );

    let run_result = (|| {
        let mut response = init(
            &local_store,
            SimLiveInitRequest {
                session_id: session_id.clone(),
                seed: request.seed,
                snapshot: request.snapshot,
                champion_by_player_id: request.champion_by_player_id,
                champion_profiles_by_id: request.champion_profiles_by_id,
                champion_ultimates_by_id: request.champion_ultimates_by_id,
                initial_state: None,
                ai_mode: request.ai_mode,
                policy: request.policy,
            },
        )?;

        let mut ticks = 0u64;
        for _ in 0..request.max_ticks {
            if read_winner(&response.state).is_some() {
                break;
            }

            response = tick(
                &local_store,
                SimLiveTickRequest {
                    session_id: session_id.clone(),
                    dt_sec: request.dt_sec,
                    running: true,
                    speed: request.speed,
                },
            )?;
            ticks = ticks.saturating_add(1);
        }

        Ok(SimLiveRunToCompletionResponse {
            winner: read_winner(&response.state),
            ticks,
            elapsed_simulated_sec: read_time_sec(&response.state),
        })
    })();

    let _ = dispose(
        &local_store,
        SimLiveDisposeRequest {
            session_id: session_id.clone(),
        },
    );

    run_result
}

pub fn skip_to_end(
    store: &SimLiveStoreState,
    request: SimLiveSkipToEndRequest,
) -> Result<SimLiveSkipToEndResponse, String> {
    if request.max_ticks == 0 {
        let sessions = store
            .sessions
            .lock()
            .map_err(|_| "lol sim v2 session store lock poisoned".to_string())?;

        let session = sessions
            .get(&request.session_id)
            .ok_or_else(|| format!("lol sim v2 session not found: {}", request.session_id))?;

        return Ok(SimLiveSkipToEndResponse {
            session_id: request.session_id,
            winner: session.state.winner.clone(),
            elapsed_simulated_sec: session.state.time_sec,
            ticks: 0,
            state: encode_runtime_state(&session.state)?,
        });
    }

    let mut sessions = store
        .sessions
        .lock()
        .map_err(|_| "lol sim v2 session store lock poisoned".to_string())?;

    let session = sessions
        .get_mut(&request.session_id)
        .ok_or_else(|| format!("lol sim v2 session not found: {}", request.session_id))?;

    let mut runtime = session.state.clone();
    runtime.lane_combat_state_by_champion = session.lane_combat_state_by_champion.clone();
    runtime.ai_mode = session.ai_mode;
    runtime.policy = session.policy.clone();
    runtime
        .extra
        .insert(SKIP_FAST_MODE_EXTRA_KEY.to_string(), Value::Bool(true));

    let dt = request.dt_sec.clamp(0.0, 0.05) * request.speed.max(0.0);
    if dt <= 0.0 {
        runtime.extra.remove(SKIP_FAST_MODE_EXTRA_KEY);
        session.lane_combat_state_by_champion = runtime.lane_combat_state_by_champion.clone();
        session.state = runtime;
        return Ok(SimLiveSkipToEndResponse {
            session_id: request.session_id,
            winner: session.state.winner.clone(),
            elapsed_simulated_sec: session.state.time_sec,
            ticks: 0,
            state: encode_runtime_state(&session.state)?,
        });
    }

    let mut ticks = 0u64;
    while ticks < request.max_ticks {
        if runtime.winner.is_some() {
            break;
        }

        let fast_skip_macro_tick = ticks % 3 == 0;

        runtime.speed = request.speed.max(0.0);
        runtime.running = true;
        runtime.time_sec += dt;
        session.tick_index = session.tick_index.saturating_add(1);

        spawn_waves_if_due(&mut runtime, session);
        move_champions(&mut runtime, dt);
        if fast_skip_macro_tick {
            place_wards(&mut runtime);
            process_sweepers(&mut runtime);
        }
        move_minions(&mut runtime, dt);
        resolve_minion_combat(&mut runtime);
        resolve_champion_combat(&mut runtime);
        resolve_structure_combat(&mut runtime);
        tick_neutral_timers(&mut runtime);
        cleanup_tick(&mut runtime);
        ticks = ticks.saturating_add(1);
        if runtime.winner.is_some() {
            runtime.running = false;
            break;
        }
    }

    runtime.extra.remove(SKIP_FAST_MODE_EXTRA_KEY);
    session.lane_combat_state_by_champion = runtime.lane_combat_state_by_champion.clone();
    session.state = runtime;

    Ok(SimLiveSkipToEndResponse {
        session_id: request.session_id,
        winner: session.state.winner.clone(),
        elapsed_simulated_sec: session.state.time_sec,
        ticks,
        state: encode_runtime_state(&session.state)?,
    })
}

fn decode_runtime_state(state: Value) -> Result<RuntimeState, String> {
    serde_json::from_value(state)
        .map_err(|err| format!("failed to decode sim_live runtime state: {err}"))
}

fn encode_runtime_state(state: &RuntimeState) -> Result<Value, String> {
    serde_json::to_value(state)
        .map_err(|err| format!("failed to encode sim_live runtime state: {err}"))
}

fn next_run_to_completion_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0)
}
