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
        NSLog("OpenUnleashed: game data %@, saves %@", gameDir.path, saveDir.path)
    }

    /// Both folders the engine needs are present.
    static var gameDataPresent: Bool {
        let fm = FileManager.default
        var isDir: ObjCBool = false
        return ["GameData", "FEData"].allSatisfy {
            fm.fileExists(atPath: gameDir.appendingPathComponent($0).path, isDirectory: &isDir) && isDir.boolValue
        }
    }

    private var gateWindow: UIWindow?

    /// UIKit is up (the engine's first `resumed`).
    func launched() {
        if Shell.gameDataPresent {
            ou_data_ready()
        } else {
            showGate(error: nil)
        }
    }

    /// The engine refused the data: back to the gate with the message.
    func engineError(_ message: String) {
        showGate(error: message)
    }

    /// Stage 3 placeholder for the Game Data screen (Stage 4): explains what is missing.
    private func showGate(error: String?) {
        let window = gateWindow ?? (SceneDelegate.scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: UIScreen.main.bounds))
        let vc = GateViewController()
        vc.errorText = error
        window.rootViewController = vc
        window.makeKeyAndVisible()
        gateWindow = window
    }
}

/// "Game data missing" screen.
final class GateViewController: UIViewController {
    var errorText: String?

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        let title = UILabel()
        title.text = "OpenUnleashed"
        title.font = .systemFont(ofSize: 72, weight: .bold)
        title.textColor = .white
        let body = UILabel()
        body.numberOfLines = 0
        body.textAlignment = .center
        body.textColor = UIColor(white: 0.8, alpha: 1)
        body.font = .systemFont(ofSize: 34)
        var text = "Game data missing.\n\nThe original GameData and FEData folders of your own copy are needed in\n\(Shell.gameDir.path)"
        if let e = errorText { text += "\n\nThe engine could not start:\n\(e)" }
        body.text = text
        let stack = UIStackView(arrangedSubviews: [title, body])
        stack.axis = .vertical
        stack.alignment = .center
        stack.spacing = 40
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            stack.widthAnchor.constraint(lessThanOrEqualTo: view.safeAreaLayoutGuide.widthAnchor),
        ])
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
}
