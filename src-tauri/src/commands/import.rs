//! Desktop auto-import from the public GitHub `olmanager-data` repository.
//!
//! The desktop build has no bundled data directory, so this module downloads a
//! `tar.gz` archive directly from `codeload.github.com` and extracts it into
//! writable app-data directories:
//!
//!   <app_data>/data/**     ← modular competition/team/player/staff JSON
//!   <app_data>/public/**   ← player-photos / teams-icons / staff-photos / ...
//!
//! The competition loaders (see `commands::competitions`) read from
//! `<app_data>/data`, and the `olm-asset://` protocol (see `lib.rs`) serves
//! imported photos with a fallback to bundled assets.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;

use log::info;
use olm_core::domain::player::Player;
use olm_core::domain::staff::Staff;
use olm_core::domain::team::Team;
use olm_core::game::Game;
use olm_core::game_setup;
use olm_core::generator::definitions::StaffDataFile;
use olm_core::state::StateManager;
use serde_json::Value;
use tauri::Emitter;
use tauri::Manager as TauriManager;
use tauri::State;

use crate::SaveManagerState;

/// Default public OLMDBManager export endpoint. Overridable at runtime via the
/// `OLM_IMPORT_SOURCE` env var.
const DEFAULT_IMPORT_SOURCE: &str =
    "https://codeload.github.com/OpenLeagueManager/olmanager-data/tar.gz/refs/heads/main";
const IMPORT_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const IMPORT_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const IMPORT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(45);
const IMPORT_READ_TIMEOUT: Duration = Duration::from_secs(45);
const IMPORT_PROGRESS_EVENT: &str = "olm-import-progress";

const PUBLIC_PHOTO_DIRS: [&str; 8] = [
    "player-photos",
    "teams-icons",
    "competitions-icons",
    "staff-photos",
    "staff-icons",
    "manager-icons",
    "default",
    "social-avatars",
];

#[derive(Debug, Default, serde::Serialize)]
pub struct ImportSummary {
    pub data_files: usize,
    pub photo_files: usize,
    pub player_count: usize,
    pub team_count: usize,
    pub staff_count: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub phase: &'static str,
    pub message: String,
    pub processed: usize,
    pub total: Option<usize>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct ImportCacheInfo {
    pub exists: bool,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct CatalogResponse {
    pub summary: ImportSummary,
    pub players: Vec<CatalogPlayer>,
    pub teams: Vec<CatalogTeam>,
    pub staff: Vec<CatalogStaff>,
}

#[derive(Debug, serde::Serialize)]
pub struct CatalogPlayer {
    pub id: String,
    pub name: String,
    pub full_name: String,
    pub team_id: Option<String>,
    pub nationality: Option<String>,
    pub role: Option<String>,
    pub image_url: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct CatalogTeam {
    pub id: String,
    pub name: String,
    pub short_name: Option<String>,
    pub country: Option<String>,
    pub competition_id: Option<String>,
    pub logo_url: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct CatalogStaff {
    pub id: String,
    pub name: String,
    pub role: Option<String>,
    pub team_id: Option<String>,
    pub nationality: Option<String>,
    pub image_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Writable destination directories (shared with competitions + asset protocol)
// ---------------------------------------------------------------------------

/// Writable `data/` directory the import extracts into and the loaders prefer.
pub fn writable_data_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app_handle)?.join("data"))
}

/// Writable `public/` directory imported photos are extracted into.
pub fn writable_public_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app_handle)?.join("public"))
}

fn import_cache_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app_handle)?.join("import-cache"))
}

fn import_cache_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(import_cache_dir(app_handle)?.join("olmanager_export.tar.gz"))
}

fn import_zip_cache_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(import_cache_dir(app_handle)?.join("olmanager_export.zip"))
}

fn app_data_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))
}

/// Resolve a `/public`-namespaced asset (e.g. `player-photos/x.webp`) for the
/// `olm-asset://` protocol: imported file first, bundled frontend asset second.
/// Returns the bytes and a best-effort MIME type.
pub fn resolve_public_asset(app_handle: &tauri::AppHandle, rel: &str) -> Option<(Vec<u8>, String)> {
    let safe = safe_relative(rel)?;

    // 1. Imported photo in the writable app-data dir (the whole point).
    if let Ok(public_dir) = writable_public_dir(app_handle) {
        if let Ok(bytes) = std::fs::read(public_dir.join(&safe)) {
            return Some((bytes, mime_for(&safe)));
        }
    }

    // 2. Bundled resources (tauri.conf.json → bundle.resources). Tauri
    //    extracts these at build time; check both the direct path and the
    //    `_up_`-prefixed variant (produced when the resource glob starts with
    //    `../`). This covers player photos and any other public/ assets.
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        for candidate in &[
            resource_dir.join("public").join(&safe),
            resource_dir.join("_up_").join("public").join(&safe),
        ] {
            if let Ok(bytes) = std::fs::read(candidate) {
                return Some((bytes, mime_for(&safe)));
            }
        }
    }

    // 3. Bundled frontend asset embedded in the production build (Vite copies
    //    `public/` to the dist root). Try both key spellings to be safe.
    let key = safe.to_string_lossy().replace('\\', "/");
    let resolver = app_handle.asset_resolver();
    if let Some(asset) = resolver
        .get(format!("/{key}"))
        .or_else(|| resolver.get(key.clone()))
    {
        let mime = if asset.mime_type.is_empty() {
            mime_for(&safe)
        } else {
            asset.mime_type
        };
        return Some((asset.bytes, mime));
    }

    // 4. Dev mode: assets aren't embedded (served by Vite), so read the
    //    frontend `public/` dir from disk. cwd is `src-tauri/` under `tauri dev`.
    if let Ok(cwd) = std::env::current_dir() {
        for candidate in [
            cwd.join("..").join("public").join(&safe),
            cwd.join("public").join(&safe),
        ] {
            if let Ok(bytes) = std::fs::read(&candidate) {
                return Some((bytes, mime_for(&safe)));
            }
        }
    }

    None
}

