use tauri::State;

use olm_core::sim_live::{
    dispose, init, reset, run_to_completion, skip_to_end, tick, SimLiveDisposeRequest,
    SimLiveDisposeResponse, SimLiveResetRequest, SimLiveRunToCompletionRequest,
    SimLiveRunToCompletionResponse, SimLiveSkipToEndRequest, SimLiveSkipToEndResponse,
    SimLiveStateResponse, SimLiveStoreState, SimLiveTickRequest,
};

#[tauri::command]
pub fn sim_live_init(
    state: State<'_, SimLiveStoreState>,
    request: olm_core::sim_live::SimLiveInitRequest,
) -> Result<SimLiveStateResponse, String> {
    init(&state, request)
}

#[tauri::command]
pub fn sim_live_tick(
    state: State<'_, SimLiveStoreState>,
    request: SimLiveTickRequest,
) -> Result<SimLiveStateResponse, String> {
    tick(&state, request)
}

#[tauri::command]
pub fn sim_live_reset(
    state: State<'_, SimLiveStoreState>,
    request: SimLiveResetRequest,
) -> Result<SimLiveStateResponse, String> {
    reset(&state, request)
}

#[tauri::command]
pub fn sim_live_dispose(
    state: State<'_, SimLiveStoreState>,
    request: SimLiveDisposeRequest,
) -> Result<SimLiveDisposeResponse, String> {
    dispose(&state, request)
}

#[tauri::command]
pub async fn sim_live_run_to_completion(
    state: State<'_, SimLiveStoreState>,
    request: SimLiveRunToCompletionRequest,
) -> Result<SimLiveRunToCompletionResponse, String> {
    run_to_completion(&state, request)
}

#[tauri::command]
pub fn sim_live_skip_to_end(
    state: State<'_, SimLiveStoreState>,
    request: SimLiveSkipToEndRequest,
) -> Result<SimLiveSkipToEndResponse, String> {
    skip_to_end(&state, request)
}
