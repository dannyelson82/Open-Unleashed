// NFS Haven for Apple TV — the downloaded game data in Caches/games/porsche/data (TVOS.md, Stage 4). GPL-3.0-or-later.
//
// Launch check (cheap, no hashing): the manifest the last complete download used is kept beside the data
// (`Caches/games/porsche/data/.nfsh/manifest.json`) with a "verified" marker; every listed file must exist with its size.
// A download resumes partial files with HTTP Range, verifies each file's SHA-256 (retrying up to 3 times),
// and only then marks the set verified. tvOS may purge Caches at any time — then the screen simply comes back.
import CryptoKit
import Foundation

struct DownloadProgress {
    var doneBytes: Int64 = 0
    var totalBytes: Int64 = 0
    var filesDone = 0
    var filesTotal = 0
    var currentFile = ""
    var currentDone: Int64 = 0
    var currentSize: Int64 = 0
    var bytesPerSecond: Double = 0
}

enum StoreError: LocalizedError {
    case diskFull(need: Int64, free: Int64)
    case hashMismatch(String)
    case cancelled

    var errorDescription: String? {
        switch self {
        case .diskFull(let need, let free):
            return String(format: "Not enough space: %.0f MB more needed, %.0f MB free. Delete other apps' data or choose a smaller set.", Double(need) / 1e6, Double(free) / 1e6)
        case .hashMismatch(let p): return "\(p) arrived damaged three times (SHA-256 mismatch). Check the server's copy."
        case .cancelled: return "Download cancelled."
        }
    }
}

enum GameStore {
    static var root: URL { Shell.gameDir }
    static var meta: URL { root.appendingPathComponent(".nfsh", isDirectory: true) }
    static var manifestFile: URL { meta.appendingPathComponent("manifest.json") }
    static var verifiedFile: URL { meta.appendingPathComponent("verified") }
    static var partsDir: URL { meta.appendingPathComponent("parts", isDirectory: true) }

    /// The verified manifest, if the last download completed.
    static func localManifest() -> Manifest? {
        guard FileManager.default.fileExists(atPath: verifiedFile.path),
              let d = try? Data(contentsOf: manifestFile) else { return nil }
        return try? JSONDecoder().decode(Manifest.self, from: d)
    }

    /// What a download of `server` would change in the verified local copy: files whose content differs or that
    /// are new (count and bytes to fetch), plus files the server no longer lists. All zero = up to date.
    static func differences(local: Manifest, server: Manifest) -> (files: Int, bytes: Int64, removed: Int) {
        let have = Dictionary(local.files.map { ($0.path, $0.sha256) }, uniquingKeysWith: { a, _ in a })
        let fetch = server.files.filter { have[$0.path] != $0.sha256 }
        let wanted = Set(server.files.map { $0.path })
        return (fetch.count, fetch.reduce(0) { $0 + $1.size }, local.files.filter { !wanted.contains($0.path) }.count)
    }

    /// Launch check: a verified manifest whose files are all present with the right sizes.
    /// Returns the missing/changed paths (empty = ready), or nil when there's no verified download at all.
    static func check() -> [String]? {
        guard let m = localManifest() else { return nil }
        let fm = FileManager.default
        return m.files.compactMap { e in
            let p = root.appendingPathComponent(e.path).path
            let size = (try? fm.attributesOfItem(atPath: p)[.size] as? NSNumber)?.int64Value
            return size == e.size ? nil : e.path
        }
    }

    static var isReady: Bool { check()?.isEmpty == true }

    static func freeSpace() -> Int64 {
        // tvOS has no "important usage" capacity; the plain free space is the bound
        let v = try? root.resourceValues(forKeys: [.volumeAvailableCapacityKey])
        return v?.volumeAvailableCapacity.map(Int64.init) ?? Int64.max
    }

    static func sha256(of url: URL) -> String? {
        guard let h = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? h.close() }
        var hasher = SHA256()
        while let chunk = try? h.read(upToCount: 1 << 20), !chunk.isEmpty { hasher.update(data: chunk) }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

/// One file download with Range resume, streaming into a `.part` file and hashing as it goes.
final class FileDownload: NSObject, URLSessionDataDelegate {
    private var handle: FileHandle?
    private var hasher = SHA256()
    private var continuation: CheckedContinuation<String, Error>?
    private let onBytes: (Int64) -> Void
    private var status = 0

