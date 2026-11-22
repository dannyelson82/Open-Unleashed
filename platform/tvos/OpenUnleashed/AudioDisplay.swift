// OpenUnleashed for Apple TV — audio session and display mode (TVOS.md Stages 6.1, 7.2). GPL-3.0-or-later.
import AVFoundation
import AVKit
import UIKit

enum AudioSetup {
    /// Before the engine opens its CoreAudio stream (cpal): a game's "playback" session, multichannel content
    /// declared (lets tvOS spatialise a 5.1 mix on AirPods — `airpods-plan.md` C1), active.
    static func configure() {
        let s = AVAudioSession.sharedInstance()
        do {
            try s.setCategory(.playback, mode: .default, options: [])
            try s.setSupportsMultichannelContent(true)
            try s.setActive(true)
            AudioRoutes.watch()
        } catch {
            NSLog("OpenUnleashed: audio session setup failed: %@", error.localizedDescription)
        }
        NSLog("OpenUnleashed: audio route %@, %d/%d output channels, latency %.0f ms",
              s.currentRoute.outputs.map { "\($0.portName) (\($0.portType.rawValue))" }.joined(separator: ", "),
              s.outputNumberOfChannels, s.maximumOutputNumberOfChannels, s.outputLatency * 1000)
    }
}

enum AudioRoutes {
    private static var observer: NSObjectProtocol?

    /// AVAudioSession route changes → the engine reopens its output on the new route; "old device unavailable"
    /// (AirPods out / disconnected) also pauses a single-player race (`airpods-plan.md` B, E).
    static func watch() {
        guard observer == nil else { return }
        observer = NotificationCenter.default.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { n in
            let raw = (n.userInfo?[AVAudioSessionRouteChangeReasonKey] as? NSNumber)?.uintValue ?? 0
            let reason = AVAudioSession.RouteChangeReason(rawValue: raw) ?? .unknown
            let s = AVAudioSession.sharedInstance()
            NSLog("OpenUnleashed: audio route change (%lu) → %@, %d channels", raw,
                  s.currentRoute.outputs.map { $0.portName }.joined(separator: ", "), s.outputNumberOfChannels)
            switch reason {
            case .newDeviceAvailable, .oldDeviceUnavailable, .override, .routeConfigurationChange, .categoryChange:
                ou_audio_route_changed(reason == .oldDeviceUnavailable)
            default:
                break
            }
        }
    }
}

enum DisplayMode {
    /// Ask for 60 Hz SDR so "Match Frame Rate" doesn't switch the TV to 24/50 Hz (every Apple TV 4K outputs at
    /// most 4K60). Only matters when Match Frame Rate / Dynamic Range is on in the Apple TV's settings.
    static func request60Hz(on window: UIWindow?) {
        guard let window else { return }
        var fmt: CMVideoFormatDescription?
        CMVideoFormatDescriptionCreate(allocator: kCFAllocatorDefault, codecType: kCMVideoCodecType_H264,
                                       width: 3840, height: 2160, extensions: nil, formatDescriptionOut: &fmt)
        guard let fmt else { return }
        let criteria = AVDisplayCriteria(refreshRate: 60, formatDescription: fmt)
        window.avDisplayManager.preferredDisplayCriteria = criteria
    }

    /// The Display setting's render resolution → `NFS_RENDER_SCALE` for the engine (`-ou.renderScale 1|1.5|2`).
    static func exportRenderScale() {
        let v = UserDefaults.standard.double(forKey: "ou.renderScale")
        if v >= 1 { setenv("NFS_RENDER_SCALE", String(v), 1) }
    }
}
