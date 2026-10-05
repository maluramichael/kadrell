import Foundation

/// Eine Extension in einem öffentlichen GitHub-Repo mit Topic `kadrell-extension`: ein Treffer der Marktplatz-Suche.
struct RemoteExtension: Equatable, Identifiable {
    var owner: String
    var repo: String
    var description: String
    var stars: Int
    var htmlURL: URL
    var id: String { "\(owner)/\(repo)" }
    var fullName: String { "\(owner)/\(repo)" }
}

/// Der „Marktplatz": GitHub nach Repos mit Topic `kadrell-extension` durchsuchen und eins in den Katalog laden.
/// Kein eigener Server, nur die öffentliche Such-API und der Tarball des Repos. Installiert wird in denselben
/// Ordner wie von Hand gelegte Extensions, der `FolderWatcher` lässt sie danach erscheinen.
enum GitHubExtensions {
    static let topic = "kadrell-extension"
    /// Obergrenze für den heruntergeladenen Tarball: eine Extension ist klein, ein riesiges Repo würde sonst den
    /// Speicher des Einzelprozesses erschöpfen (alle Sessions hängen daran).
    static let maxDownloadBytes = 25 * 1024 * 1024

    enum InstallError: LocalizedError {
        case download, extract, empty, tooLarge, occupied(String)
        var errorDescription: String? {
            switch self {
            case .download: String(localized: "Download von GitHub fehlgeschlagen", bundle: Bundle.app)
            case .extract: String(localized: "Archiv ließ sich nicht entpacken", bundle: Bundle.app)
            case .empty: String(localized: "Repo enthält keine Extension (kadrell.json fehlt)", bundle: Bundle.app)
            case .tooLarge: String(localized: "Repo ist zu groß (über \(maxDownloadBytes / 1024 / 1024) MB)", bundle: Bundle.app)
            case .occupied(let name): String(localized: "Name \(name) ist schon von einer anderen Extension belegt, erst entfernen", bundle: Bundle.app)
            }
        }
    }

    /// Such-URL: immer der Topic, dazu die freien Wörter des Nutzers, nach Sternen sortiert.
    static func searchURL(_ query: String) -> URL {
        var c = URLComponents(string: "https://api.github.com/search/repositories")!
        let terms = ["topic:\(topic)"] + query.split(separator: " ").map(String.init)
        c.queryItems = [URLQueryItem(name: "q", value: terms.joined(separator: " ")),
                        URLQueryItem(name: "sort", value: "stars"),
                        URLQueryItem(name: "per_page", value: "20")]
        return c.url!
    }

    /// Antwort der Such-API in Treffer übersetzen; Einträge ohne gültige URL fallen weg.
    static func parse(_ data: Data) -> [RemoteExtension] {
        guard let root = try? JSONDecoder().decode(SearchResponse.self, from: data) else { return [] }
        return root.items.compactMap { item in
            URL(string: item.html_url).map {
                RemoteExtension(owner: item.owner.login, repo: item.name,
                                description: item.description ?? "", stars: item.stargazers_count, htmlURL: $0)
            }
        }
    }

    static func search(_ query: String) async throws -> [RemoteExtension] {
        let (data, resp) = try await URLSession.shared.data(for: request(searchURL(query)))
        // Ohne Status-Prüfung sähe der Nutzer bei 403 (Rate-Limit) „keine Treffer“ statt eines Fehlers.
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else { throw InstallError.download }
        return parse(data)
    }

    /// Lädt den Tarball des Repos (Standard-Branch), entpackt ihn und legt die Extension unter `<catalogDir>/<name>`
    /// ab. `knownOrigins` (name → owner/repo) verhindert, dass ein fremdes Repo einen belegten Namen überschreibt.
    /// Liefert den Ordnernamen.
    @discardableResult
    static func install(_ remote: RemoteExtension, into catalogDir: URL, knownOrigins: [String: String] = [:]) async throws -> String {
        let url = URL(string: "https://api.github.com/repos/\(remote.owner)/\(remote.repo)/tarball")!
        let data = try await download(url)
        return try await installTarball(data, fallbackName: remote.repo, into: catalogDir,
                                        origin: remote.fullName, knownOrigins: knownOrigins)
    }

