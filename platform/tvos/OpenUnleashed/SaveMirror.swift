// NFS Haven for Apple TV — the save mirror (TVOS.md, Stage 4.4), one per game. GPL-3.0-or-later.
//
// Caches (where the games keep their saves) may be purged by tvOS; NSUserDefaults is the only truly persistent
// storage (~500 KB for the whole app). So each game's small save files are mirrored into UserDefaults whenever they
// change (checked every 15 s and when the app goes to the background), if they fit the game's share, and restored
// from there before a game starts when Caches lost them. Replays and photos are not mirrored (the server keeps NFS
// SE's replays: SaveSync). Porsche's server sync is the engine's own (crates/open-unleashed/src/sync.rs).
import Foundation

enum SaveMirror {
    /// Porsche keeps the keys it always had.
    static func key(_ g: GameID) -> String { g == .porsche ? "nfsh.saves.mirror" : "nfsh.saves.mirror.\(g.rawValue)" }
    static func stampKey(_ g: GameID) -> String { g == .porsche ? "nfsh.saves.mirrorStamp" : "nfsh.saves.mirrorStamp.\(g.rawValue)" }
    /// Each game's share of the ~500 KB UserDefaults budget (settings use some too).
    static func budget(_ g: GameID) -> Int { g == .porsche ? 400_000 : 80_000 }

    /// The save files a game wrote, relative to its save folder.
    /// Porsche: `SaveData/*.sav`, `SaveData/nfs5.trk`, `SaveData/*.json`, `career.json`, `extras.json`, `settings.json`.
    /// NFS SE (ReNFS's home folder): `config.dat` (settings, records), `renfs.cfg`, `joystick.cfg`, tournaments `*.sav`.
    static func files(_ g: GameID) -> [String: URL] {
        let root = g.saveDir
        var out: [String: URL] = [:]
        switch g {
        case .porsche:
            for top in ["career.json", "extras.json", "settings.json"] {
                let u = root.appendingPathComponent(top)
                if FileManager.default.fileExists(atPath: u.path) { out[top] = u }
            }
            let sd = root.appendingPathComponent("SaveData")
            for name in (try? FileManager.default.contentsOfDirectory(atPath: sd.path)) ?? [] {
                let l = name.lowercased()
                if l.hasSuffix(".sav") || l == "nfs5.trk" || l.hasSuffix(".json") {
                    out["SaveData/\(name)"] = sd.appendingPathComponent(name)
                }
            }
        case .nfsse:
            for name in (try? FileManager.default.contentsOfDirectory(atPath: root.path)) ?? [] where nfsseSmall(name) {
                out[name] = root.appendingPathComponent(name)
            }
        }
        return out
    }

    static func nfsseSmall(_ name: String) -> Bool {
        let l = name.lowercased()
        return ["config.dat", "renfs.cfg", "joystick.cfg"].contains(l) || l.hasSuffix(".sav")
    }

    /// Sizes + modification times: cheap change detection.
    private static func stamp(_ f: [String: URL]) -> String {
        f.keys.sorted().map { k in
            let a = try? FileManager.default.attributesOfItem(atPath: f[k]!.path)
            return "\(k):\((a?[.size] as? NSNumber)?.int64Value ?? 0):\((a?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0)"
        }.joined(separator: "|")
    }

    /// Mirror `g`'s saves now if anything changed. Returns the mirrored size, or nil when it didn't fit / nothing
    /// changed. A game with no save files at all leaves its mirror alone (a purged Caches must never empty it).
    @discardableResult
    static func mirror(_ g: GameID) -> Int? {
        let f = files(g)
        if f.isEmpty { return nil }
        let s = stamp(f)
        if s == UserDefaults.standard.string(forKey: stampKey(g)) { return nil }
        var dict: [String: Data] = [:]
        var total = 0
        for (k, u) in f {
            guard let d = try? Data(contentsOf: u) else { continue }
            dict[k] = d
            total += d.count
        }
        if total > budget(g) {
            NSLog("OpenUnleashed: %@ saves are %d bytes — too big for the UserDefaults mirror (%d); not mirrored", g.rawValue, total, budget(g))
            return nil
        }
        UserDefaults.standard.set(dict, forKey: key(g))
        UserDefaults.standard.set(s, forKey: stampKey(g))
        NSLog("OpenUnleashed: %d %@ save files mirrored to UserDefaults (%d bytes)", dict.count, g.rawValue, total)
        return total
    }

    /// Before a game starts: put back every mirrored file Caches no longer has (never overwrites).
    static func restoreMissing(_ g: GameID) {
        guard let dict = UserDefaults.standard.dictionary(forKey: key(g)) as? [String: Data] else { return }
        var restored = 0
        for (k, d) in dict {
            let u = g.saveDir.appendingPathComponent(k)
            if FileManager.default.fileExists(atPath: u.path) { continue }
            try? FileManager.default.createDirectory(at: u.deletingLastPathComponent(), withIntermediateDirectories: true)
            if (try? d.write(to: u)) != nil { restored += 1 }
        }
        if restored > 0 { NSLog("OpenUnleashed: %d %@ save files restored from the UserDefaults mirror", restored, g.rawValue) }
    }

    private static var timer: Timer?

    /// Mirror the running game's saves every 15 s (and, for NFS SE, send changes to the server).
    static func startWatching(_ g: GameID) {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { _ in
            mirror(g)
            if g == .nfsse { SaveSync.pushSoon() }
        }
    }
}