    init(onBytes: @escaping (Int64) -> Void) { self.onBytes = onBytes }

    /// Downloads `url` into `part`, resuming from its current size; returns the whole file's SHA-256.
    func run(_ req: URLRequest, part: URL) async throws -> String {
        let fm = FileManager.default
        if !fm.fileExists(atPath: part.path) { fm.createFile(atPath: part.path, contents: nil) }
        let have = (try? fm.attributesOfItem(atPath: part.path)[.size] as? NSNumber)?.int64Value ?? 0
        // hash what's already there (resume)
        if have > 0, let h = try? FileHandle(forReadingFrom: part) {
            while let c = try? h.read(upToCount: 1 << 20), !c.isEmpty { hasher.update(data: c) }
            try? h.close()
        }
        var req = req
        if have > 0 { req.setValue("bytes=\(have)-", forHTTPHeaderField: "Range") }
        handle = try FileHandle(forWritingTo: part)
        try handle?.seekToEnd()
        let session = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
        defer { session.finishTasksAndInvalidate() }
        onBytes(have)
        return try await withCheckedThrowingContinuation { (c: CheckedContinuation<String, Error>) in
            continuation = c
            session.dataTask(with: req).resume()
        }
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse, completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        status = (response as? HTTPURLResponse)?.statusCode ?? 0
        if status == 200 {
            // the server ignored the range (or the part was stale): start over
            try? handle?.truncate(atOffset: 0)
            hasher = SHA256()
        }
        completionHandler((status == 200 || status == 206) ? .allow : .cancel)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        do {
            try handle?.write(contentsOf: data)
            hasher.update(data: data)
            onBytes(Int64(data.count))
        } catch {
            dataTask.cancel()
            finish(.failure(error))
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        try? handle?.close()
        handle = nil
        if let e = error {
            finish(.failure(status == 401 ? ServerError.unauthorized : (status >= 400 ? ServerError.http(status, "download failed") : e)))
        } else if status == 200 || status == 206 {
            finish(.success(hasher.finalize().map { String(format: "%02x", $0) }.joined()))
        } else if status == 416 {
            // the part already holds the whole file (or more): let the caller verify/restart
            finish(.success(hasher.finalize().map { String(format: "%02x", $0) }.joined()))
        } else {
            finish(.failure(ServerError.http(status, "download failed")))
        }
    }

    private func finish(_ r: Result<String, Error>) {
        guard let c = continuation else { return }
        continuation = nil
        c.resume(with: r)
    }
}

/// Brings Caches/games/porsche/data in line with a set's manifest.
final class GameDownloader {
    let client: ServerClient
    let set: String
    private(set) var cancelled = false
    var onProgress: ((DownloadProgress) -> Void)?

    init(client: ServerClient, set: String) {
        self.client = client
        self.set = set
    }

    func cancel() { cancelled = true }

