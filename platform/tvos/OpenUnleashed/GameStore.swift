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

/// **One URLSession for a whole download** (keep-alive: no new connection per file) whose tasks each hand their
/// response and bytes to their own `Sink`. The delegate queue is serial, so a sink is only ever called from one thread.
final class Transfer: NSObject, URLSessionDataDelegate {
    final class Sink {
        var status = 0
        /// the response arrived: answer whether to go on
        var onResponse: (Int) -> Bool = { _ in true }
        var onData: (Data) throws -> Void = { _ in }
        var onEnd: () -> Void = {}
        var continuation: CheckedContinuation<Int, Error>?
        var failed: Error?
    }

    private let lock = NSLock()
    private var sinks: [Int: Sink] = [:]
    private let queue: OperationQueue = {
        let q = OperationQueue()
        q.maxConcurrentOperationCount = 1
        q.qualityOfService = .userInitiated
        return q
    }()
    /// made once in `init`, before any task can ask for it (a lazy one raced: concurrent first downloads each made a
    /// session of their own, whose tasks never ran)
    private(set) var session: URLSession!

    override init() {
        super.init()
        let cfg = URLSessionConfiguration.default
        cfg.httpMaximumConnectionsPerHost = 8
        cfg.timeoutIntervalForRequest = 30
        cfg.waitsForConnectivity = false
        session = URLSession(configuration: cfg, delegate: self, delegateQueue: queue)
    }

    /// Run `req` into `sink`; returns the HTTP status once the body has been delivered.
    func run(_ req: URLRequest, sink: Sink) async throws -> Int {
        try await withCheckedThrowingContinuation { (c: CheckedContinuation<Int, Error>) in
            sink.continuation = c
            let task = session.dataTask(with: req)
            lock.lock(); sinks[task.taskIdentifier] = sink; lock.unlock()
            task.resume()
        }
    }

    func invalidate() { session.finishTasksAndInvalidate() }

    private func sink(_ t: URLSessionTask) -> Sink? {
        lock.lock(); defer { lock.unlock() }
        return sinks[t.taskIdentifier]
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive response: URLResponse, completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard let s = sink(dataTask) else { return completionHandler(.cancel) }
        s.status = status
        completionHandler(s.onResponse(status) ? .allow : .cancel)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        guard let s = sink(dataTask), s.failed == nil else { return }
        do { try s.onData(data) } catch {
            s.failed = error
            dataTask.cancel()
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        lock.lock(); let s = sinks.removeValue(forKey: task.taskIdentifier); lock.unlock()
        guard let s, let c = s.continuation else { return }
        s.continuation = nil
        s.onEnd()
        if let f = s.failed { return c.resume(throwing: f) }
        if let e = error {
            return c.resume(throwing: s.status == 401 ? ServerError.unauthorized : (s.status >= 400 ? ServerError.http(s.status, "download failed") : e))
        }
        c.resume(returning: s.status)
    }
}

/// Progress shared by concurrent downloads (callbacks come from the transfer's queue and the download tasks).
final class ProgressBox {
    private let lock = NSLock()
    private var p: DownloadProgress
    private let started = Date()
    private var sessionBytes: Int64 = 0
    var onChange: ((DownloadProgress) -> Void)?
    private var lastReport = Date.distantPast

    init(_ p: DownloadProgress) { self.p = p }

    func bytes(_ n: Int64, file: String? = nil, fileDone: Int64 = 0, fileSize: Int64 = 0) {
        lock.lock()
        p.doneBytes += n
        sessionBytes += max(0, n)
        if let file { p.currentFile = file; p.currentDone = fileDone; p.currentSize = fileSize }
        p.bytesPerSecond = Double(sessionBytes) / max(0.001, Date().timeIntervalSince(started))
        // at most ~20 updates a second reach the screen
        let now = Date()
        let report = now.timeIntervalSince(lastReport) > 0.05
        if report { lastReport = now }
        let snap = p
        lock.unlock()
        if report { onChange?(snap) }
    }

    func fileDone() {
        lock.lock(); p.filesDone += 1; let snap = p; lock.unlock()
        onChange?(snap)
    }