    /// Streamt den Tarball mit harter Byte-Obergrenze, statt ihn unbegrenzt in den Speicher zu laden.
    static func download(_ url: URL) async throws -> Data {
        let (stream, resp) = try await URLSession.shared.bytes(for: request(url))
        guard let http = resp as? HTTPURLResponse, http.statusCode == 200 else { throw InstallError.download }
        if http.expectedContentLength > maxDownloadBytes { throw InstallError.tooLarge }
        var data = Data()
        for try await byte in stream {
            data.append(byte)
            if data.count > maxDownloadBytes { throw InstallError.tooLarge }
        }
        return data
    }

    /// Entpackt einen heruntergeladenen `tar.gz` und legt die Extension in den Katalog. Getrennt von `install`,
    /// damit die Ablage ohne Netz prüfbar ist. Der Repo-Ordner im Archiv wird abgeschnitten (`--strip-components 1`).
    /// `tar` (libarchive) wehrt `../`, absolute Pfade und Symlinks nach außen selbst ab.
    @discardableResult
    static func installTarball(_ data: Data, fallbackName: String, into catalogDir: URL,
                               origin: String = "", knownOrigins: [String: String] = [:]) async throws -> String {
        let fm = FileManager.default
        let tmp = fm.temporaryDirectory.appendingPathComponent("kadrell-install-\(UUID().uuidString)")
        let dest = tmp.appendingPathComponent("x")
        try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: tmp) }
        let tar = tmp.appendingPathComponent("src.tar.gz")
        try data.write(to: tar)
        let r = try await ProcessRunner.run("/usr/bin/tar", ["-xzf", tar.path, "-C", dest.path, "--strip-components", "1"])
        guard r.status == 0 else { throw InstallError.extract }
        guard fm.fileExists(atPath: dest.appendingPathComponent("kadrell.json").path) else { throw InstallError.empty }
        let name = safeName(manifestName(dest) ?? fallbackName)
        guard !name.isEmpty else { throw InstallError.empty }
        // Wie bei Shell-Hooks verlangt die Trust-Prüfung: nicht für Gruppe oder andere beschreibbar.
        _ = try await ProcessRunner.run("/bin/chmod", ["-R", "go-w", dest.path])
        let target = catalogDir.appendingPathComponent(name)
        try fm.createDirectory(at: catalogDir, withIntermediateDirectories: true)
        if fm.fileExists(atPath: target.path) {
            // Nur das Repo, das den Namen vorher installiert hat, darf ihn aktualisieren.
            guard knownOrigins[name] == origin else { throw InstallError.occupied(name) }
            try fm.removeItem(at: target)
        }
        try fm.moveItem(at: dest, to: target)
        return name
    }

    private static func request(_ url: URL) -> URLRequest {
        var req = URLRequest(url: url)
        req.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        req.setValue("Kadrell", forHTTPHeaderField: "User-Agent") // ohne User-Agent antwortet die GitHub-API mit 403.
        return req
    }

    private static func manifestName(_ dir: URL) -> String? {
        guard let data = try? Data(contentsOf: dir.appendingPathComponent("kadrell.json")),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let name = obj["name"] as? String, !name.isEmpty else { return nil }
        return name
    }

    /// Nur harmlose Zeichen und keine führenden Punkte: ein Name mit „/" oder „.." darf nicht aus dem Katalog ausbrechen.
    private static func safeName(_ s: String) -> String {
        let kept = s.filter { $0.isLetter || $0.isNumber || "._-".contains($0) }
        return String(kept.drop { $0 == "." })
    }

    private struct SearchResponse: Decodable {
        struct Owner: Decodable { var login: String }
        struct Item: Decodable {
            var name: String
            var html_url: String
            var description: String?
            var stargazers_count: Int
            var owner: Owner
        }
        var items: [Item]
    }
}
