// NFS Haven for Apple TV — The Need for Speed SE on the TV: ReNFS (engines/renfs, unchanged) through the host layer
// (platform/renfs-host). GPL-3.0-or-later.
//
// - Picture: the game's newest frame (640x480 menus, the race's 320x200 / 640x480 …) drawn 4:3 in the middle of the
//   screen with a "sharp bilinear" filter (crisp pixels without uneven sizes at 2.25x / 4.5x), Metal.
// - Sound: the game's 44.1 kHz stereo mix pulled by AVAudioEngine.
// - Controllers: a game controller is the game's own pad (left stick steers, right/left trigger gas/brake, shoulders
//   gears, A horn, B map, X camera, Y handbrake, Menu = Esc: back in the menus, the pause menu in a race; the
//   original's joystick calibration the first time). The Siri Remote moves through the menus: swipe or press the
//   ring = arrows, click = Enter, Back/Menu = Esc. Play/Pause (or a pad's View/Options button) opens the keyboard to
//   type a name. A keyboard works as the PC's.
// - Background: the game thread and its clock stop, so a race carries on where it was.
// - Quit (the game's own Exit): the app closes; open it again for the launcher.
import AVFoundation
import GameController
import MetalKit
import UIKit

final class NFSSEViewController: GCEventViewController, MTKViewDelegate {
    static weak var current: NFSSEViewController?

    private var mtk: MTKView!
    private var queue: MTLCommandQueue!
    private var pipeline: MTLRenderPipelineState!
    private var sampler: MTLSamplerState!
    private var texture: MTLTexture?
    private var lastSerial: UInt64 = 0
    private let audio = NFSSEAudio()
    private let input = NFSSEInput()
    private var started = false

    override func loadView() {
        let v = MTKView(frame: UIScreen.main.bounds, device: MTLCreateSystemDefaultDevice())
        v.colorPixelFormat = .bgra8Unorm
        v.clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        v.preferredFramesPerSecond = 60
        v.framebufferOnly = true
        mtk = v
        view = v
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        NFSSEViewController.current = self
        // the controllers (and the remote's Menu button) go to the game, never to UIKit's focus engine or the Home screen
        controllerUserInteractionEnabled = false
        guard let device = mtk.device else { return }
        queue = device.makeCommandQueue()
        do {
            let lib = try device.makeLibrary(source: Self.shader, options: nil)
            let d = MTLRenderPipelineDescriptor()
            d.vertexFunction = lib.makeFunction(name: "v_main")
            d.fragmentFunction = lib.makeFunction(name: "f_main")
            d.colorAttachments[0].pixelFormat = mtk.colorPixelFormat
            pipeline = try device.makeRenderPipelineState(descriptor: d)
        } catch {
            NSLog("NFS SE: presenter shader failed: %@", "\(error)")
        }
        let sd = MTLSamplerDescriptor()
        sd.minFilter = .linear
        sd.magFilter = .linear
        sd.sAddressMode = .clampToEdge
        sd.tAddressMode = .clampToEdge
        sampler = device.makeSamplerState(descriptor: sd)
        mtk.delegate = self
        input.onTextRequest = { [weak self] in self?.askForText() }
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !started else { return }
        started = true
        startGame()
    }

    private func startGame() {
        let g = GameID.nfsse
        NFSSEViewController.prepareHome(g.saveDir)
        var cb = RenfsHostCallbacks(
            message: { text in
                let t = text.map { String(cString: $0) } ?? "The game stopped."
                NSLog("NFS SE: %@", t)
                DispatchQueue.main.async { Shell.shared.gameMessage(t) }
            },
            rumble: { low, high, ms in NFSSEInput.rumble(low: low, high: high, ms: ms) },
            ended: { code in
                NSLog("NFS SE: the game ended (%d)", code)
                DispatchQueue.main.async { NFSSEViewController.current?.gameEnded() }
            })
        let lang = UserDefaults.standard.string(forKey: "nfsh.nfsse.lang")
        let ok = g.dataDir.path.withCString { d in
            g.saveDir.path.withCString { h in
                if let lang { return lang.withCString { renfs_host_start(d, h, $0, &cb) } }
                return renfs_host_start(d, h, nil, &cb)
            }
        }
        NSLog("NFS SE: started=%d data=%@ home=%@", ok ? 1 : 0, g.dataDir.path, g.saveDir.path)
        audio.start()
        input.start()
        if let script = UserDefaults.standard.string(forKey: "nfsh.seScript") { input.play(script: script) }
    }