fn mime_for(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "webp" => "image/webp",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn import_source() -> String {
    std::env::var("OLM_IMPORT_SOURCE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_IMPORT_SOURCE.to_string())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Download the configured public OLMDBManager export and extract it into the
/// writable data/public directories.
#[tauri::command]
pub async fn auto_import_database(
    app_handle: tauri::AppHandle,
    state: State<'_, StateManager>,
    sm_state: State<'_, SaveManagerState>,
) -> Result<ImportSummary, String> {
    let source = import_source();
    info!("[cmd] auto_import_database: source={source}");
    emit_progress(
        &app_handle,
        "downloading",
        "Conectando con GitHub...",
        0,
        None,
    );
    let bytes = download_export(&source, &app_handle).await?;
    emit_progress(
        &app_handle,
        "extracting",
        "Preparando importacion segura...",
        0,
        None,
    );
    let import_app_handle = app_handle.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        let summary = import_tar_gz_safely(&bytes, &import_app_handle)?;
        emit_progress(
            &import_app_handle,
            "caching",
            "Guardando tar.gz para futuros imports...",
            0,
            None,
        );
        write_import_cache(&import_app_handle, &bytes)?;
        Ok::<ImportSummary, String>(summary)
    })
    .await
    .map_err(|e| format!("import task panicked: {e}"))??;
    info!(
        "[cmd] auto_import_database: {} data files, {} photos, {} players, {} teams, {} staff, {} skipped",
        summary.data_files,
        summary.photo_files,
        summary.player_count,
        summary.team_count,
        summary.staff_count,
        summary.skipped
    );
    if let Err(err) = rehydrate_active_game(&app_handle, &state, &sm_state) {
        log::warn!("[cmd] auto_import_database: active rehydrate skipped: {err}");
    }
    Ok(summary)
}

/// Import a local OLMDBManager export `.zip` from disk (manual fallback).
#[tauri::command]
pub async fn import_export_zip(
    app_handle: tauri::AppHandle,
    path: String,
    state: State<'_, StateManager>,
    sm_state: State<'_, SaveManagerState>,
) -> Result<ImportSummary, String> {
    info!("[cmd] import_export_zip: path={path}");
    emit_progress(&app_handle, "reading", "Leyendo ZIP local...", 0, None);
    let import_app_handle = app_handle.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?;
        let summary = import_zip_safely(&bytes, &import_app_handle)?;
        write_import_zip_cache(&import_app_handle, &bytes)?;
        Ok::<ImportSummary, String>(summary)
    })
    .await
    .map_err(|e| format!("import task panicked: {e}"))??;
    if let Err(err) = rehydrate_active_game(&app_handle, &state, &sm_state) {
        log::warn!("[cmd] import_export_zip: active rehydrate skipped: {err}");
    }
    Ok(summary)
}

/// Re-import the last successfully downloaded/imported export without network.
#[tauri::command]
pub async fn import_cached_export(
    app_handle: tauri::AppHandle,
    state: State<'_, StateManager>,
    sm_state: State<'_, SaveManagerState>,
) -> Result<ImportSummary, String> {
    let path = import_cache_path(&app_handle)?;
    info!("[cmd] import_cached_export: path={}", path.display());
    emit_progress(
        &app_handle,
        "reading",
        "Leyendo ultimo tar.gz guardado...",
        0,
        None,
    );
    let import_app_handle = app_handle.clone();
    let summary = tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("No hay tar.gz guardado para reimportar: {e}"))?;
        import_tar_gz_safely(&bytes, &import_app_handle)
    })
    .await
    .map_err(|e| format!("import task panicked: {e}"))??;
    if let Err(err) = rehydrate_active_game(&app_handle, &state, &sm_state) {
        log::warn!("[cmd] import_cached_export: active rehydrate skipped: {err}");
    }
    Ok(summary)
}

#[tauri::command]
pub fn get_import_cache_info(app_handle: tauri::AppHandle) -> Result<ImportCacheInfo, String> {
    let path = import_cache_path(&app_handle)?;
    let metadata = std::fs::metadata(&path).ok();
    Ok(ImportCacheInfo {
        exists: metadata.as_ref().is_some_and(|meta| meta.is_file()),
        path: path.to_string_lossy().to_string(),
        size_bytes: metadata.map(|meta| meta.len()).unwrap_or(0),
    })
}

/// Counts of the currently imported catalog (or zeros if nothing imported yet).
#[tauri::command]
pub fn get_catalog_summary(app_handle: tauri::AppHandle) -> Result<ImportSummary, String> {
    let data_dir = writable_data_dir(&app_handle)?;
    let public_dir = writable_public_dir(&app_handle)?;
    Ok(catalog_summary(&data_dir, &public_dir))
}

/// Full imported catalog (players/teams/staff) for the Settings browser.
#[tauri::command]
pub fn get_catalog(app_handle: tauri::AppHandle) -> Result<CatalogResponse, String> {
    let data_dir = writable_data_dir(&app_handle)?;
    let public_dir = writable_public_dir(&app_handle)?;
    Ok(current_catalog(&data_dir, &public_dir))
}

// ---------------------------------------------------------------------------
// Download + extraction
// ---------------------------------------------------------------------------

fn emit_progress(
    app_handle: &tauri::AppHandle,
    phase: &'static str,
    message: impl Into<String>,
    processed: usize,
    total: Option<usize>,
) {
    let _ = app_handle.emit(
        IMPORT_PROGRESS_EVENT,
        ImportProgress {
            phase,
            message: message.into(),
            processed,
            total,
        },
    );
}

fn write_import_cache(app_handle: &tauri::AppHandle, bytes: &[u8]) -> Result<(), String> {
    write_import_cache_to(import_cache_path(app_handle)?, bytes)
}

fn write_import_zip_cache(app_handle: &tauri::AppHandle, bytes: &[u8]) -> Result<(), String> {
    write_import_cache_to(import_zip_cache_path(app_handle)?, bytes)
}

fn write_import_cache_to(path: PathBuf, bytes: &[u8]) -> Result<(), String> {
    let cache_dir = path
        .parent()
        .ok_or_else(|| format!("cache path has no parent: {path:?}"))?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("mkdir {cache_dir:?}: {e}"))?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "import-cache".to_string());
    let tmp_path = cache_dir.join(format!("{file_name}.tmp-{}", timestamp_millis()));
    std::fs::write(&tmp_path, bytes).map_err(|e| format!("write {tmp_path:?}: {e}"))?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("replace cache {path:?}: {e}"))?;
    }
    std::fs::rename(&tmp_path, &path).map_err(|e| format!("cache {path:?}: {e}"))?;
    Ok(())
}