    var stats: (files: Int, bytes: Int64, seconds: Double) {
        lock.lock(); defer { lock.unlock() }
        return (p.filesDone, sessionBytes, Date().timeIntervalSince(started))
    }
}

/// Brings Caches/games/porsche/data in line with a set's manifest: a **fresh install streams the set's tar**
/// (`/v1/data/{game}/{set}.tar`, extracted as it arrives, every file's SHA-256 checked, resumed from the first missing
/// file); an update or a repair fetches the files it needs, **several at once**, each with Range resume and its
/// SHA-256 (`If-Range` on the file's hash). One URLSession throughout.
final class GameDownloader {
    let client: ServerClient
    let set: String
    private(set) var cancelled = false
    var onProgress: ((DownloadProgress) -> Void)?
    /// how many files download at once (per-file mode)
    static let concurrency = 5

    init(client: ServerClient, set: String) {
        self.client = client
        self.set = set
    }

    func cancel() { cancelled = true }

    private func request(_ url: URL) -> URLRequest {
        var req = URLRequest(url: url)
        if !client.token.isEmpty { req.setValue("Bearer \(client.token)", forHTTPHeaderField: "Authorization") }
        req.setValue(ServerClient.deviceName, forHTTPHeaderField: "X-NFSH-Device")
        return req
    }

    private static func size(_ url: URL) -> Int64? {
        (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber)?.int64Value
    }

    /// A file's part while it downloads: named after its path (stable for resume, and unique: two files with the
    /// same contents — the game has some — download side by side without sharing a part).
    static func partURL(_ e: ManifestEntry) -> URL {
        let name = SHA256.hash(data: Data(e.path.utf8)).prefix(16).map { String(format: "%02x", $0) }.joined()
        return GameStore.partsDir.appendingPathComponent(name)
    }

    /// Put a verified part in place.
    private static func place(_ part: URL, as e: ManifestEntry) throws {
        let fm = FileManager.default
        let dest = GameStore.root.appendingPathComponent(e.path)
        try fm.createDirectory(at: dest.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? fm.removeItem(at: dest)
        try fm.moveItem(at: part, to: dest)
    }

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
        let tarMarker = GameStore.meta.appendingPathComponent("tar-install")
        let resumingTar = fm.fileExists(atPath: tarMarker.path)
        var todo: [ManifestEntry] = []
        var present = 0
        for e in manifest.files {
            let dest = GameStore.root.appendingPathComponent(e.path)
            let size = Self.size(dest)
            if size != nil { present += 1 }
            // a file a tar install already placed was verified as it was written
            if size == e.size, resumingTar { continue }
            if size == e.size, previousVerified, previous[e.path]?.sha256 == e.sha256 { continue }
            if size == e.size, GameStore.sha256(of: dest) == e.sha256 { continue }
            todo.append(e)
        }
        try raw.write(to: GameStore.manifestFile, options: .atomic)
        UserDefaults.standard.set("partial", forKey: "nfsh.data.verifiedHashesOf")

        var p0 = DownloadProgress()
        p0.filesTotal = todo.count
        p0.totalBytes = todo.reduce(0) { $0 + $1.size }
        let free = GameStore.freeSpace()
        if p0.totalBytes > free - 50_000_000 {
            throw StoreError.diskFull(need: p0.totalBytes, free: free)
        }
        let progress = ProgressBox(p0)
        progress.onChange = { [weak self] p in self?.onProgress?(p) }
        onProgress?(p0)

        let transfer = Transfer()
        defer { transfer.invalidate() }
        // a fresh install (nothing there yet), or one a tar stream had begun: the fast way
        // (test hook `-nfsh.noTar YES`: the files one by one even for a fresh install, to compare)
        if !todo.isEmpty, present == 0 || resumingTar, !UserDefaults.standard.bool(forKey: "nfsh.noTar") {
            try? Data().write(to: tarMarker)
            do {
                try await tarInstall(manifest: manifest, raw: raw, transfer: transfer, progress: progress)
            } catch let e as StoreError {
                throw e
            } catch {
                // no bundle on this server (an older one), or the stream broke: the files one by one from where it got
                NSLog("NFS Haven: fast install stopped (%@); the remaining files one by one", error.localizedDescription)
            }
            todo = manifest.files.filter { Self.size(GameStore.root.appendingPathComponent($0.path)) != $0.size }
        }
        try await perFile(todo, transfer: transfer, progress: progress)

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
        try? fm.removeItem(at: tarMarker)
        let st = progress.stats
        NSLog("NFS Haven: game data download: %d files, %.1f MB in %.1f s = %.1f MB/s", st.files, Double(st.bytes) / 1e6, st.seconds, Double(st.bytes) / 1e6 / max(st.seconds, 0.001))
        try Data("\(set)\n".utf8).write(to: GameStore.verifiedFile, options: .atomic)
        UserDefaults.standard.set("set", forKey: "nfsh.data.verifiedHashesOf")
        UserDefaults.standard.set(set, forKey: "nfsh.data.verifiedSet")
    }

