import { lazy, Suspense, useEffect, useState } from "react";
import { BrowserRouter, Route, Routes, useLocation } from "react-router-dom";
import { useSettingsStore } from "@/store/settingsStore";
import { useBugReportStore } from "@/store/bugReportStore";
import { useUpdater } from "@/hooks/useUpdater";
import { invoke } from "@tauri-apps/api/core";
import UpdateModal from "@/ui-v2/_legacy/components/updater/UpdateModal";
import DashboardV2 from "./dashboard/DashboardV2";
import { TitleBarV2 } from "./components/TitleBarV2";
import ErrorBoundary from "./components/ErrorBoundary";
import FloatingBugButton from "@/ui-v2/_legacy/components/dashboard/FloatingBugButton";
import i18n from "@/i18n";

const MainMenu = lazy(() => import("@/ui-v2/_legacy/pages/MainMenu"));
const TeamSelectionV2 = lazy(() => import("@/ui-v2/pages/TeamSelectionV2"));
const MatchSimulation = lazy(() => import("@/ui-v2/_legacy/pages/MatchSimulation"));
const SettingsV2 = lazy(() => import("@/ui-v2/pages/SettingsV2"));

const SCALE_MAP: Record<string, string> = {
  xsmall: "9px",
  small: "14px",
  normal: "16px",
  large: "18px",
  xlarge: "20px",
};

const AUTO_CHECK_UPDATES = import.meta.env.PROD;

/// Maps frontend routes to Discord Rich Presence state keys.
/// Unmapped routes fall through to the `_` default in the backend.
const PATHNAME_TO_STATE_KEY: Record<string, string> = {
  "/dashboard": "dashboard",
  "/finanzas": "finances",
  "/finances": "finances",
  "/competiciones": "competitions",
  "/competitions": "competitions",
  "/match": "match",
  "/settings": "settings",
  "/": "main_menu",
  "/select-team": "main_menu",
};

const DASHBOARD_TAB_ROUTES = [
  "/finanzas",
  "/finances",
  "/competiciones",
  "/competitions",
];

function LazyFallback() {
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center bg-background">
      <div className="size-8 animate-spin rounded-full border-4 border-primary border-t-transparent" />
    </div>
  );
}

const DASHBOARD_ROUTES = ["/dashboard", ...DASHBOARD_TAB_ROUTES];

export function AppContent() {
  const location = useLocation();
  const setCurrentRoute = useBugReportStore((s) => s.setCurrentRoute);
  const setCurrentDashboardTab = useBugReportStore((s) => s.setCurrentDashboardTab);
  const showBugButton = ["/dashboard", "/match", "/select-team", ...DASHBOARD_TAB_ROUTES].includes(location.pathname);

  // Keep bug-report context in sync with the current route.
  // Dashboard tabs are only meaningful while on a dashboard route; clear the
  // value on other routes so bug reports from /match or /select-team do not
  // carry a stale dashboard tab from a previous visit.
  useEffect(() => {
    setCurrentRoute(location.pathname);
    if (!DASHBOARD_ROUTES.includes(location.pathname)) {
      setCurrentDashboardTab("");
    }
  }, [location.pathname, setCurrentRoute, setCurrentDashboardTab]);

  // Update Discord Rich Presence on route change
  useEffect(() => {
    const stateKey = PATHNAME_TO_STATE_KEY[location.pathname];
    if (stateKey) {
      invoke("update_discord_presence", { stateKey }).catch(() => {
        // Silently ignore — Discord may not be available
      });
    }
  }, [location.pathname]);

  return (
    <>
      <Suspense fallback={<LazyFallback />}>
        <Routes>
          <Route path="/" element={<MainMenu />} />
          <Route path="/select-team" element={<TeamSelectionV2 />} />
          <Route path="/dashboard" element={<DashboardV2 />} />
          <Route path="/finanzas" element={<DashboardV2 />} />
          <Route path="/finances" element={<DashboardV2 />} />
          <Route path="/competiciones" element={<DashboardV2 />} />
          <Route path="/competitions" element={<DashboardV2 />} />
          <Route path="/match" element={<MatchSimulation />} />
          <Route path="/settings" element={<SettingsV2 />} />
        </Routes>
      </Suspense>
      {showBugButton && <FloatingBugButton />}
    </>
  );
}

