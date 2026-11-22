// NFS Haven for Apple TV — the Game Data screen (TVOS.md, Stage 4.3). GPL-3.0-or-later.
//
// Shown when Caches/games/porsche/data doesn't hold a complete, verified download, or at launch when the server has newer data
// for the chosen set (a patch, an updated mod set): then Update fetches only the changed files and Play starts
// with the files already here. Fully usable with the Siri Remote or a
// game controller (tvOS focus engine): Server (address + token, Find on network, Test), data set choice
// (original / mod sets, with sizes; "Original files" is always one press away), Download with overall and
// per-file progress, MB/s, resume and SHA-256 verification, and clear errors.
import UIKit

final class GameDataViewController: UIViewController {
    var errorText: String?
    /// Set when the server has newer data than the verified local copy (files and bytes to fetch, files removed).
    var update: (files: Int, bytes: Int64, removed: Int)?
    /// Called when the data is complete and verified.
    var onReady: (() -> Void)?

    private let titleLabel = UILabel()
    private let statusLabel = UILabel()
    private let errorLabel = UILabel()
    private let serverButton = UIButton(type: .system)
    private let findButton = UIButton(type: .system)
    private let testButton = UIButton(type: .system)
    private let setButton = UIButton(type: .system)
    private let downloadButton = UIButton(type: .system)
    private let playButton = UIButton(type: .system)
    private let progress = UIProgressView(progressViewStyle: .default)
    private let progressLabel = UILabel()
    private let fileLabel = UILabel()
    private var finder: ServerFinder?
    private var downloader: GameDownloader?
    private var sets: [DataSetInfo] = []
    private var busy = false

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(white: 0.06, alpha: 1)

        titleLabel.text = "Game Data"
        titleLabel.font = .systemFont(ofSize: 64, weight: .bold)
        titleLabel.textColor = .white

        for l in [statusLabel, errorLabel, progressLabel, fileLabel] {
            l.numberOfLines = 0
            l.textAlignment = .center
            l.font = .systemFont(ofSize: 30)
            l.textColor = UIColor(white: 0.82, alpha: 1)
        }
        errorLabel.textColor = UIColor(red: 1, green: 0.45, blue: 0.4, alpha: 1)
        fileLabel.font = .monospacedSystemFont(ofSize: 24, weight: .regular)
        fileLabel.textColor = UIColor(white: 0.6, alpha: 1)