    // MARK: per file, several at once

    private func perFile(_ todo: [ManifestEntry], transfer: Transfer, progress: ProgressBox) async throws {
        guard !todo.isEmpty else { return }
        try await withThrowingTaskGroup(of: Void.self) { group in
            var next = 0
            func add() {
                guard next < todo.count else { return }
                let e = todo[next]
                next += 1
                group.addTask { [self] in try await self.fetch(e, transfer: transfer, progress: progress) }
            }
            for _ in 0..<min(Self.concurrency, todo.count) { add() }
            while try await group.next() != nil {
                if cancelled { group.cancelAll(); throw StoreError.cancelled }
                add()
            }
        }
    }

    /// One file into `parts/<sha256>` with Range resume, hashed as it arrives; placed once its hash is right.
    private func fetch(_ e: ManifestEntry, transfer: Transfer, progress: ProgressBox) async throws {
        let fm = FileManager.default
        let part = Self.partURL(e)
        var attempt = 0
        while true {
            attempt += 1
            if cancelled { throw StoreError.cancelled }
            if !fm.fileExists(atPath: part.path) { fm.createFile(atPath: part.path, contents: nil) }
            let have = Self.size(part) ?? 0
            var hasher = SHA256()
            if have > 0, let h = try? FileHandle(forReadingFrom: part) {
                while let c = try? h.read(upToCount: 1 << 20), !c.isEmpty { hasher.update(data: c) }
                try? h.close()
            }
            var req = request(client.fileURL(set: set, path: e.path))
            req.setValue("\"\(e.sha256)\"", forHTTPHeaderField: "If-Range")
            if have > 0 { req.setValue("bytes=\(have)-", forHTTPHeaderField: "Range") }
            let handle = try FileHandle(forWritingTo: part)
            try handle.seekToEnd()
            var got: Int64 = 0
            progress.bytes(have, file: e.path, fileDone: have, fileSize: e.size)
            let sink = Transfer.Sink()
            sink.onResponse = { status in
                if status == 200 {
                    // the server ignored the range (or the part was stale): start over
                    try? handle.truncate(atOffset: 0)
                    hasher = SHA256()
                    progress.bytes(-have)
                }
                return status == 200 || status == 206 || status == 416
            }
            sink.onData = { [weak self] d in
                if self?.cancelled == true { throw StoreError.cancelled }
                try handle.write(contentsOf: d)
                hasher.update(data: d)
                got += Int64(d.count)
                progress.bytes(Int64(d.count), file: e.path, fileDone: have + got, fileSize: e.size)
            }
            sink.onEnd = { try? handle.close() }
            let status: Int
            do {
                status = try await transfer.run(req, sink: sink)
            } catch let err as StoreError {
                throw err
            } catch {
                NSLog("NFS Haven: %@ attempt %d failed: %@", e.path, attempt, error.localizedDescription)
                progress.bytes(-(have + got))
                if attempt >= 3 { throw error }
                try? await Task.sleep(nanoseconds: 1_000_000_000)
                continue
            }
            if !(status == 200 || status == 206 || status == 416) {
                progress.bytes(-(have + got))
                if attempt >= 3 { throw ServerError.http(status, "download failed") }
                continue
            }
            if hasher.finalize().map({ String(format: "%02x", $0) }).joined() == e.sha256 {
                try Self.place(part, as: e)
                progress.fileDone()
                return
            }
            try? fm.removeItem(at: part)
            progress.bytes(-(have + got))
            if attempt >= 3 { throw StoreError.hashMismatch(e.path) }
        }
    }

    // MARK: fast install: the set's tar, extracted as it arrives

    /// Where each entry's header starts in the server's tar (`tools/nfs-haven-server/src/bundle.rs`: 512-byte ustar
    /// headers, data padded to 512), and the tar's `ETag` (from the manifest's own bytes).
    static func tarOffsets(_ m: Manifest) -> [Int64] {
        var at: Int64 = 0
        return m.files.map { f in
            defer { at += 512 + (f.size + 511) / 512 * 512 }
            return at
        }
    }

    static func tarETag(_ raw: Data) -> String {
        "\"tar-" + SHA256.hash(data: raw).prefix(16).map { String(format: "%02x", $0) }.joined() + "\""
    }