export default function AppV2() {
  const { settings, loaded, loadSettings } = useSettingsStore();
  const {
    updateAvailable,
    updateInfo,
    downloading,
    progress,
    error,
    dismissed,
    install,
    dismiss,
  } = useUpdater(AUTO_CHECK_UPDATES);

  const [ready, setReady] = useState(false);
  const [safeAreaTop, setSafeAreaTop] = useState(0);

  useEffect(() => {
    if (!loaded) loadSettings();
    if (loaded) setReady(true);
  }, [loaded, loadSettings]);

  useEffect(() => {
    if (!ready) return;
    const size = SCALE_MAP[settings.ui_scale] || "16px";
    document.documentElement.style.fontSize = size;
  }, [settings.ui_scale, ready]);

  useEffect(() => {
    if (!ready) return;
    if (settings.language && settings.language !== i18n.language) {
      i18n.changeLanguage(settings.language);
    }
  }, [ready, settings.language]);

  // --- Android: immersive mode + safe-area ---
  useEffect(() => {
    if (!ready) return;
    const isAndroid = /Android/i.test(window.navigator.userAgent);
    if (!isAndroid) return;

    let cancelled = false;

    const applyAndroidImmersive = async () => {
      if (cancelled) return;
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().setFullscreen(true);
      } catch {}
      try {
        if (document.fullscreenElement == null && document.documentElement.requestFullscreen) {
          await document.documentElement.requestFullscreen();
        }
      } catch {}
      try {
        if (screen.orientation?.lock) {
          await screen.orientation.lock("landscape");
        }
      } catch {}
    };

    void applyAndroidImmersive();

    // Measure the actual safe-area inset by reading env(safe-area-inset-top).
    // Chromium WebViews on Android may not expose env(), so fall back to 48 px
    // (typical Android status bar height in landscape).
    const measureSafeArea = () => {
      if (cancelled) return;
      const probe = document.createElement("div");
      probe.style.cssText =
        "position:fixed;top:env(safe-area-inset-top,48px);left:0;width:1px;height:1px;pointer-events:none;opacity:0;z-index:-1";
      document.body.appendChild(probe);
      // getComputedStyle resolves env() to its computed pixel value, or 48px
      const raw = parseFloat(getComputedStyle(probe).top);
      setSafeAreaTop(Number.isFinite(raw) && raw > 0 ? Math.round(raw) : 48);
      document.body.removeChild(probe);
    };
    // Wait for the WebView to settle before measuring.
    setTimeout(measureSafeArea, 400);

    const onVisible = () => {
      if (document.visibilityState === "visible") {
        void applyAndroidImmersive();
      }
    };

    window.addEventListener("focus", onVisible);
    document.addEventListener("visibilitychange", onVisible);

    return () => {
      cancelled = true;
      window.removeEventListener("focus", onVisible);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [ready]);

  useEffect(() => {
    const blockMouseBackForward = (event: MouseEvent) => {
      if (event.button === 3 || event.button === 4) {
        event.preventDefault();
        event.stopPropagation();
      }
    };

    const blockKeyboardHistoryShortcuts = (event: KeyboardEvent) => {
      if (
        event.key === "BrowserBack" ||
        event.key === "BrowserForward" ||
        event.code === "BrowserBack" ||
        event.code === "BrowserForward"
      ) {
        event.preventDefault();
        event.stopPropagation();
        return;
      }

      if (event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
        event.preventDefault();
        event.stopPropagation();
      }
    };

    window.addEventListener("mousedown", blockMouseBackForward, { capture: true });
    window.addEventListener("mouseup", blockMouseBackForward, { capture: true });
    window.addEventListener("keydown", blockKeyboardHistoryShortcuts, { capture: true });

    return () => {
      window.removeEventListener("mousedown", blockMouseBackForward, { capture: true });
      window.removeEventListener("mouseup", blockMouseBackForward, { capture: true });
      window.removeEventListener("keydown", blockKeyboardHistoryShortcuts, { capture: true });
    };
  }, []);

  // --- Discord Rich Presence lifecycle ---
  useEffect(() => {
    // Attempt to initialise the RPC client on mount (gracefully degrades
    // if Discord is not running).
    invoke("init_discord_rpc").catch(() => {});

    // Shut down the RPC client when the window is closed.
    const handleBeforeUnload = () => {
      invoke("shutdown_discord_rpc").catch(() => {});
    };
    window.addEventListener("beforeunload", handleBeforeUnload);

    return () => {
      window.removeEventListener("beforeunload", handleBeforeUnload);
      invoke("shutdown_discord_rpc").catch(() => {});
    };
  }, []);

  return (
    <div
      className="flex h-screen flex-col dark"
      style={safeAreaTop > 0 ? { paddingTop: safeAreaTop } : undefined}
    >
      <TitleBarV2 />
      <div className="flex min-h-0 flex-1 flex-col">
        <ErrorBoundary>
          <BrowserRouter>
            <AppContent />
            {updateAvailable && !dismissed && updateInfo && (
              <UpdateModal
                updateInfo={updateInfo}
                downloading={downloading}
                progress={progress}
                error={error}
                onInstall={install}
                onDismiss={dismiss}
              />
            )}
          </BrowserRouter>
        </ErrorBoundary>
      </div>
    </div>
  );
}