    /// First-run defaults for a controller (nfs-haven-plan.md Phase 6.1), in ReNFS's own files:
    /// - `joystick.cfg`: the calibration ReNFS itself made for a pad's full range (its calibration screen driven
    ///   by the Mac harness with the stick and both triggers moved end to end: platform/renfs-host/test), so the
    ///   1996 calibration screen doesn't come up; Options › Controls › Calibrate still redoes it.
    /// - `config.dat`, once: Steering = Joystick left/right and Accelerate/brake = Joystick (the triggers as pedals)
    ///   instead of the keys, for analogue driving. ReNFS turns them back to the keys by itself when no controller
    ///   is connected. config.dat exists from the game's first run on, so this applies from the second start.
    static func prepareHome(_ home: URL) {
        let cal = home.appendingPathComponent("joystick.cfg")
        if !FileManager.default.fileExists(atPath: cal.path) {
            let hex = "0000000000010000ff010000dd22000000230000800000007f0100000000000000010000ff010000dd22000000230000800000007f010000000000000001000000020000dd220000da250000f71100000000000000020000f7110000"
            var d = Data()
            var i = hex.startIndex
            while i < hex.endIndex {
                let j = hex.index(i, offsetBy: 2)
                d.append(UInt8(hex[i..<j], radix: 16)!)
                i = j
            }
            try? d.write(to: cal, options: .atomic)
        }
        let key = "nfsh.nfsse.analogDefault"
        let cfg = home.appendingPathComponent("config.dat")
        if !UserDefaults.standard.bool(forKey: key), var d = try? Data(contentsOf: cfg), d.count == 0x5f52 {
            // only if both are still the keys (the player hasn't chosen)
            if d[0x5f36] == 0 && d[0x5f37] == 0 && d[0x5f38] == 0 && d[0x5f39] == 0 {
                d[0x5f36] = 1      // steering: the joystick across
                d[0x5f38] = 2      // accelerate / brake: the joystick's pedals
                try? d.write(to: cfg, options: .atomic)
                NSLog("NFS SE: controls set to analogue (joystick) for a controller")
            }
            UserDefaults.standard.set(true, forKey: key)
        }
    }

    private func gameEnded() {
        audio.stop()
        input.stop()
        UIApplication.shared.isIdleTimerDisabled = false
        let card = UILabel(frame: view.bounds)
        card.text = "Closing… open NFS Haven again to pick a game."
        card.textAlignment = .center
        card.textColor = .white
        card.font = .systemFont(ofSize: 40, weight: .medium)
        card.backgroundColor = .black
        view.addSubview(card)
        // saves are the game's own files in Caches/games/nfsse/save, already written
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { exit(0) }
    }

    func setBackground(_ on: Bool) {
        renfs_host_pause(on)
        if on { audio.pause() } else { audio.resume() }
    }

    private func askForText() {
        guard presentedViewController == nil else { return }
        let a = UIAlertController(title: "Type", message: "The letters go to the game (a name, a code). Enter confirms in the game.", preferredStyle: .alert)
        a.addTextField { t in
            t.autocapitalizationType = .words
            t.autocorrectionType = .no
        }
        a.addAction(UIAlertAction(title: "Type it", style: .default) { [weak self] _ in
            self?.controllerUserInteractionEnabled = false
            let text = a.textFields?.first?.text ?? ""
            if !text.isEmpty { text.withCString { renfs_host_text($0) } }
        })
        a.addAction(UIAlertAction(title: "Cancel", style: .cancel) { [weak self] _ in self?.controllerUserInteractionEnabled = false })
        // the alert needs the focus engine: let the controllers drive UIKit while it is up
        controllerUserInteractionEnabled = true
        present(a, animated: true)
    }

