// OpenUnleashed for Apple TV — the OpenUnleashed server client (docs/ou-server.md). GPL-3.0-or-later.
import Foundation
import UIKit

/// The one "Server" setting (address + token), kept in UserDefaults (the only persistent storage on tvOS).
enum ServerSettings {
    static let addressKey = "ou.server.address"
    static let tokenKey = "ou.server.token"
    static let setKey = "ou.data.set"
    /// Default for this install: the Unraid server on the LAN (PROGRESS.md).
    static let defaultAddress = "tower.local"

    static var address: String {
        get { UserDefaults.standard.string(forKey: addressKey).flatMap { $0.isEmpty ? nil : $0 } ?? defaultAddress }
        set { UserDefaults.standard.set(newValue.trimmingCharacters(in: .whitespaces), forKey: addressKey) }
    }
    static var token: String {
        get { UserDefaults.standard.string(forKey: tokenKey) ?? "" }
        set { UserDefaults.standard.set(newValue.trimmingCharacters(in: .whitespaces), forKey: tokenKey) }
    }
    /// The chosen data set (`original`, `mod-<name>`).
    static var dataSet: String {
        get { UserDefaults.standard.string(forKey: setKey) ?? "original" }
        set { UserDefaults.standard.set(newValue, forKey: setKey) }
    }

    /// Base URLs to try for an address, in order. `*.ts.net` names try HTTPS first (a `tailscale serve`
    /// certificate), then plain HTTP on 8642; everything else plain HTTP on the LAN (ATS `NSAllowsLocalNetworking`).
    static func candidates(for raw: String) -> [URL] {
        let a = raw.trimmingCharacters(in: .whitespaces)
        guard !a.isEmpty else { return [] }
        if a.hasPrefix("http://") || a.hasPrefix("https://") {
            return URL(string: a).map { [$0] } ?? []
        }
        var host = a
        var port: String?
        // host:port (not an IPv6 literal)
        if let i = a.lastIndex(of: ":"), a.filter({ $0 == ":" }).count == 1 {
            host = String(a[..<i])
            port = String(a[a.index(after: i)...])
        }
        var out: [URL] = []
        if host.lowercased().hasSuffix(".ts.net") {
            if let u = URL(string: "https://\(host)\(port.map { ":\($0)" } ?? "")") { out.append(u) }
        }
        if let u = URL(string: "http://\(host):\(port ?? "8642")") { out.append(u) }
        return out
    }
}

struct ServerInfo: Decodable {
    let name: String
    let version: String
    let features: [String]
    let data_sets: [String]
    let data_state: String?
}

struct DataSetInfo: Decodable {
    let id: String
    let kind: String
    let files: Int
    let size: Int64
}

struct ManifestEntry: Codable {
    let path: String
    let size: Int64
    let sha256: String
}

struct Manifest: Codable {
    let format: Int
    let set: String
    let total_size: Int64
    let files: [ManifestEntry]
}

enum ServerError: LocalizedError {
    case unreachable(String)
    case unauthorized
    case http(Int, String)
    case badResponse

    var errorDescription: String? {
        switch self {
        case .unreachable(let a): return "Server unreachable (\(a)). Check the address, that the container runs, and (away from home) that Tailscale is on."
        case .unauthorized: return "The server refused the password. Enter the password from the ou-server container's settings."
        case .http(let c, let m): return "Server error \(c): \(m)"
        case .badResponse: return "The server sent something unexpected (is this an OpenUnleashed server?)."
        }
    }
}

/// A connection to one server (after `connect` found a working base URL).
final class ServerClient {
    let base: URL
    let token: String
    let session: URLSession

    init(base: URL, token: String) {
        self.base = base
        self.token = token
        let cfg = URLSessionConfiguration.default
        cfg.timeoutIntervalForRequest = 15
        cfg.waitsForConnectivity = false
        session = URLSession(configuration: cfg)
    }

    static var deviceName: String { UIDevice.current.name }

    func request(_ path: String, method: String = "GET") -> URLRequest {
        var r = URLRequest(url: base.appendingPathComponent(path))
        r.httpMethod = method
        if !token.isEmpty { r.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        r.setValue(ServerClient.deviceName, forHTTPHeaderField: "X-OU-Device")
        return r
    }

    func get<T: Decodable>(_ path: String, as: T.Type) async throws -> T {
        let (data, resp) = try await session.data(for: request(path))
        try Self.check(resp, data)
        guard let v = try? JSONDecoder().decode(T.self, from: data) else { throw ServerError.badResponse }
        return v
    }

    static func check(_ resp: URLResponse, _ data: Data) throws {
        guard let h = resp as? HTTPURLResponse else { throw ServerError.badResponse }
        if h.statusCode == 401 { throw ServerError.unauthorized }
        guard (200..<300).contains(h.statusCode) else {
            let msg = (try? JSONSerialization.jsonObject(with: data) as? [String: Any])?["error"] as? String ?? HTTPURLResponse.localizedString(forStatusCode: h.statusCode)
            throw ServerError.http(h.statusCode, msg)
        }
    }

    /// The first candidate URL that answers `/v1/info`.
    static func connect(address: String, token: String) async throws -> (ServerClient, ServerInfo) {
        var lastError: Error = ServerError.unreachable(address)
        for base in ServerSettings.candidates(for: address) {
            let c = ServerClient(base: base, token: token)
            do {
                let info = try await c.get("v1/info", as: ServerInfo.self)
                return (c, info)
            } catch let e as ServerError {
                if case .unauthorized = e { throw e }
                lastError = e
            } catch {
                lastError = ServerError.unreachable("\(base.absoluteString): \(error.localizedDescription)")
            }
        }
        throw lastError
    }

    func dataSets() async throws -> [DataSetInfo] {
        struct R: Decodable { let sets: [DataSetInfo] }
        return try await get("v1/data/sets", as: R.self).sets
    }

    func manifest(set: String) async throws -> (Manifest, Data) {
        let (data, resp) = try await session.data(for: request("v1/data/\(set)/manifest"))
        try Self.check(resp, data)
        guard let m = try? JSONDecoder().decode(Manifest.self, from: data) else { throw ServerError.badResponse }
        return (m, data)
    }

    /// URL of one file of a set (each path component percent-encoded).
    func fileURL(set: String, path: String) -> URL {
        var u = base.appendingPathComponent("v1/data").appendingPathComponent(set).appendingPathComponent("file")
        for c in path.split(separator: "/") { u.appendPathComponent(String(c)) }
        return u
    }
}