async fn download_export(url: &str, app_handle: &tauri::AppHandle) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(IMPORT_CONNECT_TIMEOUT)
        .timeout(IMPORT_DOWNLOAD_TIMEOUT)
        .read_timeout(IMPORT_READ_TIMEOUT)
        .build()
        .map_err(|e| format!("create import HTTP client: {e}"))?;
    let mut response = tokio::time::timeout(IMPORT_RESPONSE_TIMEOUT, client.get(url).send())
        .await
        .map_err(|_| {
            format!(
                "GitHub no empezo a responder en {} segundos. Prueba de nuevo o importa un ZIP local.",
                IMPORT_RESPONSE_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| {
            if e.is_timeout() {
                format!(
                    "download {url}: timeout after {} seconds",
                    IMPORT_DOWNLOAD_TIMEOUT.as_secs()
                )
            } else {
                format!("download {url}: {e}")
            }
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("download {url}: HTTP {status}"));
    }
    let total = response.content_length().map(|value| value as usize);
    emit_progress(
        app_handle,
        "downloading",
        "Descargando tarball de GitHub...",
        0,
        total,
    );

    let mut bytes = Vec::with_capacity(total.unwrap_or(0));
    let mut last_reported = 0usize;
    while let Some(chunk) = response.chunk().await.map_err(|e| {
        if e.is_timeout() {
            format!(
                "La descarga no recibio datos durante {} segundos. Prueba de nuevo o usa el ZIP local.",
                IMPORT_READ_TIMEOUT.as_secs()
            )
        } else {
            format!("read response {url}: {e}")
        }
    })? {
        bytes.extend_from_slice(&chunk);
        if bytes.len().saturating_sub(last_reported) >= 1_048_576
            || total.map(|expected| bytes.len() >= expected).unwrap_or(false)
        {
            last_reported = bytes.len();
            emit_progress(
                app_handle,
                "downloading",
                format!("Descargando... {:.1} MB", bytes.len() as f64 / 1_048_576.0),
                bytes.len(),
                total,
            );
        }
    }
    emit_progress(
        app_handle,
        "downloaded",
        format!(
            "Descarga completada ({:.1} MB).",
            bytes.len() as f64 / 1_048_576.0
        ),
        bytes.len(),
        Some(bytes.len()),
    );
    Ok(bytes)
}

/// Extract the export zip into the data/public dirs. Returns a summary.
fn import_zip_safely(bytes: &[u8], app_handle: &tauri::AppHandle) -> Result<ImportSummary, String> {
    let app_dir = app_data_dir(app_handle)?;
    let staging_root = app_dir.join(format!(".import-staging-{}", timestamp_millis()));
    let staging_data = staging_root.join("data");
    let staging_public = staging_root.join("public");

    let mut summary =
        match import_zip(bytes, &staging_data, &staging_public, app_handle).and_then(|summary| {
            validate_import_summary(&summary)?;
            Ok(summary)
        }) {
            Ok(summary) => summary,
            Err(err) => {
                let _ = std::fs::remove_dir_all(&staging_root);
                return Err(err);
            }
        };

    emit_progress(
        app_handle,
        "installing",
        "Validado. Activando nuevos datos...",
        0,
        None,
    );
    install_staged_import(&app_dir, &staging_root)?;

    let data_dir = app_dir.join("data");
    let public_dir = app_dir.join("public");
    let skipped = summary.skipped;
    summary = catalog_summary(&data_dir, &public_dir);
    summary.skipped = skipped;

    emit_progress(
        app_handle,
        "done",
        "Importacion completada.",
        summary.data_files + summary.photo_files,
        Some(summary.data_files + summary.photo_files),
    );
    Ok(summary)
}

fn import_zip(
    bytes: &[u8],
    data_dir: &Path,
    public_dir: &Path,
    app_handle: &tauri::AppHandle,
) -> Result<ImportSummary, String> {
    let reader = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("open zip: {e}"))?;

    let mut summary = ImportSummary::default();
    let total = zip.len();

    for i in 0..total {
        let mut entry = zip.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let Some(dest) = destination_for(&name, data_dir, public_dir) else {
            summary.skipped += 1;
            continue;
        };

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {parent:?}: {e}"))?;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("read {name}: {e}"))?;
        std::fs::write(&dest, &buf).map_err(|e| format!("write {dest:?}: {e}"))?;

        if name.starts_with("data/") {
            summary.data_files += 1;
            add_entity_counts(&mut summary, &name, &buf);
        } else {
            summary.photo_files += 1;
        }

        if i == 0 || i + 1 == total || i % 25 == 0 {
            emit_progress(
                app_handle,
                "extracting",
                format!("Extrayendo {} de {} archivos...", i + 1, total),
                i + 1,
                Some(total),
            );
        }
    }

    Ok(summary)
}

fn import_tar_gz_safely(
    bytes: &[u8],
    app_handle: &tauri::AppHandle,
) -> Result<ImportSummary, String> {
    let app_dir = app_data_dir(app_handle)?;
    let staging_root = app_dir.join(format!(".import-staging-{}", timestamp_millis()));
    let staging_data = staging_root.join("data");
    let staging_public = staging_root.join("public");

    let mut summary = match import_tar_gz(bytes, &staging_data, &staging_public, app_handle)
        .and_then(|summary| {
            validate_import_summary(&summary)?;
            Ok(summary)
        }) {
        Ok(summary) => summary,
        Err(err) => {
            let _ = std::fs::remove_dir_all(&staging_root);
            return Err(err);
        }
    };

    emit_progress(
        app_handle,
        "installing",
        "Validado. Activando nuevos datos...",
        0,
        None,
    );
    install_staged_import(&app_dir, &staging_root)?;

    let data_dir = app_dir.join("data");
    let public_dir = app_dir.join("public");
    let skipped = summary.skipped;
    summary = catalog_summary(&data_dir, &public_dir);
    summary.skipped = skipped;

    emit_progress(
        app_handle,
        "done",
        "Importacion completada.",
        summary.data_files + summary.photo_files,
        Some(summary.data_files + summary.photo_files),
    );
    Ok(summary)
}