    // MARK: keyboard (a hardware keyboard, the Simulator's Mac keyboard) when GCKeyboard isn't there

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var handled = false
        for p in presses {
            if let k = p.key, GCKeyboard.coalesced == nil {
                renfs_host_key(Int32(k.keyCode.rawValue), true)
                handled = true
            }
        }
        if !handled { super.pressesBegan(presses, with: event) }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var handled = false
        for p in presses {
            if let k = p.key, GCKeyboard.coalesced == nil {
                renfs_host_key(Int32(k.keyCode.rawValue), false)
                handled = true
            }
        }
        if !handled { super.pressesEnded(presses, with: event) }
    }

    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        pressesEnded(presses, with: event)
    }

    // MARK: drawing

    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

    func draw(in view: MTKView) {
        input.poll()
        // the screen saver: kept away while someone plays (any input in the last 10 minutes); a replay or the
        // showcase left alone longer lets the TV sleep as usual (ReNFS doesn't tell the app what screen it is on)
        let want = CACurrentMediaTime() - NFSSEInput.lastInput < 600
        if UIApplication.shared.isIdleTimerDisabled != want { UIApplication.shared.isIdleTimerDisabled = want }
        guard let device = view.device, let pipeline, let queue else { return }
        var w: Int32 = 0, h: Int32 = 0
        var serial: UInt64 = 0
        if let px = renfs_host_frame_lock(&w, &h, &serial), serial != lastSerial, w > 0, h > 0 {
            if texture == nil || texture!.width != Int(w) || texture!.height != Int(h) {
                let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .bgra8Unorm, width: Int(w), height: Int(h), mipmapped: false)
                d.usage = .shaderRead
                texture = device.makeTexture(descriptor: d)
            }
            texture?.replace(region: MTLRegionMake2D(0, 0, Int(w), Int(h)), mipmapLevel: 0, withBytes: px, bytesPerRow: Int(w) * 4)
            lastSerial = serial
        }
        renfs_host_frame_unlock()

        guard let rpd = view.currentRenderPassDescriptor, let drawable = view.currentDrawable,
              let cb = queue.makeCommandBuffer(), let enc = cb.makeRenderCommandEncoder(descriptor: rpd) else { return }
        if let texture {
            // 4:3 in the middle (the original's shape for every mode: 320x200 had tall pixels)
            let ds = view.drawableSize
            var vw = ds.width, vh = ds.width * 3 / 4
            if vh > ds.height { vh = ds.height; vw = vh * 4 / 3 }
            enc.setViewport(MTLViewport(originX: (ds.width - vw) / 2, originY: (ds.height - vh) / 2, width: vw, height: vh, znear: 0, zfar: 1))
            enc.setRenderPipelineState(pipeline)
            enc.setFragmentTexture(texture, index: 0)
            enc.setFragmentSamplerState(sampler, index: 0)
            var u = SIMD4<Float>(Float(texture.width), Float(texture.height), Float(vw), Float(vh))
            enc.setFragmentBytes(&u, length: MemoryLayout<SIMD4<Float>>.size, index: 0)
            enc.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        }
        enc.endEncoding()
        cb.present(drawable)
        cb.commit()
    }

    /// A quad over the viewport; "sharp bilinear": each source pixel a flat square, only the seams between them
    /// blended over one screen pixel.
    private static let shader = """
    #include <metal_stdlib>
    using namespace metal;
    struct VOut { float4 pos [[position]]; float2 uv; };
    vertex VOut v_main(uint id [[vertex_id]]) {
        float2 p = float2((id & 1) ? 1.0 : -1.0, (id & 2) ? -1.0 : 1.0);
        VOut o; o.pos = float4(p, 0, 1); o.uv = float2((id & 1) ? 1.0 : 0.0, (id & 2) ? 1.0 : 0.0); return o;
    }
    fragment float4 f_main(VOut in [[stage_in]], texture2d<float> t [[texture(0)]], sampler s [[sampler(0)]],
                           constant float4 &u [[buffer(0)]]) {
        float2 src = u.xy, scale = u.zw / u.xy;
        float2 texel = in.uv * src;
        float2 base = floor(texel - 0.5) + 0.5;
        float2 f = clamp((texel - base - 0.5) * scale + 0.5, 0.0, 1.0);
        return float4(t.sample(s, (base + f) / src).rgb, 1.0);
    }
    """
}

/// The game's sound: AVAudioEngine pulls 44.1 kHz stereo from the host (the session is set up by AudioSetup).
final class NFSSEAudio {
    private let engine = AVAudioEngine()
    private var node: AVAudioSourceNode?
    private let scratch = UnsafeMutablePointer<Int16>.allocate(capacity: 2 * 4096)

    func start() {
        guard node == nil, let fmt = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 2) else { return }
        let scratch = self.scratch
        let n = AVAudioSourceNode(format: fmt) { _, _, frameCount, abl in
            let list = UnsafeMutableAudioBufferListPointer(abl)
            guard list.count >= 2, let l = list[0].mData?.assumingMemoryBound(to: Float.self),
                  let r = list[1].mData?.assumingMemoryBound(to: Float.self) else { return noErr }
            var done = 0
            let total = Int(frameCount)
            while done < total {
                let k = min(4096, total - done)
                renfs_host_audio_render(scratch, Int32(k))
                for i in 0..<k {
                    l[done + i] = Float(scratch[2 * i]) / 32768
                    r[done + i] = Float(scratch[2 * i + 1]) / 32768
                }
                done += k
            }
            return noErr
        }
        node = n
        engine.attach(n)
        engine.connect(n, to: engine.mainMixerNode, format: fmt)
        do { try engine.start() } catch { NSLog("NFS SE: audio did not start: %@", "\(error)") }
    }

    func pause() { engine.pause() }
    func resume() { try? engine.start() }
    func stop() { engine.stop() }
}

