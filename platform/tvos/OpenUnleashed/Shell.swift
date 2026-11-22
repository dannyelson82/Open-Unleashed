// NFS Haven for Apple TV — app shell: storage layout and the start-up gate. GPL-3.0-or-later.
import UIKit

/// The shell's state. Everything runs on the main thread (winit's event loop is the main run loop).
final class Shell {
    static let shared = Shell()

    /// `Library/Caches` — the only sizeable writable storage on tvOS (may be purged by the system).
    static var caches: URL {
        FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
    }
    /// This game's folder: `Caches/games/porsche/` (one folder per game, `nfs-haven-plan.md` Phase 5).
    static var gameRoot: URL { caches.appendingPathComponent("games/porsche", isDirectory: true) }
    /// The downloaded game data (`GameData/`, `FEData/`, …: the chosen set, `porsche/original` or a mod set).
    static var gameDir: URL { gameRoot.appendingPathComponent("data", isDirectory: true) }
    /// The engine's data folder (`SaveData/`, settings, logs).
    static var saveDir: URL { gameRoot.appendingPathComponent("save", isDirectory: true) }

    /// Before the engine starts: tell it where the data and the saves live.
    static func prepareEnvironment() {
        try? FileManager.default.createDirectory(at: gameDir, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: saveDir, withIntermediateDirectories: true)
        setenv("NFS5_GAME_PATH", gameDir.path, 1)
        setenv("NFS5_DATA_DIR", saveDir.path, 1)
        // frame-time overlay (Stage 7.1) until the Extras menu has the switch: `-nfsh.fpsOverlay YES`
        if UserDefaults.standard.bool(forKey: "nfsh.fpsOverlay") { setenv("NFS_FPS_OVERLAY", "1", 1) }
        DisplayMode.exportRenderScale()
        // save sync with the same NFS Haven server the game data comes from (server-plan.md phase 2)
        if UserDefaults.standard.object(forKey: ServerSettings.addressKey) != nil || !ServerSettings.token.isEmpty {
            setenv("NFSH_SERVER", ServerSettings.address, 1)
            setenv("NFSH_TOKEN", ServerSettings.token, 1)
            setenv("NFSH_DEVICE", UIDevice.current.name, 1)
        }
        AudioSetup.configure()
        NSLog("OpenUnleashed: game data %@, saves %@", gameDir.path, saveDir.path)
    }

    /// The data is ready: a complete, verified download (or, for development only, data copied in by
    /// `scripts/build-tvos.sh sim --data` with the launch argument `-nfsh.trustLocalData YES`).
    static var gameDataReady: Bool {
        if GameStore.isReady { return true }
        guard UserDefaults.standard.bool(forKey: "nfsh.trustLocalData") else { return false }
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
        // no screen saver during a race / replay / intro; menus let the TV sleep as usual (§2.7)
        Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { _ in
            let want = ou_wants_no_sleep()
            if UIApplication.shared.isIdleTimerDisabled != want { UIApplication.shared.isIdleTimerDisabled = want }
        }
        if Shell.gameDataReady {
            guard GameStore.localManifest() != nil else {
                ou_data_ready()
                return
            }
            // the server may have newer data (a patch, an updated mod set): ask it, but never hold the game up
            // for long — offline or slow, the game starts with what's here
            Task { @MainActor in
                if let u = await Shell.updateAvailable() {
                    NSLog("OpenUnleashed: the server has updated game data (%d file(s), %lld bytes)", u.files, u.bytes)
                    self.showGameData(error: nil, update: u)
                } else {
                    ou_data_ready()
                }
            }
        } else {
            showGameData(error: nil)
        }
    }

    /// The server's manifest for the chosen set differs from the verified local copy: what an update would
    /// fetch. `nil` when up to date, without a server answer within ~4 s, or without a verified download.
    static func updateAvailable() async -> (files: Int, bytes: Int64, removed: Int)? {
        guard let local = GameStore.localManifest(), !ServerSettings.address.isEmpty else { return nil }
        return await withTaskGroup(of: (files: Int, bytes: Int64, removed: Int)?.self) { g in
            g.addTask {
                guard let (c, _) = try? await ServerClient.connect(address: ServerSettings.address, token: ServerSettings.token),
                      let (server, _) = try? await c.manifest(set: ServerSettings.dataSet) else { return nil }
                let d = GameStore.differences(local: local, server: server)
                return d.files + d.removed > 0 ? d : nil
            }
            g.addTask {
                try? await Task.sleep(nanoseconds: 4_000_000_000)
                return nil
            }
            let first = await g.next() ?? nil
            g.cancelAll()
            return first
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

    private func showGameData(error: String?, update: (files: Int, bytes: Int64, removed: Int)? = nil) {
        let window = gateWindow ?? (SceneDelegate.scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: UIScreen.main.bounds))
        let vc = GameDataViewController()
        vc.errorText = error
        vc.update = update
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