    private func tarInstall(manifest: Manifest, raw: Data, transfer: Transfer, progress: ProgressBox) async throws {
        let fm = FileManager.default
        let files = manifest.files
        let offsets = Self.tarOffsets(manifest)
        // resume at the first file that isn't in place yet (everything before it was verified when written)
        let first = files.firstIndex { Self.size(GameStore.root.appendingPathComponent($0.path)) != $0.size } ?? files.count
        guard first < files.count else { return }
        var req = request(client.base.appendingPathComponent("v1/data").appendingPathComponent(set + ".tar"))
        req.setValue(Self.tarETag(raw), forHTTPHeaderField: "If-Range")
        if first > 0 { req.setValue("bytes=\(offsets[first])-", forHTTPHeaderField: "Range") }
        NSLog("NFS Haven: fast install from file %d of %d (byte %lld)", first + 1, files.count, offsets[first])

        // the stream's state: header bytes gathered, then a file's bytes, then its padding
        var index = first
        var header = Data()
        var entry: ManifestEntry?
        var left: Int64 = 0
        var pad: Int64 = 0
        var handle: FileHandle?
        var hasher = SHA256()
        var part: URL?
        var skip = false   // a file before `first` when the server sent the whole tar (200)
        var done = false

        func finishFile() throws {
            guard let e = entry else { return }
            try handle?.close()
            handle = nil
            if !skip {
                let sha = hasher.finalize().map { String(format: "%02x", $0) }.joined()
                guard sha == e.sha256, let part else { throw StoreError.hashMismatch(e.path) }
                try Self.place(part, as: e)
                progress.fileDone()
            }
            entry = nil
            index += 1
        }

        func consume(_ d: Data) throws {
            var d = d[...]
            while !d.isEmpty, !done {
                if cancelled { throw StoreError.cancelled }
                if left > 0 {
                    let n = Int(min(left, Int64(d.count)))
                    let chunk = d.prefix(n)
                    if !skip {
                        try handle?.write(contentsOf: chunk)
                        hasher.update(data: chunk)
                        progress.bytes(Int64(n), file: entry?.path, fileDone: (entry?.size ?? 0) - left + Int64(n), fileSize: entry?.size ?? 0)
                    }
                    left -= Int64(n)
                    d = d.dropFirst(n)
                    if left == 0 { try finishFile() }
                    continue
                }
                if pad > 0 {
                    let n = Int(min(pad, Int64(d.count)))
                    pad -= Int64(n)
                    d = d.dropFirst(n)
                    continue
                }
                // a header (or the end's zero blocks)
                let n = min(512 - header.count, d.count)
                header.append(d.prefix(n))
                d = d.dropFirst(n)
                guard header.count == 512 else { continue }
                defer { header.removeAll(keepingCapacity: true) }
                if header.allSatisfy({ $0 == 0 }) { done = true; break }
                guard index < files.count else { throw ServerError.badResponse }
                let e = files[index]
                let name = String(decoding: header[0..<100].prefix { $0 != 0 }, as: UTF8.self)
                let prefix = String(decoding: header[345..<500].prefix { $0 != 0 }, as: UTF8.self)
                let path = prefix.isEmpty ? name : prefix + "/" + name
                let size = Int64(String(decoding: header[124..<135], as: UTF8.self), radix: 8) ?? -1
                // the stream must be the manifest, in its order
                guard path == e.path, size == e.size else { throw ServerError.badResponse }
                entry = e
                left = e.size
                pad = (e.size + 511) / 512 * 512 - e.size
                skip = index < first
                hasher = SHA256()
                if !skip {
                    let pt = Self.partURL(e)
                    fm.createFile(atPath: pt.path, contents: nil)
                    handle = try FileHandle(forWritingTo: pt)
                    part = pt
                }
                if left == 0 { try finishFile() }
            }
        }

        let sink = Transfer.Sink()
        sink.onResponse = { status in
            // 200: the whole tar (no resume, or the manifest changed): files before `first` are passed over
            if status == 200 { index = 0 }
            return status == 200 || status == 206
        }
        sink.onData = { d in try consume(d) }
        sink.onEnd = { try? handle?.close() }
        let status = try await transfer.run(req, sink: sink)
        guard status == 200 || status == 206 else { throw ServerError.http(status, "no fast install bundle") }
        guard done || index == files.count else { throw ServerError.http(status, "the bundle ended early") }
    }
}