/// Controllers, the Siri Remote and a keyboard → the game (renfs_host_pad / renfs_host_key).
final class NFSSEInput {
    var onTextRequest: (() -> Void)?
    /// When a key, button, stick or trigger last changed (the screen saver's clock).
    static var lastInput = CACurrentMediaTime()
    private var lastPad: [Int32] = []
    private var remoteHeld: Int32 = 0         // the arrow key the remote's touch surface holds (HID usage)
    private var remoteNext: CFTimeInterval = 0
    private var observers: [NSObjectProtocol] = []
    private var script: [(at: Double, key: String)] = []
    private var scriptStart: CFTimeInterval = 0
    private static var engine: CHHapticEngineBox?

    func start() {
        let nc = NotificationCenter.default
        observers.append(nc.addObserver(forName: .GCControllerDidConnect, object: nil, queue: .main) { [weak self] _ in self?.wire() })
        observers.append(nc.addObserver(forName: .GCKeyboardDidConnect, object: nil, queue: .main) { [weak self] _ in self?.wire() })
        wire()
    }

    func stop() {
        observers.forEach { NotificationCenter.default.removeObserver($0) }
        observers = []
        renfs_host_pad(false, 0, 0, 0, 0, 0)
    }

    private func wire() {
        GCKeyboard.coalesced?.keyboardInput?.keyChangedHandler = { _, _, code, pressed in
            NFSSEInput.lastInput = CACurrentMediaTime()
            renfs_host_key(Int32(code.rawValue), pressed)
        }
        for c in GCController.controllers() {
            if let g = c.extendedGamepad {
                g.buttonOptions?.pressedChangedHandler = { [weak self] _, _, down in if down { self?.onTextRequest?() } }
            } else if let m = c.microGamepad {
                // the Siri Remote
                m.reportsAbsoluteDpadValues = false
                m.allowsRotation = false
                m.buttonA.pressedChangedHandler = { _, _, down in renfs_host_key(40, down) }          // click: Enter
                m.buttonMenu.pressedChangedHandler = { _, _, down in renfs_host_key(41, down) }       // Back: Esc
                m.buttonX.pressedChangedHandler = { [weak self] _, _, down in if down { self?.onTextRequest?() } }
            }
        }
    }

    /// Every displayed frame: the pad's state, the remote's arrows, the test script.
    func poll() {
        if let g = GCController.controllers().compactMap({ $0.extendedGamepad }).first {
            func ax(_ v: Float) -> Int16 { Int16(max(-32768, min(32767, v * 32767))) }
            func tr(_ v: Float) -> Int16 { Int16(max(0, min(32767, v * 32767))) }
            var b: UInt32 = 0
            let map: [(GCControllerButtonInput?, Int)] = [
                (g.buttonA, 0), (g.buttonB, 1), (g.buttonX, 2), (g.buttonY, 3), (g.buttonOptions, 4), (g.buttonMenu, 6),
                (g.leftThumbstickButton, 7), (g.rightThumbstickButton, 8), (g.leftShoulder, 9), (g.rightShoulder, 10),
                (g.dpad.up, 11), (g.dpad.down, 12), (g.dpad.left, 13), (g.dpad.right, 14),
            ]
            for (btn, bit) in map where btn?.isPressed == true { b |= 1 << UInt32(bit) }
            // SDL's y is down-positive
            let st: [Int32] = [Int32(ax(g.leftThumbstick.xAxis.value)), Int32(ax(-g.leftThumbstick.yAxis.value)),
                               Int32(tr(g.leftTrigger.value)), Int32(tr(g.rightTrigger.value)), Int32(bitPattern: b)]
            // (a stick resting a little off centre isn't "playing")
            if lastPad.count != st.count || zip(st, lastPad).enumerated().contains(where: { i, p in i == 4 ? p.0 != p.1 : abs(p.0 - p.1) > 3000 }) {
                Self.lastInput = CACurrentMediaTime()
                lastPad = st
            }
            renfs_host_pad(true, Int16(st[0]), Int16(st[1]), Int16(st[2]), Int16(st[3]), b)
        } else {
            renfs_host_pad(false, 0, 0, 0, 0, 0)
        }
        pollRemote()
        pollScript()
    }