fn import_tar_gz(
    bytes: &[u8],
    data_dir: &Path,
    public_dir: &Path,
    app_handle: &tauri::AppHandle,
) -> Result<ImportSummary, String> {
    import_tar_gz_with_emit(bytes, data_dir, public_dir, |progress| {
        let _ = app_handle.emit(IMPORT_PROGRESS_EVENT, progress);
    })
}

fn import_tar_gz_with_emit(
    bytes: &[u8],
    data_dir: &Path,
    public_dir: &Path,
    mut emit: impl FnMut(ImportProgress),
) -> Result<ImportSummary, String> {
    let total = {
        let gz = GzDecoder::new(std::io::Cursor::new(bytes));
        let mut archive = tar::Archive::new(gz);
        archive
            .entries()
            .map_err(|e| format!("read tar: {e}"))?
            .count()
    };

    let gz = GzDecoder::new(std::io::Cursor::new(bytes));
    let mut archive = tar::Archive::new(gz);
    let mut summary = ImportSummary::default();
    for (i, entry) in archive
        .entries()
        .map_err(|e| format!("read tar: {e}"))?
        .enumerate()
    {
        let mut entry = entry.map_err(|e| format!("tar entry {i}: {e}"))?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() || entry_type.is_symlink() {
            continue;
        }

        let raw = entry
            .path()
            .map_err(|e| format!("tar entry path: {e}"))?
            .to_string_lossy()
            .to_string();
        let stripped = match strip_tar_prefix(&raw) {
            Some(stripped) => stripped,
            None => {
                summary.skipped += 1;
                continue;
            }
        };

        let Some(dest) = destination_for(stripped, data_dir, public_dir) else {
            summary.skipped += 1;
            continue;
        };

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {parent:?}: {e}"))?;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("read {stripped}: {e}"))?;
        std::fs::write(&dest, &buf).map_err(|e| format!("write {dest:?}: {e}"))?;

        if stripped.starts_with("data/") {
            summary.data_files += 1;
            add_entity_counts(&mut summary, stripped, &buf);
        } else {
            summary.photo_files += 1;
        }

        if i == 0 || i + 1 == total || i % 25 == 0 {
            emit(ImportProgress {
                phase: "extracting",
                message: format!("Extrayendo {} de {} archivos...", i + 1, total),
                processed: i + 1,
                total: Some(total),
            });
        }
    }

    Ok(summary)
}

/// Strip GitHub's `<repo>-<branch>/` tarball prefix generically.
fn strip_tar_prefix(raw: &str) -> Option<&str> {
    match raw.find('/') {
        Some(idx) => {
            let stripped = &raw[idx + 1..];
            if stripped.is_empty() {
                None
            } else {
                Some(stripped)
            }
        }
        None => None,
    }
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn validate_import_summary(summary: &ImportSummary) -> Result<(), String> {
    if summary.data_files == 0 {
        return Err("El paquete no contiene archivos data/ importables.".to_string());
    }
    if summary.player_count == 0 || summary.team_count == 0 {
        return Err(
            "El paquete no parece una exportacion valida: faltan jugadores o equipos.".to_string(),
        );
    }
    Ok(())
}

fn install_staged_import(app_dir: &Path, staging_root: &Path) -> Result<(), String> {
    let backup_root = app_dir.join(format!(".import-backup-{}", timestamp_millis()));
    let data_dir = app_dir.join("data");
    let public_dir = app_dir.join("public");
    let staging_data = staging_root.join("data");
    let staging_public = staging_root.join("public");

    std::fs::create_dir_all(&backup_root).map_err(|e| format!("mkdir {backup_root:?}: {e}"))?;

    let result = (|| {
        replace_dir(&data_dir, &staging_data, &backup_root)?;
        replace_dir(&public_dir, &staging_public, &backup_root)?;
        Ok::<(), String>(())
    })();

    if let Err(err) = result {
        let _ = restore_backup(&data_dir, &public_dir, &backup_root);
        let _ = std::fs::remove_dir_all(staging_root);
        return Err(err);
    }

    let _ = std::fs::remove_dir_all(&backup_root);
    let _ = std::fs::remove_dir_all(staging_root);
    Ok(())
}

fn replace_dir(dest: &Path, staged: &Path, backup_root: &Path) -> Result<(), String> {
    if !staged.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {parent:?}: {e}"))?;
    }
    if dest.exists() {
        let backup = backup_root.join(
            dest.file_name()
                .ok_or_else(|| format!("invalid destination path {dest:?}"))?,
        );
        std::fs::rename(dest, &backup).map_err(|e| format!("backup {dest:?}: {e}"))?;
    }
    std::fs::rename(staged, dest).map_err(|e| format!("install {staged:?} -> {dest:?}: {e}"))
}

fn restore_backup(data_dir: &Path, public_dir: &Path, backup_root: &Path) -> Result<(), String> {
    for dest in [data_dir, public_dir] {
        let Some(name) = dest.file_name() else {
            continue;
        };
        let backup = backup_root.join(name);
        if dest.exists() {
            let _ = std::fs::remove_dir_all(dest);
        }
        if backup.exists() {
            std::fs::rename(&backup, dest).map_err(|e| format!("restore {dest:?}: {e}"))?;
        }
    }
    Ok(())
}

