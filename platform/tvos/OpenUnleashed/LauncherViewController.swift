// NFS Haven for Apple TV — the launcher: one tile per game (nfs-haven-plan.md Phase 4, first version).
// GPL-3.0-or-later.
//
// Our own plain tiles (no EA artwork). Each shows whether the game's data is on this Apple TV. The last game played
// has the focus. A game runs until it is quit; then the app ends and opens here again.
import UIKit

final class LauncherViewController: UIViewController {
    var onPick: ((GameID) -> Void)?
    private var tiles: [GameID: UIButton] = [:]

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(white: 0.06, alpha: 1)

        let title = UILabel()
        title.text = "NFS Haven"
        title.font = .systemFont(ofSize: 76, weight: .heavy)
        title.textColor = .white
        let hint = UILabel()
        hint.text = "Choose a game. Quitting a game closes the app; open it again to pick another."
        hint.font = .systemFont(ofSize: 28)
        hint.textColor = UIColor(white: 0.6, alpha: 1)

        let row = UIStackView()
        row.axis = .horizontal
        row.spacing = 80
        row.distribution = .fillEqually
        for g in GameID.allCases {
            let b = UIButton(type: .system)
            var c = UIButton.Configuration.filled()
            c.baseBackgroundColor = g == .porsche ? UIColor(red: 0.55, green: 0.08, blue: 0.08, alpha: 1) : UIColor(red: 0.1, green: 0.22, blue: 0.5, alpha: 1)
            c.baseForegroundColor = .white
            c.cornerStyle = .large
            c.titleAlignment = .center
            c.contentInsets = NSDirectionalEdgeInsets(top: 60, leading: 40, bottom: 60, trailing: 40)
            var t = AttributedString(g.title)
            t.font = .systemFont(ofSize: 48, weight: .bold)
            c.attributedTitle = t
            var s = AttributedString("\(g.subtitle)\n\(Self.status(g))")
            s.font = .systemFont(ofSize: 28)
            c.attributedSubtitle = s
            b.configuration = c
            b.addAction(UIAction { [weak self] _ in self?.onPick?(g) }, for: .primaryActionTriggered)
            b.heightAnchor.constraint(equalToConstant: 420).isActive = true
            tiles[g] = b
            row.addArrangedSubview(b)
        }

        let stack = UIStackView(arrangedSubviews: [title, row, hint])
        stack.axis = .vertical
        stack.alignment = .center
        stack.spacing = 70
        stack.translatesAutoresizingMaskIntoConstraints = false
        row.widthAnchor.constraint(equalToConstant: 1400).isActive = true
        view.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: view.centerYAnchor),
        ])
    }

    /// Whether the game's data is here (cheap: the folders, not the checksums).
    private static func status(_ g: GameID) -> String {
        let have = g.requiredItems.allSatisfy { FileManager.default.fileExists(atPath: g.dataDir.appendingPathComponent($0).path) }
        return have ? "Ready" : "Game data: download from your server"
    }

    override var preferredFocusEnvironments: [UIFocusEnvironment] {
        let last = UserDefaults.standard.string(forKey: GameID.lastKey).flatMap(GameID.init) ?? .porsche
        return tiles[last].map { [$0] } ?? []
    }
}
