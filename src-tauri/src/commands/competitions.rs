use log::info;
use olm_core::competitions;
use olm_core::domain::player::Player;
use olm_core::domain::staff::Staff;
use olm_core::domain::team::Team;
use olm_core::generator::definitions::{CompetitionManifest, LeagueSelectionData};
use std::path::{Path, PathBuf};
use tauri::Manager as TauriManager;

// ---------------------------------------------------------------------------
// Path resolution (Tauri-specific — uses AppHandle)
// ---------------------------------------------------------------------------

/// Resolve the base `data/competitions/` directory with multi-tier fallback.
fn resolve_competitions_base(app_handle: &tauri::AppHandle) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    info!("[competitions] cwd: {:?}", cwd);

    let mut candidates: Vec<Option<PathBuf>> = vec![
        // Project-local data takes precedence during development.
        Some(cwd.join("..").join("data").join("competitions")),
        Some(cwd.join("data").join("competitions")),
        Some(cwd.join("src-tauri").join("data").join("competitions")),
    ];

    candidates.extend(
        android_writable_data_candidates(app_handle)
            .into_iter()
            .map(|data_dir| Some(data_dir.join("competitions"))),
    );

    candidates.extend([
        // Bundled resource — sync-data.mjs copies data/ → src-tauri/data/
        // during build, and "data/**/*" in tauri.conf.json bundles it
        // directly (no _up_ prefix) so it works on Android too.
        app_handle
            .path()
            .resource_dir()
            .ok()
            .map(|dir| dir.join("data").join("competitions")),
        // Imported data (writable app-data dir) — last resort so stale
        // cached copies don't shadow project changes during dev.
        app_handle
            .path()
            .app_data_dir()
            .ok()
            .map(|dir| dir.join("data").join("competitions")),
    ]);

    let candidate_count = candidates.len();
    for candidate in candidates.into_iter().flatten() {
        info!("[competitions] checking candidate: {:?}", candidate);
        if has_competition_manifest_in(&candidate) {
            info!("[competitions] resolved to: {:?}", candidate);
            return Some(candidate);
        }
    }

    info!(
        "[competitions] no competitions directory found among {} candidates",
        candidate_count
    );
    None
}

fn has_competition_manifest_in(competitions_base: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(competitions_base) else {
        return false;
    };

    entries.flatten().any(|entry| {
        let path = entry.path();
        path.is_dir() && path.join("manifest.json").is_file()
    })
}

fn has_data_competition_manifest(data_base: &Path) -> bool {
    has_competition_manifest_in(&data_base.join("competitions"))
}

fn data_candidate_status(candidate: &Path) -> String {
    let competitions_base = candidate.join("competitions");
    let has_manifest = has_competition_manifest_in(&competitions_base);
    format!(
        "{} | data_dir={} | competitions_dir={} | manifest={}",
        candidate.display(),
        candidate.is_dir(),
        competitions_base.is_dir(),
        has_manifest
    )
}

#[cfg(target_os = "android")]
fn android_writable_data_candidates(app_handle: &tauri::AppHandle) -> Vec<PathBuf> {
    let Ok(app_data_dir) = app_handle.path().app_data_dir() else {
        return Vec::new();
    };

    writable_data_candidates_from_app_data_dir(&app_data_dir)
}

#[cfg(not(target_os = "android"))]
fn android_writable_data_candidates(_app_handle: &tauri::AppHandle) -> Vec<PathBuf> {
    Vec::new()
}

#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
fn writable_data_candidates_from_app_data_dir(app_data_dir: &Path) -> Vec<PathBuf> {
    app_data_dir
        .ancestors()
        .take(6)
        .flat_map(|ancestor| [ancestor.join("data"), ancestor.join("files").join("data")])
        .collect()
}

/// Resolve the base `data/` directory for runtime file reads.
pub fn resolve_data_base(app_handle: &tauri::AppHandle) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;

    for candidate in data_base_candidates(app_handle, &cwd).into_iter().flatten() {
        info!("[competitions] checking data candidate: {:?}", candidate);
        if candidate.is_dir() && has_data_competition_manifest(&candidate) {
            info!("[competitions] resolved data to: {:?}", candidate);
            return Some(candidate);
        }
    }

    None
}

fn data_base_candidates(app_handle: &tauri::AppHandle, cwd: &Path) -> Vec<Option<PathBuf>> {
    let mut candidates: Vec<Option<PathBuf>> = vec![
        // Project-local data takes precedence during development.
        Some(cwd.join("..").join("data")),
        Some(cwd.join("data")),
        Some(cwd.join("src-tauri").join("data")),
    ];

    candidates.extend(
        android_writable_data_candidates(app_handle)
            .into_iter()
            .map(Some),
    );

    candidates.extend([
        // Bundled resource — sync-data.mjs copies data/ → src-tauri/data/
        // during build, and "data/**/*" in tauri.conf.json bundles it
        // directly (no _up_ prefix) so it works on Android too.
        app_handle
            .path()
            .resource_dir()
            .ok()
            .map(|dir| dir.join("data")),
        // Imported data (writable app-data dir) — last resort so stale
        // cached copies don't shadow project changes during dev.
        app_handle
            .path()
            .app_data_dir()
            .ok()
            .map(|dir| dir.join("data")),
    ]);

    candidates
}