fn collect_staff_from_shards(app_handle: &tauri::AppHandle) -> Vec<Staff> {
    let Some(data_base) = crate::commands::competitions::resolve_data_base(app_handle) else {
        log::debug!("[import] data base unavailable during staff rehydrate");
        return Vec::new();
    };
    let staffs_dir = data_base.join("staffs");
    let Ok(entries) = std::fs::read_dir(&staffs_dir) else {
        log::debug!(
            "[import] staffs dir unavailable during rehydrate: {:?}",
            staffs_dir
        );
        return Vec::new();
    };

    let mut staff = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let competition_id = stem.strip_suffix("_staffs");
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let mut members = match serde_json::from_slice::<StaffDataFile>(&bytes)
            .map(|data| data.staff)
            .or_else(|_| serde_json::from_slice::<Vec<Staff>>(&bytes))
        {
            Ok(members) => members,
            Err(err) => {
                log::debug!("[import] skipped malformed staff shard {:?}: {err}", path);
                continue;
            }
        };

        if let Some(cid) = competition_id {
            let prefix = format!("{cid}-");
            for member in &mut members {
                if let Some(team_id) = member.team_id.clone() {
                    if team_id != "fa" && team_id != "freeagent" && !team_id.starts_with(&prefix) {
                        member.team_id = Some(format!("{prefix}{team_id}"));
                    }
                }
            }
        }
        staff.extend(members);
    }

    log::info!(
        "[import] collected {} staff directly from {:?}",
        staff.len(),
        staffs_dir
    );
    staff
}

fn collect_runtime_staff(app_handle: &tauri::AppHandle) -> Vec<Staff> {
    let mut staff = collect_staff_from_shards(app_handle);

    staff.extend(
        crate::commands::competitions::load_staff_free_agents(app_handle).unwrap_or_else(|err| {
            log::debug!("[import] free-agent staff unavailable during rehydrate: {err}");
            Vec::new()
        }),
    );

    for manifest in crate::commands::competitions::scan_competitions(app_handle)
        .iter()
        .filter(|manifest| !manifest.legacy)
    {
        let prefix = format!("{}-", manifest.id);
        match crate::commands::competitions::load_competition_staff(app_handle, manifest) {
            Ok(comp_staff) => {
                for mut member in comp_staff {
                    if let Some(team_id) = member.team_id.clone() {
                        if team_id != "fa"
                            && team_id != "freeagent"
                            && !team_id.starts_with(&prefix)
                        {
                            member.team_id = Some(format!("{}-{team_id}", manifest.id));
                        }
                    }
                    staff.push(member);
                }
            }
            Err(err) => {
                log::debug!(
                    "[import] staff unavailable for '{}' during rehydrate: {err}",
                    manifest.id
                );
            }
        }
    }

    let mut seen = HashSet::new();
    staff
        .into_iter()
        .filter(|member| seen.insert(member.id.clone()))
        .collect()
}

/// Load every non-legacy competition's players and apply the SAME namespacing
/// the new-game world assembly does (`assemble_world_from_modular_data`):
/// prefix `team_id` with the competition id and backfill default morale /
/// condition. Deduplicated by player id so multi-competition overlaps collapse.
fn collect_runtime_players(app_handle: &tauri::AppHandle) -> Vec<Player> {
    let mut players: Vec<Player> = Vec::new();

    for manifest in crate::commands::competitions::scan_competitions(app_handle)
        .iter()
        .filter(|manifest| !manifest.legacy)
    {
        let cid = &manifest.id;
        let prefix = format!("{}-", cid);
        match crate::commands::competitions::load_competition_players(app_handle, manifest) {
            Ok(comp_players) => {
                for mut player in comp_players {
                    if let Some(team_id) = player.team_id.clone() {
                        if team_id != "fa"
                            && team_id != "freeagent"
                            && !team_id.starts_with(&prefix)
                        {
                            player.team_id = Some(format!("{}-{team_id}", cid));
                        }
                    }
                    if player.morale == 0 {
                        player.morale = 68;
                    }
                    if player.condition == 0 {
                        player.condition = 100;
                    }
                    players.push(player);
                }
            }
            Err(err) => {
                log::debug!("[import] players unavailable for '{cid}' during rehydrate: {err}");
            }
        }
    }

    let mut seen = HashSet::new();
    players
        .into_iter()
        .filter(|player| seen.insert(player.id.clone()))
        .collect()
}

/// Load every non-legacy competition's teams and apply the SAME namespacing the
/// new-game world assembly does: prefix the team id with the competition id and
/// stamp `competition_id`. Deduplicated by team id.
fn collect_runtime_teams(app_handle: &tauri::AppHandle) -> Vec<Team> {
    let mut teams: Vec<Team> = Vec::new();

    for manifest in crate::commands::competitions::scan_competitions(app_handle)
        .iter()
        .filter(|manifest| !manifest.legacy)
    {
        let cid = &manifest.id;
        let prefix = format!("{}-", cid);
        match crate::commands::competitions::load_competition_teams(app_handle, manifest) {
            Ok(mut comp_teams) => {
                for team in &mut comp_teams {
                    if !team.id.starts_with(&prefix) {
                        team.id = format!("{prefix}{}", team.id);
                    }
                    team.competition_id = Some(cid.to_string());
                }
                teams.extend(comp_teams);
            }
            Err(err) => {
                log::debug!("[import] teams unavailable for '{cid}' during rehydrate: {err}");
            }
        }
    }

    let mut seen = HashSet::new();
    teams
        .into_iter()
        .filter(|team| seen.insert(team.id.clone()))
        .collect()
}

/// Append catalog players whose id is absent from the save. Existing players are
/// never mutated, so transfers and in-save edits are preserved — only genuinely
/// missing players (e.g. roster slots added to the source data after the save
/// was generated) are backfilled.
fn merge_missing_players(game: &mut Game, imported_players: Vec<Player>) -> usize {
    let mut existing_ids: HashSet<String> = game
        .players
        .iter()
        .map(|player| player.id.clone())
        .collect();
    let before = game.players.len();
    for player in imported_players {
        if existing_ids.insert(player.id.clone()) {
            game.players.push(player);
        }
    }
    game.players.len().saturating_sub(before)
}

/// Player ids with an in-save transfer record (user-made or simulated). Their
/// team assignment was changed during play and must be preserved against the
/// catalog. Includes players swapped in as part of a transfer.
fn transfer_protected_player_ids(game: &Game) -> HashSet<String> {
    let mut ids = HashSet::new();
    for entry in &game.transfer_history.entries {
        ids.insert(entry.player_id.clone());
        for included in &entry.included_players {
            ids.insert(included.player_id.clone());
        }
    }
    ids
}