    func run() async throws {
        let fm = FileManager.default
        try fm.createDirectory(at: GameStore.partsDir, withIntermediateDirectories: true)
        let (manifest, raw) = try await client.manifest(set: set)
        // a new download invalidates the old verified marker until it completes
        try? fm.removeItem(at: GameStore.verifiedFile)

        // what's already right (by size; hashes for files this run didn't fetch are trusted only if the
        // previous manifest — verified — listed the same hash)
        let previous: [String: ManifestEntry] = {
            guard let d = try? Data(contentsOf: GameStore.manifestFile), let m = try? JSONDecoder().decode(Manifest.self, from: d) else { return [:] }
            return Dictionary(m.files.map { ($0.path, $0) }, uniquingKeysWith: { a, _ in a })
        }()
        let previousVerified = UserDefaults.standard.string(forKey: "nfsh.data.verifiedHashesOf") == "set"
        var todo: [ManifestEntry] = []
        for e in manifest.files {
            let dest = GameStore.root.appendingPathComponent(e.path)
            let size = (try? fm.attributesOfItem(atPath: dest.path)[.size] as? NSNumber)?.int64Value
            if size == e.size, previousVerified, previous[e.path]?.sha256 == e.sha256 { continue }
            if size == e.size, GameStore.sha256(of: dest) == e.sha256 { continue }
            todo.append(e)
        }
        try raw.write(to: GameStore.manifestFile, options: .atomic)
        UserDefaults.standard.set("partial", forKey: "nfsh.data.verifiedHashesOf")

        var p = DownloadProgress()
        p.filesTotal = todo.count
        p.totalBytes = todo.reduce(0) { $0 + $1.size }
        let partBytes = todo.reduce(Int64(0)) { acc, e in
            acc + ((try? fm.attributesOfItem(atPath: GameStore.partsDir.appendingPathComponent(e.sha256).path)[.size] as? NSNumber)?.int64Value ?? 0)
        }
        let free = GameStore.freeSpace()
        if p.totalBytes - partBytes > free - 50_000_000 {
            throw StoreError.diskFull(need: p.totalBytes - partBytes, free: free)
        }
        onProgress?(p)

        let started = Date()
        var sessionBytes: Int64 = 0
        for e in todo {
            if cancelled { throw StoreError.cancelled }
            p.currentFile = e.path
            p.currentSize = e.size
            p.currentDone = 0
            let part = GameStore.partsDir.appendingPathComponent(e.sha256)
            var attempt = 0
            while true {
                attempt += 1
                let baseDone = p.doneBytes
                var fileBytes: Int64 = 0
                let dl = FileDownload { [weak self] n in
                    fileBytes += n
                    sessionBytes += n
                    p.currentDone = fileBytes
                    p.doneBytes = baseDone + fileBytes
                    p.bytesPerSecond = Double(sessionBytes) / max(0.001, Date().timeIntervalSince(started))
                    self?.onProgress?(p)
                }
                var req = URLRequest(url: client.fileURL(set: set, path: e.path))
                if !client.token.isEmpty { req.setValue("Bearer \(client.token)", forHTTPHeaderField: "Authorization") }
                req.setValue(ServerClient.deviceName, forHTTPHeaderField: "X-NFSH-Device")
                req.setValue("\"\(e.sha256)\"", forHTTPHeaderField: "If-Range")
                let sha: String
                do {
                    sha = try await dl.run(req, part: part)
                } catch {
                    p.doneBytes = baseDone
                    if attempt >= 3 { throw error }
                    try? await Task.sleep(nanoseconds: 1_000_000_000)
                    continue
                }
                if sha == e.sha256 {
                    let dest = GameStore.root.appendingPathComponent(e.path)
                    try fm.createDirectory(at: dest.deletingLastPathComponent(), withIntermediateDirectories: true)
                    try? fm.removeItem(at: dest)
                    try fm.moveItem(at: part, to: dest)
                    p.doneBytes = baseDone + e.size
                    break
                }
                try? fm.removeItem(at: part)
                p.doneBytes = baseDone
                if attempt >= 3 { throw StoreError.hashMismatch(e.path) }
            }
            p.filesDone += 1
            onProgress?(p)
        }

        // files of an earlier set that this one doesn't have (e.g. switching back from a mod set)
        let wanted = Set(manifest.files.map { $0.path })
        if let en = fm.enumerator(at: GameStore.root, includingPropertiesForKeys: [.isRegularFileKey]) {
            for case let u as URL in en {
                let rel = u.path.replacingOccurrences(of: GameStore.root.path + "/", with: "")
                if rel.hasPrefix(".nfsh") { en.skipDescendants(); continue }
                if (try? u.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true, !wanted.contains(rel) {
                    try? fm.removeItem(at: u)
                }
            }
        }
        try? fm.removeItem(at: GameStore.partsDir)
        try Data("\(set)\n".utf8).write(to: GameStore.verifiedFile, options: .atomic)
        UserDefaults.standard.set("set", forKey: "nfsh.data.verifiedHashesOf")
        UserDefaults.standard.set(set, forKey: "nfsh.data.verifiedSet")
    }
}
