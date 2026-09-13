let capturedWindowId = null;

const PREFIX = "POOKIE_FOCUS:";

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
        print(PREFIX + "CAPTURE_UNAVAILABLE");
        return;
    }

    capturedWindowId = windowId(active);

    print(
        PREFIX +
        "CAPTURED:" +
        capturedWindowId
    );
}

function restoreCapturedWindow() {
    if (!capturedWindowId) {
        print(PREFIX + "RESTORE_NO_TARGET");
        return;
    }

    const target = findWindow(capturedWindowId);

    if (!target) {
        print(
            PREFIX +
            "RESTORE_NOT_FOUND:" +
            capturedWindowId
        );

        return;
    }

    workspace.activeWindow = target;

    print(
        PREFIX +
        "RESTORE_REQUESTED:" +
        capturedWindowId
    );
}

function reportActiveWindow() {
    const active = workspace.activeWindow;

    if (!active) {
        print(PREFIX + "ACTIVE_NONE");
        return;
    }

    print(
        PREFIX +
        "ACTIVE:" +
        windowId(active)
    );
}

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