/// Correct stale team assignments: when a player already exists in the save but
/// the catalog (updated source data) now places them on a different team, move
/// them to the catalog team — UNLESS they have an in-save transfer record, in
/// which case the in-game move (user or simulated) wins. Matched by id; only
/// reassigned when the catalog provides a concrete team_id (never released to
/// free agency on a catalog gap).
fn reassign_stale_player_teams(game: &mut Game, imported_players: &[Player]) -> usize {
    let protected = transfer_protected_player_ids(game);
    let catalog_team: std::collections::HashMap<&str, &str> = imported_players
        .iter()
        .filter_map(|player| {
            player
                .team_id
                .as_deref()
                .map(|team_id| (player.id.as_str(), team_id))
        })
        .collect();

    let mut reassigned = 0;
    for player in game.players.iter_mut() {
        if protected.contains(&player.id) {
            continue;
        }
        let Some(&catalog_tid) = catalog_team.get(player.id.as_str()) else {
            continue;
        };
        if player.team_id.as_deref() != Some(catalog_tid) {
            player.team_id = Some(catalog_tid.to_string());
            reassigned += 1;
        }
    }
    reassigned
}

/// Append catalog teams whose id is absent from the save. Existing teams are
/// never mutated — only teams missing entirely from the save are backfilled.
fn merge_missing_teams(game: &mut Game, imported_teams: Vec<Team>) -> usize {
    let mut existing_ids: HashSet<String> = game.teams.iter().map(|team| team.id.clone()).collect();
    let before = game.teams.len();
    for team in imported_teams {
        if existing_ids.insert(team.id.clone()) {
            game.teams.push(team);
        }
    }
    game.teams.len().saturating_sub(before)
}

fn merge_missing_staff(game: &mut Game, imported_staff: Vec<Staff>) -> usize {
    // Backfill display fields (nickname) onto staff that already exist in the
    // game/save but predate the field — older saves persisted nickname as "".
    let nickname_by_id: std::collections::HashMap<String, String> = imported_staff
        .iter()
        .filter(|member| !member.nickname.is_empty())
        .map(|member| (member.id.clone(), member.nickname.clone()))
        .collect();
    for member in game.staff.iter_mut() {
        if member.nickname.is_empty() {
            if let Some(nickname) = nickname_by_id.get(&member.id) {
                member.nickname = nickname.clone();
            }
        }
    }

    let mut existing_ids: HashSet<String> =
        game.staff.iter().map(|member| member.id.clone()).collect();
    let before = game.staff.len();
    for member in imported_staff {
        if existing_ids.insert(member.id.clone()) {
            game.staff.push(member);
        }
    }
    game.staff.len().saturating_sub(before)
}

/// Counts of catalog changes applied to a save during rehydration.
#[derive(Debug, Default, Clone, Copy)]
pub struct RehydrateCounts {
    pub teams: usize,
    pub players: usize,
    /// Existing players whose stale team assignment was corrected to the catalog.
    pub players_reassigned: usize,
    pub staff: usize,
}

impl RehydrateCounts {
    pub fn total(&self) -> usize {
        self.teams + self.players + self.players_reassigned + self.staff
    }
}

/// Reconcile a save with the imported catalog:
/// - backfill teams, players and staff missing from the save;
/// - correct stale player team assignments left behind by source-data updates,
///   while preserving in-game transfers (see `reassign_stale_player_teams`).
///
/// Existing entities are otherwise never mutated, so saves generated before a
/// data update pick up new roster slots and team moves without losing progress.
pub fn rehydrate_game_from_catalog(
    app_handle: &tauri::AppHandle,
    game: &mut Game,
) -> RehydrateCounts {
    let teams = merge_missing_teams(game, collect_runtime_teams(app_handle));

    let catalog_players = collect_runtime_players(app_handle);
    let players_reassigned = reassign_stale_player_teams(game, &catalog_players);
    let players = merge_missing_players(game, catalog_players);

    game_setup::apply_default_market_values(&mut game.players);

    let staff = merge_missing_staff(game, collect_runtime_staff(app_handle));

    RehydrateCounts {
        teams,
        players,
        players_reassigned,
        staff,
    }
}

fn rehydrate_active_game(
    app_handle: &tauri::AppHandle,
    state: &State<'_, StateManager>,
    sm_state: &State<'_, SaveManagerState>,
) -> Result<RehydrateCounts, String> {
    let Some(mut game) = state.get_game(|game| game.clone()) else {
        return Ok(RehydrateCounts::default());
    };
    let added = rehydrate_game_from_catalog(app_handle, &mut game);
    if added.total() == 0 {
        return Ok(added);
    }

    let save_id = state.get_save_id();
    state.set_game(game.clone());

    if let Some(save_id) = save_id {
        let mut sm = sm_state
            .0
            .lock()
            .map_err(|e| format!("save manager lock: {e}"))?;
        sm.save_game(&game, &save_id)?;
    }

    info!(
        "[cmd] auto_import_database: rehydrated active game with {} missing teams, {} players, {} reassigned players, {} staff",
        added.teams, added.players, added.players_reassigned, added.staff
    );
    Ok(added)
}

/// Decide the on-disk destination for a zip entry, or None to skip it.
fn destination_for(name: &str, data_dir: &Path, public_dir: &Path) -> Option<PathBuf> {
    let rel = safe_relative(name)?;
    let mut comps = rel.components();
    let first = comps.next()?.as_os_str().to_str()?;

    match first {
        "data" => {
            let rest: PathBuf = comps.collect();
            if rest.as_os_str().is_empty() {
                None
            } else {
                Some(data_dir.join(rest))
            }
        }
        "public" => {
            let sub = comps.next()?.as_os_str().to_str()?;
            if !PUBLIC_PHOTO_DIRS.contains(&sub) {
                return None;
            }
            let rest: PathBuf = comps.collect();
            if rest.as_os_str().is_empty() {
                None
            } else {
                Some(public_dir.join(sub).join(rest))
            }
        }
        _ => None,
    }
}

/// Reject path traversal: only allow normal, in-tree relative components.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let p = Path::new(path);
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

// ---------------------------------------------------------------------------
// Entity counting + catalog reading
// ---------------------------------------------------------------------------

