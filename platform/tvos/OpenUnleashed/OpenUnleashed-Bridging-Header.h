// The engine's C ABI (crates/open-unleashed/src/tvos.rs). GPL-3.0-or-later.
#pragma once
#include <stdint.h>

typedef struct {
    /// UIKit is up: check the game data, show the Game Data screen or call ou_data_ready().
    void (*launched)(void);
    /// The engine could not start from the data reported ready (UTF-8 message).
    void (*engine_error)(const char *message);
} OuShellCallbacks;

/// Runs the engine's event loop (winit's UIKit backend calls UIApplicationMain; never returns).
int32_t ou_main(const OuShellCallbacks *callbacks);
/// Caches/game/ is complete and verified: start the game.
void ou_data_ready(void);
/// The app's window scene is connected: windows can be shown from now on.
void ou_scene_ready(void);
/// End the app (from the Game Data screen).
void ou_request_quit(void);