fn data_directory_not_found_error(app_handle: &tauri::AppHandle) -> String {
    let Ok(cwd) = std::env::current_dir() else {
        return "Data directory not found. Could not resolve current working directory."
            .to_string();
    };

    let checked = data_base_candidates(app_handle, &cwd)
        .into_iter()
        .flatten()
        .map(|candidate| data_candidate_status(&candidate))
        .collect::<Vec<_>>()
        .join("\n");

    format!("Data directory not found. Checked candidates:\n{}", checked)
}

// ---------------------------------------------------------------------------
// Thin wrappers — resolve paths and delegate to olm_core
// ---------------------------------------------------------------------------

pub fn scan_competitions(app_handle: &tauri::AppHandle) -> Vec<CompetitionManifest> {
    let Some(base) = resolve_competitions_base(app_handle) else {
        return vec![];
    };
    competitions::scan_competitions(&base)
}

pub fn load_competition_manifest(
    app_handle: &tauri::AppHandle,
    competition_id: &str,
) -> Result<CompetitionManifest, String> {
    let base = resolve_competitions_base(app_handle)
        .ok_or_else(|| "Competitions directory not found.".to_string())?;
    competitions::load_competition_manifest(&base, competition_id)
}

pub fn load_competition_teams(
    app_handle: &tauri::AppHandle,
    manifest: &CompetitionManifest,
) -> Result<Vec<Team>, String> {
    let data_base =
        resolve_data_base(app_handle).ok_or_else(|| data_directory_not_found_error(app_handle))?;
    competitions::load_teams(&data_base, manifest)
}

pub fn load_competition_players(
    app_handle: &tauri::AppHandle,
    manifest: &CompetitionManifest,
) -> Result<Vec<Player>, String> {
    let data_base =
        resolve_data_base(app_handle).ok_or_else(|| data_directory_not_found_error(app_handle))?;
    competitions::load_players(&data_base, manifest)
}

pub fn load_competition_staff(
    app_handle: &tauri::AppHandle,
    manifest: &CompetitionManifest,
) -> Result<Vec<Staff>, String> {
    let data_base =
        resolve_data_base(app_handle).ok_or_else(|| data_directory_not_found_error(app_handle))?;
    competitions::load_staff(&data_base, manifest)
}

pub fn load_staff_free_agents(app_handle: &tauri::AppHandle) -> Result<Vec<Staff>, String> {
    let data_base =
        resolve_data_base(app_handle).ok_or_else(|| data_directory_not_found_error(app_handle))?;
    competitions::load_staff_free_agents(&data_base)
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_league_selection_data(
    app_handle: tauri::AppHandle,
) -> Result<LeagueSelectionData, String> {
    info!("[cmd] get_league_selection_data");
    let data_base = resolve_data_base(&app_handle)
        .ok_or_else(|| data_directory_not_found_error(&app_handle))?;
    Ok(competitions::build_league_selection(&data_base))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn manifest_check_requires_real_competition_manifest() {
        let root =
            std::env::temp_dir().join(format!("olm-competitions-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        let competitions_dir = root.join("competitions");
        fs::create_dir_all(competitions_dir.join("lec")).unwrap();

        assert!(!has_competition_manifest_in(&competitions_dir));

        fs::write(competitions_dir.join("lec").join("manifest.json"), "{}").unwrap();

        assert!(has_competition_manifest_in(&competitions_dir));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn writable_candidates_include_android_files_dir_data() {
        let app_data_dir = Path::new("/data/user/0/com.openleaguemanager.olmanager");

        let candidates = writable_data_candidates_from_app_data_dir(app_data_dir);

        assert!(candidates.contains(&PathBuf::from(
            "/data/user/0/com.openleaguemanager.olmanager/data"
        )));
        assert!(candidates.contains(&PathBuf::from(
            "/data/user/0/com.openleaguemanager.olmanager/files/data"
        )));
    }

    #[test]
    fn diagnostic_status_reports_manifest_presence() {
        let root =
            std::env::temp_dir().join(format!("olm-data-status-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        fs::create_dir_all(root.join("competitions").join("lec")).unwrap();
        fs::write(
            root.join("competitions").join("lec").join("manifest.json"),
            "{}",
        )
        .unwrap();

        let status = data_candidate_status(&root);

        assert!(status.contains("data_dir=true"));
        assert!(status.contains("competitions_dir=true"));
        assert!(status.contains("manifest=true"));

        let _ = fs::remove_dir_all(&root);
    }
}
