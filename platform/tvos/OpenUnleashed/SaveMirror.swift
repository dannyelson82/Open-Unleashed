// OpenUnleashed for Apple TV — the save mirror (TVOS.md, Stage 4.4). GPL-3.0-or-later.
//
// Caches (where the engine keeps its saves) may be purged by tvOS; NSUserDefaults is the only truly persistent
// storage (~500 KB for an app). So the save files are mirrored into UserDefaults whenever they change (checked
// every 15 s and when the app goes to the background), if they fit, and restored from there before the engine
// starts when Caches lost them. Replays and photos are not mirrored. The OpenUnleashed server sync (Stage 9 item 7)
// builds on this: `SaveMirror.files()` is the seam.
import Foundation

enum SaveMirror {
    static let key = "ou.saves.mirror"
    static let stampKey = "ou.saves.mirrorStamp"
    /// Stay well under the ~500 KB UserDefaults budget (settings use some too).
    static let budget = 400_000

    /// The save files the engine wrote: `SaveData/*.sav`, `SaveData/nfs5.trk`, `SaveData/*.json`, `career.json`,
    /// `extras.json` (relative to Caches/save).
    static func files() -> [String: URL] {
        let root = Shell.saveDir
        var out: [String: URL] = [:]
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
        return out
    }

    /// Sizes + modification times: cheap change detection.
    private static func stamp(_ f: [String: URL]) -> String {
        f.keys.sorted().map { k in
            let a = try? FileManager.default.attributesOfItem(atPath: f[k]!.path)
            return "\(k):\((a?[.size] as? NSNumber)?.int64Value ?? 0):\((a?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0)"
        }.joined(separator: "|")
    }

    /// Mirror now if anything changed. Returns the mirrored size, or nil when it didn't fit / nothing changed.
    @discardableResult
    static func mirror() -> Int? {
        let f = files()
        let s = stamp(f)
        if s == UserDefaults.standard.string(forKey: stampKey) { return nil }
        var dict: [String: Data] = [:]
        var total = 0
        for (k, u) in f {
            guard let d = try? Data(contentsOf: u) else { continue }
            dict[k] = d
            total += d.count
        }
        if total > budget {
            NSLog("OpenUnleashed: saves are %d bytes — too big for the UserDefaults mirror (%d); not mirrored", total, budget)
            return nil
        }
        UserDefaults.standard.set(dict, forKey: key)
        UserDefaults.standard.set(s, forKey: stampKey)
        NSLog("OpenUnleashed: %d save files mirrored to UserDefaults (%d bytes)", dict.count, total)
        return total
    }

    /// Before the engine starts: put back every mirrored file Caches no longer has (never overwrites).
    static func restoreMissing() {
        guard let dict = UserDefaults.standard.dictionary(forKey: key) as? [String: Data] else { return }
        var restored = 0
        for (k, d) in dict {
            let u = Shell.saveDir.appendingPathComponent(k)
            if FileManager.default.fileExists(atPath: u.path) { continue }
            try? FileManager.default.createDirectory(at: u.deletingLastPathComponent(), withIntermediateDirectories: true)
            if (try? d.write(to: u)) != nil { restored += 1 }
        }
        if restored > 0 { NSLog("OpenUnleashed: %d save files restored from the UserDefaults mirror", restored) }
    }

    private static var timer: Timer?

    /// Mirror every 15 s while the app runs.
    static func startWatching() {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 15, repeats: true) { _ in mirror() }
    }
}
