// NFS Haven for Apple TV — "Find on network": Bonjour `_nfs-haven._tcp` (NSBonjourServices). GPL-3.0-or-later.
import Foundation

/// Browses for NFS Haven servers for a few seconds and resolves them to host:port.
final class ServerFinder: NSObject, NetServiceBrowserDelegate, NetServiceDelegate {
    struct Found { let name: String; let address: String }
    private let browser = NetServiceBrowser()
    private var services: [NetService] = []
    private var found: [Found] = []
    private var done: (([Found]) -> Void)?

    func find(seconds: TimeInterval = 4, completion: @escaping ([Found]) -> Void) {
        done = completion
        browser.delegate = self
        browser.searchForServices(ofType: "_nfs-haven._tcp.", inDomain: "local.")
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in self?.finish() }
    }

    private func finish() {
        browser.stop()
        services.forEach { $0.stop() }
        let r = found
        done?(r)
        done = nil
    }

    func netServiceBrowser(_ b: NetServiceBrowser, didFind s: NetService, moreComing: Bool) {
        services.append(s)
        s.delegate = self
        s.resolve(withTimeout: 3)
    }

    func netServiceDidResolveAddress(_ s: NetService) {
        guard let host = s.hostName else { return }
        let h = host.hasSuffix(".") ? String(host.dropLast()) : host
        let a = s.port == 8642 ? h : "\(h):\(s.port)"
        if !found.contains(where: { $0.address == a }) { found.append(Found(name: s.name, address: a)) }
    }
}
