// The Siri Remote as a mouse in the game's menus: the touch surface moves the pointer like a trackpad, a click
// is the left button, Menu is back and Play/Pause opens Extras (engine: `ou_remote_*` in tvos.rs). The buttons
// are read through the GameController framework; the touch surface through UIKit's indirect touches (a pan
// gesture on the game's window). SDL leaves the remote alone. GPL-3.0-or-later.
import Foundation
import GameController
import UIKit

enum RemoteInput {
    /// a game has started: touches before that (the launcher) don't move the game's pointer
    static var live = false

    static func start() {
        NSLog("OpenUnleashed: RemoteInput start, %d controllers", GCController.controllers().count)
        GCController.controllers().forEach(attach)
        NotificationCenter.default.addObserver(forName: .GCControllerDidConnect, object: nil, queue: .main) { n in
            if let c = n.object as? GCController { attach(c) }
        }
        // the engine's window (winit's) is made after this: take each window that becomes key
        NotificationCenter.default.addObserver(forName: UIWindow.didBecomeKeyNotification, object: nil, queue: .main) { n in
            if let w = n.object as? UIWindow { RemotePan.shared.attach(to: w) }
        }
    }

    /// A Siri Remote: a micro gamepad without the extended profile. Porsche only: NFS SE reads the remote itself
    /// (`NFSSEInput`), and these handlers would take its buttons away.
    private static func attach(_ c: GCController) {
        guard Shell.game != .nfsse || !live, c.extendedGamepad == nil, let micro = c.microGamepad else { return }
        NSLog("OpenUnleashed: Siri Remote as pointer (%@)", c.vendorName ?? "remote")
        ou_remote_button(0)   // engine log: the shell attached the remote (0 = no action)
        micro.allowsRotation = false
        // the touch surface is not read here: its d-pad values are smoothed, and as a finger lifts they ease
        // back towards the centre, which pulled the pointer back like elastic (real Apple TV, 2026-11-27).
        // UIKit's touches end cleanly on the lift (`RemotePan`).
        micro.buttonA.pressedChangedHandler = { _, _, pressed in ou_remote_click(pressed) }
        micro.buttonMenu.pressedChangedHandler = { _, _, pressed in if pressed { ou_remote_button(1) } }
        micro.buttonX.pressedChangedHandler = { _, _, pressed in if pressed { ou_remote_button(2) } }
    }
}

/// The touch surface as a trackpad: a pan of the remote's indirect touches on a window, sent as movement in the
/// engine's remote units (a swipe the width of the window is two units).
final class RemotePan: NSObject {
    static let shared = RemotePan()
    private var last = CGPoint.zero
    private var attached = Set<ObjectIdentifier>()
    private var logged = 0

    func attach(to w: UIWindow) {
        // NFS SE's window keeps the remote's touches for the game (its GCEventViewController); a pan here took them
        if RemoteInput.live && Shell.game == .nfsse { return }
        guard attached.insert(ObjectIdentifier(w)).inserted else { return }
        let pan = UIPanGestureRecognizer(target: self, action: #selector(pan(_:)))
        pan.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.indirect.rawValue)]
        pan.cancelsTouchesInView = false
        w.addGestureRecognizer(pan)
    }

    @objc private func pan(_ g: UIPanGestureRecognizer) {
        let t = g.translation(in: g.view)
        switch g.state {
        case .began:
            last = t
        case .changed:
            let (dx, dy) = (t.x - last.x, t.y - last.y)
            last = t
            let width = max(g.view?.bounds.width ?? 1920, 1)
            if RemoteInput.live {
                ou_remote_touch(Float(dx / width * 2), Float(dy / width * 2))   // UIKit's y is down, as the screen's
            }
        default:   // ended, cancelled: the finger lifted, nothing more moves
            if logged < 3, g.state == .ended {
                logged += 1
                NSLog("OpenUnleashed: remote pan ended at translation %.0f, %.0f (window %.0f wide)", t.x, t.y, g.view?.bounds.width ?? 0)
            }
            last = .zero
        }
    }
}