fn rel_category(rel_name: &str) -> Option<&'static str> {
    let lower = rel_name.to_ascii_lowercase();
    if !lower.starts_with("data/") || lower.starts_with("data/draft/") {
        return None;
    }
    if lower.contains("/players/") || lower.ends_with("_players.json") {
        Some("players")
    } else if lower.contains("/teams/") || lower.ends_with("_teams.json") {
        Some("teams")
    } else if lower.contains("/staffs/")
        || lower.contains("/staff/")
        || lower.ends_with("_staffs.json")
        || lower.ends_with("_staff.json")
    {
        Some("staff")
    } else {
        None
    }
}

fn add_entity_counts(summary: &mut ImportSummary, rel_name: &str, bytes: &[u8]) {
    let Some(category) = rel_category(rel_name) else {
        return;
    };
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return;
    };
    let count = match category {
        "players" => entity_array_len(&value, &["players"]),
        "teams" => entity_array_len(&value, &["teams"]),
        "staff" => entity_array_len(&value, &["staff", "staffs"]),
        _ => 0,
    };
    match category {
        "players" => summary.player_count += count,
        "teams" => summary.team_count += count,
        "staff" => summary.staff_count += count,
        _ => {}
    }
}

fn entity_array_len(value: &Value, keys: &[&str]) -> usize {
    if let Some(items) = value.as_array() {
        return items.len();
    }
    keys.iter()
        .find_map(|key| value.get(*key)?.as_array().map(Vec::len))
        .unwrap_or(0)
}

fn entity_items<'a>(value: &'a Value, keys: &[&str]) -> Vec<&'a Value> {
    if let Some(items) = value.as_array() {
        return items.iter().collect();
    }
    keys.iter()
        .find_map(|key| value.get(*key)?.as_array())
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn json_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn catalog_summary(data_dir: &Path, public_dir: &Path) -> ImportSummary {
    current_catalog(data_dir, public_dir).summary
}

fn current_catalog(data_dir: &Path, public_dir: &Path) -> CatalogResponse {
    let mut catalog = CatalogResponse::default();
    let mut files = Vec::new();
    if walk_files(data_dir, &mut files).is_err() {
        return catalog;
    }

    for file in files {
        let Ok(rel) = file.strip_prefix(data_dir) else {
            continue;
        };
        let rel_name = format!("data/{}", rel.to_string_lossy().replace('\\', "/"));
        let Some(category) = rel_category(&rel_name) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        add_entity_counts(&mut catalog.summary, &rel_name, &bytes);
        catalog.summary.data_files += 1;
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };

        match category {
            "players" => {
                for item in entity_items(&value, &["players"]) {
                    let Some(id) = json_string(item, &["id"]) else {
                        continue;
                    };
                    let full_name =
                        json_string(item, &["full_name", "name"]).unwrap_or_else(|| id.clone());
                    let name = json_string(item, &["match_name", "nickname", "full_name", "name"])
                        .unwrap_or_else(|| full_name.clone());
                    catalog.players.push(CatalogPlayer {
                        id,
                        name,
                        full_name,
                        team_id: json_string(item, &["team_id"]),
                        nationality: json_string(item, &["nationality", "country"]),
                        role: json_string(item, &["role", "position", "lol_role"]),
                        image_url: json_string(item, &["profile_image_url", "image_url"]),
                    });
                }
            }
            "teams" => {
                for item in entity_items(&value, &["teams"]) {
                    let Some(id) = json_string(item, &["id"]) else {
                        continue;
                    };
                    catalog.teams.push(CatalogTeam {
                        name: json_string(item, &["name"]).unwrap_or_else(|| id.clone()),
                        short_name: json_string(item, &["short_name", "abbreviation"]),
                        country: json_string(item, &["country", "region"]),
                        competition_id: json_string(item, &["competition_id"]),
                        logo_url: json_string(item, &["logo_url"]),
                        id,
                    });
                }
            }
            "staff" => {
                for item in entity_items(&value, &["staff", "staffs"]) {
                    let Some(id) = json_string(item, &["id"]) else {
                        continue;
                    };
                    let first = json_string(item, &["first_name"]).unwrap_or_default();
                    let last = json_string(item, &["last_name"]).unwrap_or_default();
                    let full_name = format!("{first} {last}").trim().to_string();
                    let name = if full_name.is_empty() {
                        json_string(item, &["nickname", "name"]).unwrap_or_else(|| id.clone())
                    } else {
                        full_name
                    };
                    catalog.staff.push(CatalogStaff {
                        id,
                        name,
                        role: json_string(item, &["role"]),
                        team_id: json_string(item, &["team_id"]),
                        nationality: json_string(item, &["nationality", "country"]),
                        image_url: json_string(item, &["profile_image_url", "image_url"]),
                    });
                }
            }
            _ => {}
        }
    }

    catalog.players.sort_by(|a, b| a.id.cmp(&b.id));
    catalog.players.dedup_by(|a, b| a.id == b.id);
    catalog.teams.sort_by(|a, b| a.id.cmp(&b.id));
    catalog.teams.dedup_by(|a, b| a.id == b.id);
    catalog.staff.sort_by(|a, b| a.id.cmp(&b.id));
    catalog.staff.dedup_by(|a, b| a.id == b.id);

    catalog.summary.player_count = catalog.players.len();
    catalog.summary.team_count = catalog.teams.len();
    catalog.summary.staff_count = catalog.staff.len();
    catalog.summary.photo_files = count_files(public_dir);

    catalog.players.sort_by(|a, b| a.name.cmp(&b.name));
    catalog.teams.sort_by(|a, b| a.name.cmp(&b.name));
    catalog.staff.sort_by(|a, b| a.name.cmp(&b.name));
    catalog
}

fn count_files(root: &Path) -> usize {
    let mut files = Vec::new();
    if walk_files(root, &mut files).is_err() {
        return 0;
    }
    files.len()
}

