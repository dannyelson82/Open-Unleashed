// NFS Haven for Apple TV — the games the app can start (nfs-haven-plan.md, "Engine module interface", first step).
// GPL-3.0-or-later.
//
// One game per process: Porsche Unleashed runs on the OpenUnleashed engine (winit owns the app, so it can start only
// once), The Need for Speed SE on ReNFS through the host layer (platform/renfs-host). The launcher picks one; quitting
// a game ends the app, and the Home screen opens it at the launcher again.
import UIKit

enum GameID: String, CaseIterable {
    case porsche
    case nfsse

    var title: String {
        switch self {
        case .porsche: return "Porsche Unleashed"
        case .nfsse: return "The Need for Speed SE"
        }
    }

    var subtitle: String {
        switch self {
        case .porsche: return "2000 · OpenUnleashed engine"
        case .nfsse: return "1996 · ReNFS engine"
        }
    }

    /// Folders (and files) a usable copy of the game's data has, checked before it starts.
    var requiredItems: [String] {
        switch self {
        case .porsche: return ["GameData", "FEData"]
        case .nfsse: return ["FRONTEND", "GAMEDATA", "SIMDATA", "NFS_DOS.EXE"]
        }
    }

    /// `Caches/games/<id>/`: the downloaded data in `data/`, the game's own files (settings, saves) in `save/`.
    var root: URL { Shell.caches.appendingPathComponent("games/\(rawValue)", isDirectory: true) }
    var dataDir: URL { root.appendingPathComponent("data", isDirectory: true) }
    var saveDir: URL { root.appendingPathComponent("save", isDirectory: true) }

    static let lastKey = "nfsh.lastGame"
}
