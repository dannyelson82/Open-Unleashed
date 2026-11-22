// NFS Haven for Apple TV — NFS SE's saves on the NFS Haven server (nfs-haven-plan.md Phase 5.3). GPL-3.0-or-later.
//
// The game's home folder (`Caches/games/nfsse/save`) is kept in the server's `nfsse` namespace with the same rules
// as Porsche's sync (crates/open-unleashed/src/sync.rs): every PUT names the copy it replaces (`If-Match`), the
// server keeps versions, and a conflict never loses anything — the server's copy is kept beside the local one as
// `<file>.server` and the local copy (what was just played) becomes the current one, the server's earlier copy a
// version. Files: `config.dat`, `renfs.cfg`, `joystick.cfg`, tournaments `*.sav`, replays `*.rpl`.
// Pull before the game starts (at most ~5 s; offline it starts with what's here), push every 15 s when something
// changed and when the app goes to the background. Without a server address nothing happens.
import CryptoKit
import Foundation

enum SaveSync {
    static let namespace = "nfsse"
    private static let stateKey = "nfsh.sync.nfsse"
    private static var busy = false

    static var enabled: Bool { !ServerSettings.address.isEmpty }

    static func syncable(_ name: String) -> Bool {
        SaveMirror.nfsseSmall(name) || name.lowercased().hasSuffix(".rpl")
    }

    /// file → SHA-256 of the copy last agreed with the server
    private static var state: [String: String] {
        get { UserDefaults.standard.dictionary(forKey: stateKey) as? [String: String] ?? [:] }
        set { UserDefaults.standard.set(newValue, forKey: stateKey) }
    }

    private static func sha(_ d: Data) -> String { SHA256.hash(data: d).map { String(format: "%02x", $0) }.joined() }

    private static func localFiles() -> [String: URL] {
        let root = GameID.nfsse.saveDir
        var out: [String: URL] = [:]
        for name in (try? FileManager.default.contentsOfDirectory(atPath: root.path)) ?? [] where syncable(name) {
            out[name] = root.appendingPathComponent(name)
        }
        return out
    }

    private static func fileURL(_ c: ServerClient, _ name: String) -> URL {
        c.base.appendingPathComponent("v1/saves").appendingPathComponent(namespace).appendingPathComponent("files").appendingPathComponent(name)
    }

    private static func connect() async -> ServerClient? {
        guard enabled else { return nil }
        return try? await ServerClient.connect(address: ServerSettings.address, token: ServerSettings.token).0
    }

    /// Before the game starts: fetch what changed on the server (another Apple TV, the Mac). Gives up after `timeout`.
    static func pull(timeout: Double = 5) async {
        guard enabled else { return }
        await withTaskGroup(of: Void.self) { g in
            g.addTask { await pullNow() }
            g.addTask { try? await Task.sleep(nanoseconds: UInt64(timeout * 1e9)) }
            await g.next()
            g.cancelAll()
        }
    }

    private static func pullNow() async {
        guard let c = await connect() else { return }
        struct F: Decodable { let file: String; let sha256: String; let writer: String? }
        struct L: Decodable { let files: [F] }
        guard let list = try? await c.get("v1/saves/\(namespace)", as: L.self) else { return }
        var st = state
        var down = 0, kept = 0
        for f in list.files where syncable(f.file) && !f.file.contains("/") {
            let local = GameID.nfsse.saveDir.appendingPathComponent(f.file)
            let have = (try? Data(contentsOf: local)).map(sha)
            if have == f.sha256 { st[f.file] = f.sha256; continue }
            guard let data = await fetch(c, f.file), sha(data) == f.sha256 else { continue }
            if have == nil || have == st[f.file] {
                // not here, or unchanged here since the last sync: the server's is newer
                try? data.write(to: local, options: .atomic)
                st[f.file] = f.sha256
                down += 1
            } else {
                // changed in both places: keep both (the local copy is pushed next, the server keeps its version)
                try? data.write(to: local.appendingPathExtension("server"), options: .atomic)
                st[f.file] = f.sha256
                kept += 1
            }
        }
        state = st
        NSLog("NFS SE: save sync pulled %d file(s), %d conflict(s) kept as .server", down, kept)
    }

    private static func fetch(_ c: ServerClient, _ name: String) async -> Data? {
        var r = URLRequest(url: fileURL(c, name))
        r.setValue("Bearer \(c.token)", forHTTPHeaderField: "Authorization")
        r.setValue(ServerClient.deviceName, forHTTPHeaderField: "X-NFSH-Device")
        guard let (d, resp) = try? await c.session.data(for: r), (resp as? HTTPURLResponse)?.statusCode == 200 else { return nil }
        return d
    }

    /// Send local changes (in the background; one run at a time).
    static func pushSoon() {
        guard enabled, !busy else { return }
        let changed = localFiles().contains { name, u in
            (try? Data(contentsOf: u)).map(sha) != state[name]
        }
        guard changed else { return }
        busy = true
        Task {
            await pushNow()
            await MainActor.run { busy = false }
        }
    }

    private static func pushNow() async {
        guard let c = await connect() else { return }
        var st = state
        var up = 0
        for (name, u) in localFiles() {
            guard let d = try? Data(contentsOf: u) else { continue }
            let s = sha(d)
            if s == st[name] { continue }
            for attempt in 0..<2 {
                var r = URLRequest(url: fileURL(c, name))
                r.httpMethod = "PUT"
                r.httpBody = d
                r.setValue("Bearer \(c.token)", forHTTPHeaderField: "Authorization")
                r.setValue(ServerClient.deviceName, forHTTPHeaderField: "X-NFSH-Device")
                if let last = st[name] { r.setValue("\"\(last)\"", forHTTPHeaderField: "If-Match") }
                guard let (_, resp) = try? await c.session.data(for: r), let h = resp as? HTTPURLResponse else { break }
                if (200..<300).contains(h.statusCode) {
                    st[name] = s
                    up += 1
                    break
                }
                if h.statusCode == 412, attempt == 0 {
                    // the server has a copy we haven't seen: keep it beside ours, then make ours current
                    let etag = (h.value(forHTTPHeaderField: "ETag") ?? "").trimmingCharacters(in: CharacterSet(charactersIn: "\""))
                    if let theirs = await fetch(c, name) {
                        try? theirs.write(to: u.appendingPathExtension("server"), options: .atomic)
                    }
                    st[name] = etag.isEmpty ? nil : etag
                    continue
                }
                NSLog("NFS SE: save sync: %@ refused (%d)", name, h.statusCode)
                break
            }
        }
        state = st
        if up > 0 { NSLog("NFS SE: save sync sent %d file(s)", up) }
    }
}
