let capturedWindowId = null;

const POOKIE_SERVICE =
"io.github.riyanj220.PookiePaste";

const POOKIE_PATH =
"/io/github/riyanj220/PookiePaste/Focus";

const POOKIE_INTERFACE =
"io.github.riyanj220.PookiePaste.Focus";

function windowId(window) {
    if (!window) {
        return null;
    }

    return window.internalId.toString();
}

function findWindow(id) {
    const windows = workspace.stackingOrder;

    for (let i = 0; i < windows.length; i++) {
        const window = windows[i];

        if (windowId(window) === id) {
            return window;
        }
    }

    return null;
}

function captureActiveWindow() {
    const active = workspace.activeWindow;

    if (!active) {
        callDBus(
            POOKIE_SERVICE,
            POOKIE_PATH,
            POOKIE_INTERFACE,
            "CaptureUnavailable"
        );

        return;
    }

    capturedWindowId = windowId(active);

    callDBus(
        POOKIE_SERVICE,
        POOKIE_PATH,
        POOKIE_INTERFACE,
        "Captured",
        capturedWindowId
    );
}

function restoreCapturedWindow() {
    if (!capturedWindowId) {
        callDBus(
            POOKIE_SERVICE,
            POOKIE_PATH,
            POOKIE_INTERFACE,
            "RestoreNoTarget"
        );

        return;
    }

    const target = findWindow(capturedWindowId);

    if (!target) {
        callDBus(
            POOKIE_SERVICE,
            POOKIE_PATH,
            POOKIE_INTERFACE,
            "RestoreNotFound",
            capturedWindowId
        );

        return;
    }

    /*
     * This is the exact activation mechanism that was
     * verified on Plasma 6.7.4.
     */
    workspace.activeWindow = target;

    callDBus(
        POOKIE_SERVICE,
        POOKIE_PATH,
        POOKIE_INTERFACE,
        "RestoreRequested",
        capturedWindowId
    );
}

function reportActiveWindow() {
    const active = workspace.activeWindow;

    if (!active) {
        callDBus(
            POOKIE_SERVICE,
            POOKIE_PATH,
            POOKIE_INTERFACE,
            "ActiveNone"
        );

        return;
    }

    callDBus(
        POOKIE_SERVICE,
        POOKIE_PATH,
        POOKIE_INTERFACE,
        "Active",
        windowId(active)
    );
}

/*
 * Final production action IDs.
 *
 * These IDs must remain stable after release.
 * No user-visible default shortcuts are assigned.
 */
registerShortcut(
    "PookiePasteFocusCapture",
    "Pookie Paste: Capture Focus",
    "",
    captureActiveWindow
);

registerShortcut(
    "PookiePasteFocusRestore",
    "Pookie Paste: Restore Focus",
    "",
    restoreCapturedWindow
);

registerShortcut(
    "PookiePasteFocusActive",
    "Pookie Paste: Report Active Focus",
    "",
    reportActiveWindow
);
