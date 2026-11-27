// NFS Haven for Apple TV — audio session and display mode (TVOS.md Stages 6.1, 7.2). GPL-3.0-or-later.
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
        // 5.1 (Extras › Audio › Speakers): ask for every channel the route offers (tvOS gives the app 2 unless asked),
        // and tell the engine where L, R, C, LFE, Ls, Rs are by the route's channel labels
        if s.maximumOutputNumberOfChannels > 2 {
            try? s.setPreferredOutputNumberOfChannels(s.maximumOutputNumberOfChannels)
        }
        exportSpeakerOrder(s)
        NSLog("OpenUnleashed: audio route %@, %d/%d output channels, latency %.0f ms",
              s.currentRoute.outputs.map { "\($0.portName) (\($0.portType.rawValue))" }.joined(separator: ", "),
              s.outputNumberOfChannels, s.maximumOutputNumberOfChannels, s.outputLatency * 1000)
    }
}

/// `NFS_SPEAKER_ORDER`: the output channel of each of L, R, C, LFE, Ls, Rs, from the route's channel labels (the
/// standard order when a label is missing). Never assume an HDMI layout's order (`enhancements-plan.md` §6).
func exportSpeakerOrder(_ s: AVAudioSession) {
    let channels = s.currentRoute.outputs.first?.channels ?? []
    let labels = channels.map { $0.channelLabel }
    let wanted: [[AudioChannelLabel]] = [
        [kAudioChannelLabel_Left], [kAudioChannelLabel_Right], [kAudioChannelLabel_Center], [kAudioChannelLabel_LFEScreen],
        [kAudioChannelLabel_LeftSurround, kAudioChannelLabel_RearSurroundLeft, kAudioChannelLabel_LeftSurroundDirect],
        [kAudioChannelLabel_RightSurround, kAudioChannelLabel_RearSurroundRight, kAudioChannelLabel_RightSurroundDirect],
    ]
    let order = wanted.enumerated().map { i, ls in labels.firstIndex(where: { ls.contains($0) }) ?? i }
    setenv("NFS_SPEAKER_ORDER", order.map(String.init).joined(separator: ","), 1)
    NSLog("NFS Haven: %d output channels, labels %@ → speaker order %@", s.outputNumberOfChannels, labels.map { String($0) }.joined(separator: ","), order.map(String.init).joined(separator: ","))
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
        // `avDisplayManager` is AVKit's addition to UIWindow: without AVKit loaded the window has no such method,
        // and the call stopped the app (real Apple TV, 2026-11-27; AVKit is now linked in project.yml too)
        guard let window, window.responds(to: NSSelectorFromString("avDisplayManager")) else { return }
        var fmt: CMVideoFormatDescription?
        CMVideoFormatDescriptionCreate(allocator: kCFAllocatorDefault, codecType: kCMVideoCodecType_H264,
                                       width: 3840, height: 2160, extensions: nil, formatDescriptionOut: &fmt)
        guard let fmt else { return }
        let criteria = AVDisplayCriteria(refreshRate: 60, formatDescription: fmt)
        window.avDisplayManager.preferredDisplayCriteria = criteria
    }

    /// The Display setting's render resolution → `NFS_RENDER_SCALE` for the engine (`-nfsh.renderScale 1|1.5|2`).
    static func exportRenderScale() {
        let v = UserDefaults.standard.double(forKey: "nfsh.renderScale")
        if v >= 1 { setenv("NFS_RENDER_SCALE", String(v), 1) }
    }
}