        for (b, sel) in [(serverButton, #selector(editServer)), (findButton, #selector(findServers)),
                         (testButton, #selector(testConnection)), (setButton, #selector(chooseSet)),
                         (downloadButton, #selector(startDownload)), (playButton, #selector(playWithoutUpdate))] {
            b.addTarget(self, action: sel, for: .primaryActionTriggered)
            b.titleLabel?.font = .systemFont(ofSize: 34, weight: .medium)
        }
        findButton.setTitle("Find on network", for: .normal)
        testButton.setTitle("Test connection", for: .normal)
        playButton.setTitle("Play", for: .normal)

        progress.isHidden = true
        progress.progressTintColor = UIColor(red: 0.85, green: 0.1, blue: 0.1, alpha: 1)

        let serverRow = UIStackView(arrangedSubviews: [serverButton, findButton, testButton])
        serverRow.axis = .horizontal
        serverRow.spacing = 40
        let actionRow = UIStackView(arrangedSubviews: [setButton, downloadButton, playButton])
        actionRow.axis = .horizontal
        actionRow.spacing = 40

        let stack = UIStackView(arrangedSubviews: [titleLabel, statusLabel, serverRow, actionRow, progress, progressLabel, fileLabel, errorLabel])
        stack.axis = .vertical
        stack.alignment = .center
        stack.spacing = 36
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        let g = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: g.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: g.centerYAnchor),
            stack.widthAnchor.constraint(lessThanOrEqualTo: g.widthAnchor, multiplier: 0.9),
            progress.widthAnchor.constraint(equalToConstant: 1100),
        ])
        refresh()
        if UserDefaults.standard.bool(forKey: "nfsh.autoDownload") {
            // test hook (launch argument `-nfsh.autoDownload YES`): download without a button press
            startDownload()
        } else if errorText == nil {
            Task { await loadSets(quiet: true) }
        }
    }

    override var preferredFocusEnvironments: [UIFocusEnvironment] { [downloadButton] }

    private func refresh() {
        let have = GameStore.check()
        if let e = errorText {
            statusLabel.text = "The game could not start with the downloaded data."
            errorLabel.text = e
        } else if let u = update, !busy {
            var t = "Your server has updated game data: \(u.files) file(s) changed (\(Self.mb(u.bytes)))"
            if u.removed > 0 { t += ", \(u.removed) removed" }
            statusLabel.text = t + ".\nUpdate downloads only those files. Play starts with the files already on this Apple TV."
        } else if have == nil {
            statusLabel.text = "The original game data (your own copy) is downloaded once from your NFS Haven server and kept on this Apple TV.\nIf tvOS clears its storage, this screen comes back and only the missing files are fetched again."
        } else {
            statusLabel.text = "\(have!.count) file(s) of the game data are missing (tvOS may have cleared storage). Download them again."
        }
        serverButton.setTitle("Server: \(ServerSettings.address)\(ServerSettings.token.isEmpty ? "" : "  🔑")", for: .normal)
        let set = ServerSettings.dataSet
        let info = sets.first { $0.id == set }
        setButton.setTitle("Data: \(Self.setName(set))\(info.map { "  (\(Self.mb($0.size)))" } ?? "")", for: .normal)
        downloadButton.setTitle(busy ? "Cancel" : (update != nil ? "Update" : "Download"), for: .normal)
        playButton.isHidden = update == nil || busy
        [serverButton, findButton, testButton, setButton].forEach { $0.isEnabled = !busy }
    }

    static func setName(_ id: String) -> String {
        let set = id.split(separator: "/", maxSplits: 1).last.map(String.init) ?? id
        if set == "original" { return "Original files" }
        if set.hasPrefix("mod-") { return "Mod: " + set.dropFirst(4) }
        return id
    }

    static func mb(_ n: Int64) -> String { String(format: "%.0f MB", Double(n) / 1e6) }

    private func show(error: Error?) {
        errorLabel.text = error?.localizedDescription
    }

    // MARK: server

    @objc private func editServer() {
        let a = UIAlertController(title: "NFS Haven server", message: "Name or IP address (e.g. tower.local, 192.168.50.37, tower-1.tawny-taipan.ts.net) and the server's password.", preferredStyle: .alert)
        a.addTextField { t in
            t.text = ServerSettings.address
            t.placeholder = "tower.local"
            t.keyboardType = .URL
            t.autocapitalizationType = .none
            t.autocorrectionType = .no
        }
        a.addTextField { t in
            t.text = ServerSettings.token
            t.placeholder = "password"
            t.isSecureTextEntry = true
            t.autocapitalizationType = .none
            t.autocorrectionType = .no
        }
        a.addAction(UIAlertAction(title: "Save", style: .default) { [weak self] _ in
            ServerSettings.address = a.textFields?[0].text ?? ""
            ServerSettings.token = a.textFields?[1].text ?? ""
            self?.refresh()
            Task { await self?.loadSets(quiet: false) }
        })
        a.addAction(UIAlertAction(title: "Cancel", style: .cancel))
        present(a, animated: true)
    }

    @objc private func findServers() {
        findButton.setTitle("Searching…", for: .normal)
        let f = ServerFinder()
        finder = f
        f.find { [weak self] found in
            guard let self else { return }
            self.findButton.setTitle("Find on network", for: .normal)
            if found.isEmpty {
                self.errorLabel.text = "No NFS Haven server found on this network (Bonjour _nfs-haven._tcp). Enter its address instead — see docs/nfs-haven-server.md for advertising it from Unraid."
                return
            }
            let a = UIAlertController(title: "Servers found", message: nil, preferredStyle: .actionSheet)
            for s in found {
                a.addAction(UIAlertAction(title: "\(s.name) — \(s.address)", style: .default) { _ in
                    ServerSettings.address = s.address
                    self.refresh()
                    Task { await self.loadSets(quiet: false) }
                })
            }
            a.addAction(UIAlertAction(title: "Cancel", style: .cancel))
            self.present(a, animated: true)
        }
    }

    @objc private func testConnection() {
        Task { await loadSets(quiet: false, test: true) }
    }

    @MainActor
    private func loadSets(quiet: Bool, test: Bool = false) async {
        if test { errorLabel.text = "Connecting…" }
        do {
            let (c, info) = try await ServerClient.connect(address: ServerSettings.address, token: ServerSettings.token)
            sets = try await c.dataSets()
            if !sets.contains(where: { $0.id == ServerSettings.dataSet }) { ServerSettings.dataSet = "porsche/original" }
            errorLabel.text = test ? "✓ \(info.name) — nfs-haven-server \(info.version) at \(c.base.absoluteString); data: \(info.data_state ?? "?"), sets: \(sets.map { Self.setName($0.id) }.joined(separator: ", "))" : nil
            errorLabel.textColor = test ? UIColor(red: 0.5, green: 0.9, blue: 0.5, alpha: 1) : UIColor(red: 1, green: 0.45, blue: 0.4, alpha: 1)
        } catch {
            errorLabel.textColor = UIColor(red: 1, green: 0.45, blue: 0.4, alpha: 1)
            if !quiet { show(error: error) }
        }
        refresh()
    }

    @objc private func chooseSet() {
        let a = UIAlertController(title: "Game data", message: "Mod sets are the original files with the mod's files on top. \"Original files\" always goes back to the unmodified game.", preferredStyle: .actionSheet)
        let list = sets.isEmpty ? [DataSetInfo(id: "porsche/original", kind: "original", files: 0, size: 0)] : sets.filter { $0.id.hasPrefix("porsche/") && $0.kind != "hd" }
        for s in list {
            let t = "\(Self.setName(s.id))\(s.size > 0 ? "  —  \(Self.mb(s.size))" : "")\(s.id == ServerSettings.dataSet ? "  ✓" : "")"
            a.addAction(UIAlertAction(title: t, style: .default) { [weak self] _ in
                ServerSettings.dataSet = s.id
                self?.refresh()
            })
        }
        a.addAction(UIAlertAction(title: "Cancel", style: .cancel))
        present(a, animated: true)
    }

    // MARK: download

    @objc private func startDownload() {
        if busy {
            downloader?.cancel()
            return
        }
        busy = true
        errorText = nil
        errorLabel.text = nil
        progress.isHidden = false
        progress.progress = 0
        progressLabel.text = "Connecting to \(ServerSettings.address)…"
        refresh()
        Task { @MainActor in
            do {
                let (c, _) = try await ServerClient.connect(address: ServerSettings.address, token: ServerSettings.token)
                let d = GameDownloader(client: c, set: ServerSettings.dataSet)
                downloader = d
                d.onProgress = { [weak self] p in
                    DispatchQueue.main.async { self?.render(p) }
                }
                try await d.run()
                progressLabel.text = "Game data complete and verified."
                fileLabel.text = nil
                busy = false
                update = nil
                refresh()
                onReady?()
            } catch {
                NSLog("NFS Haven: game data download failed: %@", error.localizedDescription)
                busy = false
                progress.isHidden = true
                progressLabel.text = nil
                fileLabel.text = nil
                show(error: error)
                refresh()
            }
        }
    }

    /// Keep the current files this time (the check runs again at the next launch).
    @objc private func playWithoutUpdate() {
        guard !busy else { return }
        onReady?()
    }

    private func render(_ p: DownloadProgress) {
        let frac = p.totalBytes > 0 ? Float(Double(p.doneBytes) / Double(p.totalBytes)) : 1
        progress.setProgress(frac, animated: false)
        progressLabel.text = String(format: "%.0f %%  ·  %@ of %@  ·  file %d of %d  ·  %.1f MB/s",
                                    frac * 100, Self.mb(p.doneBytes), Self.mb(p.totalBytes), min(p.filesDone + 1, p.filesTotal), p.filesTotal, p.bytesPerSecond / 1e6)
        fileLabel.text = p.currentFile.isEmpty ? nil : String(format: "%@  %.0f %%", p.currentFile, p.currentSize > 0 ? Double(p.currentDone) / Double(p.currentSize) * 100 : 100)
    }
}