fn walk_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(root).map_err(|e| format!("read_dir {root:?}: {e}"))? {
        let entry = entry.map_err(|e| format!("read_dir entry {root:?}: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    #[test]
    fn strip_tar_prefix_removes_first_component() {
        assert_eq!(
            strip_tar_prefix("olmanager-data-main/data/players.json"),
            Some("data/players.json")
        );
    }

    #[test]
    fn strip_tar_prefix_accepts_other_branch_names() {
        assert_eq!(
            strip_tar_prefix("olmanager-data-master/public/player-photos/p1.webp"),
            Some("public/player-photos/p1.webp")
        );
    }

    #[test]
    fn strip_tar_prefix_returns_none_for_top_level_file() {
        assert_eq!(strip_tar_prefix("README.md"), None);
    }

    #[test]
    fn import_tar_gz_extracts_data_and_public_files() -> Result<(), String> {
        let tmp = std::env::temp_dir().join(format!("olm-import-test-{}", timestamp_millis()));
        let data_dir = tmp.join("data");
        let public_dir = tmp.join("public");
        let source_dir = tmp.join("source");
        std::fs::create_dir_all(&data_dir).map_err(|e| format!("mkdir data: {e}"))?;
        std::fs::create_dir_all(&public_dir).map_err(|e| format!("mkdir public: {e}"))?;

        let players = source_dir.join("data/players/test_players.json");
        std::fs::create_dir_all(players.parent().unwrap())
            .map_err(|e| format!("mkdir players: {e}"))?;
        std::fs::write(&players, r#"{"players":[{"id":"p1","full_name":"Alice"}]}"#)
            .map_err(|e| format!("write players: {e}"))?;

        let photo = source_dir.join("public/player-photos/p1.webp");
        std::fs::create_dir_all(photo.parent().unwrap())
            .map_err(|e| format!("mkdir photo dir: {e}"))?;
        std::fs::write(&photo, b"photo").map_err(|e| format!("write photo: {e}"))?;

        let player_bytes = std::fs::read(&players).map_err(|e| format!("read players: {e}"))?;
        let photo_bytes = std::fs::read(&photo).map_err(|e| format!("read photo: {e}"))?;

        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(player_bytes.len() as u64);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                "olmanager-data-main/data/players/test_players.json",
                player_bytes.as_slice(),
            )
            .map_err(|e| format!("append players: {e}"))?;
        let mut header = tar::Header::new_gnu();
        header.set_size(photo_bytes.len() as u64);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                "olmanager-data-main/public/player-photos/p1.webp",
                photo_bytes.as_slice(),
            )
            .map_err(|e| format!("append photo: {e}"))?;
        let tar_bytes = builder
            .into_inner()
            .map_err(|e| format!("finish tar: {e}"))?;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&tar_bytes)
            .map_err(|e| format!("gzip: {e}"))?;
        let bytes = encoder.finish().map_err(|e| format!("gzip finish: {e}"))?;

        let mut progress = Vec::new();
        let summary =
            import_tar_gz_with_emit(&bytes, &data_dir, &public_dir, |p| progress.push(p))?;

        assert_eq!(summary.data_files, 1, "expected one data file");
        assert_eq!(summary.photo_files, 1, "expected one photo file");
        assert_eq!(summary.player_count, 1, "expected one player counted");
        assert!(
            data_dir.join("players/test_players.json").is_file(),
            "data file should be extracted"
        );
        assert!(
            public_dir.join("player-photos/p1.webp").is_file(),
            "photo file should be extracted"
        );
        assert!(!progress.is_empty(), "progress events should be emitted");

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn import_tar_gz_skips_unwanted_entries() -> Result<(), String> {
        let tmp = std::env::temp_dir().join(format!("olm-import-skip-test-{}", timestamp_millis()));
        let data_dir = tmp.join("data");
        let public_dir = tmp.join("public");
        std::fs::create_dir_all(&data_dir).map_err(|e| format!("mkdir data: {e}"))?;
        std::fs::create_dir_all(&public_dir).map_err(|e| format!("mkdir public: {e}"))?;

        let players = tmp.join("source/data/players/test_players.json");
        std::fs::create_dir_all(players.parent().unwrap())
            .map_err(|e| format!("mkdir players: {e}"))?;
        std::fs::write(&players, r#"{"players":[{"id":"p1","full_name":"Alice"}]}"#)
            .map_err(|e| format!("write players: {e}"))?;

        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                "olmanager-data-main/README.md",
                b"x".as_slice(),
            )
            .map_err(|e| format!("append readme: {e}"))?;
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                "olmanager-data-main/ignored/secret.txt",
                b"x".as_slice(),
            )
            .map_err(|e| format!("append ignored: {e}"))?;
        let player_bytes = std::fs::read(&players).map_err(|e| format!("read players: {e}"))?;
        let mut header = tar::Header::new_gnu();
        header.set_size(player_bytes.len() as u64);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                "olmanager-data-main/data/players/test_players.json",
                player_bytes.as_slice(),
            )
            .map_err(|e| format!("append players: {e}"))?;
        let tar_bytes = builder
            .into_inner()
            .map_err(|e| format!("finish tar: {e}"))?;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&tar_bytes)
            .map_err(|e| format!("gzip: {e}"))?;
        let bytes = encoder.finish().map_err(|e| format!("gzip finish: {e}"))?;

        let summary = import_tar_gz_with_emit(&bytes, &data_dir, &public_dir, |_| {})?;

        assert_eq!(summary.data_files, 1, "expected one data file");
        assert_eq!(summary.player_count, 1, "expected one player counted");
        assert_eq!(
            summary.skipped, 2,
            "expected README and ignored file to be skipped"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    #[test]
    fn write_import_cache_to_writes_to_given_path() -> Result<(), String> {
        let tmp = std::env::temp_dir().join(format!("olm-cache-test-{}", timestamp_millis()));
        std::fs::create_dir_all(&tmp).map_err(|e| format!("mkdir tmp: {e}"))?;
        let path = tmp.join("olmanager_export.zip");

        write_import_cache_to(path.clone(), b"zip bytes")?;

        assert!(path.is_file(), "cache file should exist at requested path");
        assert_eq!(
            std::fs::read(&path).map_err(|e| format!("read cache: {e}"))?,
            b"zip bytes"
        );

        write_import_cache_to(path.clone(), b"updated bytes")?;
        assert_eq!(
            std::fs::read(&path).map_err(|e| format!("read updated cache: {e}"))?,
            b"updated bytes"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }
}
