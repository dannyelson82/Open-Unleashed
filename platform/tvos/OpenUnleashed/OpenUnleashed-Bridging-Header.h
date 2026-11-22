// The engine's C ABI (crates/open-unleashed/src/tvos.rs). GPL-3.0-or-later.
#pragma once
#include <stdbool.h>
#include <stdint.h>

typedef struct {
    /// UIKit is up: check the game data, show the Game Data screen or call ou_data_ready().
    void (*launched)(void);
    /// The engine could not start from the data reported ready (UTF-8 message).
    void (*engine_error)(const char *message);
    /// A field wants text: show the on-screen keyboard; answer with ou_text_entered (NULL = cancelled).
    void (*text_prompt)(const char *title, const char *current, uint32_t max_len, bool secure);
} OuShellCallbacks;

/// Runs the engine's event loop (winit's UIKit backend calls UIApplicationMain; never returns).
int32_t ou_main(const OuShellCallbacks *callbacks);
/// Caches/game/ is complete and verified: start the game.
void ou_data_ready(void);
/// The app's window scene is connected: windows can be shown from now on.
void ou_scene_ready(void);
/// The on-screen keyboard closed: the text (UTF-8), or NULL when cancelled.
void ou_text_entered(const char *text);
/// App life cycle (from the scene delegate).
void ou_app_background(void);
void ou_app_foreground(void);
/// A race / replay / intro is running: keep the screen saver off.
bool ou_wants_no_sleep(void);
/// End the app (from the Game Data screen).
void ou_request_quit(void);