    private func pollRemote() {
        guard GCController.controllers().first(where: { $0.extendedGamepad != nil }) == nil,
              let m = GCController.controllers().compactMap({ $0.microGamepad }).first else {
            release()
            return
        }
        let x = m.dpad.xAxis.value, y = m.dpad.yAxis.value
        if x != 0 || y != 0 || m.buttonA.isPressed || m.buttonMenu.isPressed { Self.lastInput = CACurrentMediaTime() }
        var want: Int32 = 0
        if abs(x) > 0.5 || abs(y) > 0.5 { want = abs(x) > abs(y) ? (x > 0 ? 79 : 80) : (y > 0 ? 82 : 81) }
        let now = CACurrentMediaTime()
        if want != remoteHeld {
            release()
            if want != 0 {
                renfs_host_key(want, true)
                remoteHeld = want
                remoteNext = now + 0.08
            }
        } else if want != 0 && now >= remoteNext {
            // a short press per step, repeated while the finger stays at the edge
            renfs_host_key(want, false)
            renfs_host_key(want, true)
            remoteNext = now + 0.3
        }
    }

    private func release() {
        if remoteHeld != 0 { renfs_host_key(remoteHeld, false) }
        remoteHeld = 0
    }

    // MARK: test script (launch argument `-nfsh.seScript "4:enter 7:down 9:enter"`: seconds after start, then a key)

    func play(script s: String) {
        script = s.split(separator: " ").compactMap { tok in
            let p = tok.split(separator: ":", maxSplits: 1)
            guard p.count == 2, let t = Double(p[0]) else { return nil }
            return (t, String(p[1]))
        }
        scriptStart = CACurrentMediaTime()
    }

    private func pollScript() {
        guard let first = script.first, CACurrentMediaTime() - scriptStart >= first.at else { return }
        script.removeFirst()
        let keys: [String: Int32] = ["enter": 40, "esc": 41, "up": 82, "down": 81, "left": 80, "right": 79, "space": 44]
        if first.key == "quit" {
            renfs_host_quit()
        } else if first.key.hasPrefix("t") && first.key.count > 1 {
            String(first.key.dropFirst()).withCString { renfs_host_text($0) }
        } else if let k = keys[first.key] {
            renfs_host_key(k, true)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) { renfs_host_key(k, false) }
        }
        NSLog("NFS SE: script key %@", first.key)
    }

    // MARK: rumble (the game's force feedback commands, as a pad's two motors)

    static func rumble(low: UInt16, high: UInt16, ms: UInt32) {
        DispatchQueue.main.async {
            guard let c = GCController.controllers().first(where: { $0.extendedGamepad != nil }), let h = c.haptics else { return }
            if engine == nil || engine?.controller !== c { engine = CHHapticEngineBox(controller: c, haptics: h) }
            engine?.play(low: Float(low) / 65535, high: Float(high) / 65535, seconds: Double(ms) / 1000)
        }
    }
}

import CoreHaptics

/// One Core Haptics engine on the pad's handles: the strong motor as a low, heavy rumble, the weak as a sharper one.
final class CHHapticEngineBox {
    weak var controller: GCController?
    private let engine: CHHapticEngine?
    private var player: CHHapticAdvancedPatternPlayer?

    init(controller: GCController, haptics: GCDeviceHaptics) {
        self.controller = controller
        engine = haptics.createEngine(withLocality: .handles)
        try? engine?.start()
        engine?.resetHandler = { [weak self] in try? self?.engine?.start() }
    }

    func play(low: Float, high: Float, seconds: Double) {
        guard let engine else { return }
        try? player?.stop(atTime: CHHapticTimeImmediate)
        player = nil
        let strength = max(low, high)
        guard strength > 0.01, seconds > 0 else { return }
        let e = CHHapticEvent(eventType: .hapticContinuous, parameters: [
            CHHapticEventParameter(parameterID: .hapticIntensity, value: strength),
            CHHapticEventParameter(parameterID: .hapticSharpness, value: high > low ? 0.7 : 0.2),
        ], relativeTime: 0, duration: seconds)
        if let p = try? CHHapticPattern(events: [e], parameters: []), let pl = try? engine.makeAdvancedPlayer(with: p) {
            player = pl
            try? pl.start(atTime: CHHapticTimeImmediate)
        }
    }
}
