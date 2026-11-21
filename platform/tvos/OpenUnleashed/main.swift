// OpenUnleashed for Apple TV — the app shell's entry point. GPL-3.0-or-later.
//
// winit's UIKit backend owns the process (it calls UIApplicationMain), so `main` only prepares the
// environment and hands over to the engine; the shell's UI runs from the `launched` callback.
import Foundation

Shell.prepareEnvironment()

var callbacks = OuShellCallbacks(
    launched: { Shell.shared.launched() },
    engine_error: { message in
        Shell.shared.engineError(message.map { String(cString: $0) } ?? "unknown error")
    },
    text_prompt: { title, current, maxLen, secure in
        Shell.shared.textPrompt(title: title.map { String(cString: $0) } ?? "",
                                current: current.map { String(cString: $0) } ?? "",
                                maxLen: Int(maxLen), secure: secure)
    }
)
exit(ou_main(&callbacks))
