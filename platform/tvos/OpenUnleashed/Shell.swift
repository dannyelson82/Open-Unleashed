// OpenUnleashed for Apple TV — app shell: storage layout and the start-up gate. GPL-3.0-or-later.
import UIKit

/// The shell's state. Everything runs on the main thread (winit's event loop is the main run loop).
final class Shell {
    static let shared = Shell()

    /// `Library/Caches` — the only sizeable writable storage on tvOS (may be purged by the system).
    static var caches: URL {
        FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
    }
    /// The downloaded original game data (`GameData/`, `FEData/`, …).
    static var gameDir: URL { caches.appendingPathComponent("game", isDirectory: true) }
    /// The engine's data folder (`SaveData/`, settings, logs).
    static var saveDir: URL { caches.appendingPathComponent("save", isDirectory: true) }

    /// Before the engine starts: tell it where the data and the saves live.
    static func prepareEnvironment() {
        try? FileManager.default.createDirectory(at: gameDir, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: saveDir, withIntermediateDirectories: true)
        setenv("NFS5_GAME_PATH", gameDir.path, 1)
        setenv("NFS5_DATA_DIR", saveDir.path, 1)
        // frame-time overlay (Stage 7.1) until the Extras menu has the switch: `-ou.fpsOverlay YES`
        if UserDefaults.standard.bool(forKey: "ou.fpsOverlay") { setenv("NFS_FPS_OVERLAY", "1", 1) }
        DisplayMode.exportRenderScale()
        AudioSetup.configure()
        NSLog("OpenUnleashed: game data %@, saves %@", gameDir.path, saveDir.path)
    }

    /// The data is ready: a complete, verified download (or, for development only, data copied in by
    /// `scripts/build-tvos.sh sim --data` with the launch argument `-ou.trustLocalData YES`).
    static var gameDataReady: Bool {
        if GameStore.isReady { return true }
        guard UserDefaults.standard.bool(forKey: "ou.trustLocalData") else { return false }
        var isDir: ObjCBool = false
        return ["GameData", "FEData"].allSatisfy {
            FileManager.default.fileExists(atPath: gameDir.appendingPathComponent($0).path, isDirectory: &isDir) && isDir.boolValue
        }
    }

    private var gateWindow: UIWindow?

    /// UIKit is up and the scene is connected (the engine's first `resumed`).
    func launched() {
        SaveMirror.restoreMissing()
        SaveMirror.startWatching()
        if Shell.gameDataReady {
            ou_data_ready()
        } else {
            showGameData(error: nil)
        }
    }

    /// The engine refused the data: back to the Game Data screen with the message.
    func engineError(_ message: String) {
        showGameData(error: message)
    }

    /// The engine wants text (profile name, address, replay name…): the tvOS keyboard in a prompt over the game.
    func textPrompt(title: String, current: String, maxLen: Int, secure: Bool) {
        guard let host = SceneDelegate.scene?.windows.first(where: { $0.isKeyWindow })?.rootViewController
                ?? SceneDelegate.scene?.windows.first?.rootViewController else {
            ou_text_entered(nil)
            return
        }
        let a = UIAlertController(title: title, message: maxLen > 0 ? "Up to \(maxLen) characters." : nil, preferredStyle: .alert)
        a.addTextField { t in
            t.text = current
            t.isSecureTextEntry = secure
            t.autocapitalizationType = .none
            t.autocorrectionType = .no
        }
        a.addAction(UIAlertAction(title: "OK", style: .default) { _ in
            let text = String((a.textFields?.first?.text ?? "").prefix(maxLen > 0 ? maxLen : 256))
            text.withCString { ou_text_entered($0) }
        })
        a.addAction(UIAlertAction(title: "Cancel", style: .cancel) { _ in ou_text_entered(nil) })
        host.present(a, animated: true)
    }

    private func showGameData(error: String?) {
        let window = gateWindow ?? (SceneDelegate.scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: UIScreen.main.bounds))
        let vc = GameDataViewController()
        vc.errorText = error
        vc.onReady = { [weak self] in
            // hand the screen to the engine's window
            self?.gateWindow?.isHidden = true
            self?.gateWindow = nil
            ou_data_ready()
        }
        window.rootViewController = vc
        window.makeKeyAndVisible()
        gateWindow = window
    }
}

/// The UIScene life cycle (required by the tvOS 27 SDK). winit 0.30 predates scenes: the shell owns the
/// scene, the patched winit attaches its window to it, and the engine starts only once it's connected.
final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    static var scene: UIWindowScene?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions) {
        SceneDelegate.scene = scene as? UIWindowScene
        ou_scene_ready()
    }

    func sceneWillResignActive(_ scene: UIScene) {
        // the race pauses, the audio stream stops (TVOS.md Stage 6.3)
        ou_app_background()
    }

    func sceneDidBecomeActive(_ scene: UIScene) {
        ou_app_foreground()
        for w in (scene as? UIWindowScene)?.windows ?? [] { DisplayMode.request60Hz(on: w) }
    }

    func sceneDidEnterBackground(_ scene: UIScene) {
        SaveMirror.mirror()
    }
}
