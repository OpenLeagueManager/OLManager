package com.openleaguemanager.olmanager

import android.content.res.AssetManager
import android.os.Bundle
import android.util.Log
import androidx.activity.enableEdgeToEdge
import java.io.File
import java.io.IOException
import java.io.InputStream

class MainActivity : TauriActivity() {
  companion object {
    private const val TAG = "OLManager.Seed"
    private const val ASSET_DATA_ROOT = "data"
    private const val DATA_DIR_NAME = "data"
    private const val COMPETITIONS_DIR = "competitions"
    private const val MANIFEST_FILE = "manifest.json"
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    seedBundledDataIfNeeded()
    super.onCreate(savedInstanceState)
  }

  /**
   * Copies bundled APK assets under the `data` asset directory into the app's writable
   * files directory (`filesDir/data`) when no valid competition data is present.
   *
   * On Android, Rust's `std::fs` cannot traverse APK assets as regular
   * directories, so we must materialise them as real files before Tauri setup
   * runs and the Rust side looks for `app_data_dir/data/competitions`.
   */
  private fun seedBundledDataIfNeeded() {
    val filesDataDir = File(filesDir, DATA_DIR_NAME)
    val competitionsDir = File(filesDataDir, COMPETITIONS_DIR)

    if (hasCompetitionManifest(competitionsDir)) {
      Log.i(TAG, "Writable data directory already contains competition manifests; skipping seed")
      return
    }

    try {
      copyAssetsRecursive(ASSET_DATA_ROOT, filesDataDir)
      Log.i(TAG, "Seeded bundled data into ${filesDataDir.absolutePath}")
    } catch (e: IOException) {
      Log.e(TAG, "Failed to seed bundled data into ${filesDataDir.absolutePath}", e)
    } catch (e: Exception) {
      Log.e(TAG, "Unexpected error while seeding bundled data", e)
    }
  }

  private fun hasCompetitionManifest(competitionsDir: File): Boolean {
    if (!competitionsDir.isDirectory) return false
    return competitionsDir.listFiles()?.any { child ->
      child.isDirectory && File(child, MANIFEST_FILE).isFile
    } ?: false
  }

  @Throws(IOException::class)
  private fun copyAssetsRecursive(assetPath: String, destinationDir: File) {
    val assets: AssetManager = assets
    val entries = assets.list(assetPath) ?: emptyArray()

    if (entries.isEmpty()) {
      // `list()` returns an empty array for files (and genuinely empty dirs).
      copyAssetFile(assetPath, destinationDir)
      return
    }

    if (!destinationDir.exists() && !destinationDir.mkdirs()) {
      throw IOException("Failed to create directory: ${destinationDir.absolutePath}")
    }

    for (entry in entries) {
      val sourcePath = if (assetPath.isEmpty()) entry else "$assetPath/$entry"
      val destPath = File(destinationDir, entry)
      copyAssetsRecursive(sourcePath, destPath)
    }
  }

  @Throws(IOException::class)
  private fun copyAssetFile(assetPath: String, destinationFile: File) {
    val parent = destinationFile.parentFile
    if (parent != null && !parent.exists() && !parent.mkdirs()) {
      throw IOException("Failed to create parent directory: ${parent.absolutePath}")
    }

    assets.open(assetPath).use { input: InputStream ->
      destinationFile.outputStream().use { output ->
        input.copyTo(output)
      }
    }
  }
}
