import XCTest
@testable import Kadrell

final class GitHubExtensionsTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-store-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: root)
    }

    // MARK: Suche

    func testSearchURLCarriesTopicAndTerms() {
        let c = URLComponents(url: GitHubExtensions.searchURL("jira ticket"), resolvingAgainstBaseURL: false)!
        XCTAssertEqual(c.queryItems?.first { $0.name == "q" }?.value, "topic:kadrell-extension jira ticket")
        XCTAssertEqual(c.queryItems?.first { $0.name == "sort" }?.value, "stars")
        XCTAssertEqual(c.queryItems?.first { $0.name == "per_page" }?.value, "20")
    }

    func testParseReadsItems() {
        let json = Data("""
        {"items":[{"name":"jira","html_url":"https://github.com/me/jira","description":"Jira panel","stargazers_count":42,"owner":{"login":"me"}}]}
        """.utf8)
        XCTAssertEqual(GitHubExtensions.parse(json),
                       [RemoteExtension(owner: "me", repo: "jira", description: "Jira panel", stars: 42,
                                        htmlURL: URL(string: "https://github.com/me/jira")!)])
    }

    func testParseToleratesGarbage() {
        XCTAssertEqual(GitHubExtensions.parse(Data("nonsense".utf8)), [])
    }

    // MARK: Installieren aus Tarball

    func testInstallTarballPlacesTrustedExtensionUnderManifestName() async throws {
        let data = try makeTarball(repo: "my-repo", extName: "demo", mode: 0o777)
        let catalog = root.appendingPathComponent("catalog")
        let name = try await GitHubExtensions.installTarball(data, fallbackName: "my-repo", into: catalog)
        XCTAssertEqual(name, "demo", "der Name aus dem Manifest bestimmt den Ordner, nicht der Repo-Name")
        let found = ExtensionCatalog.scan(catalog)
        XCTAssertEqual(found.map(\.name), ["demo"])
        XCTAssertNil(found.first?.problem, "nach dem Härten gehört alles dir und ist weder für Gruppe noch andere schreibbar")
    }

    func testInstallTarballFallsBackToRepoNameWithoutManifestName() async throws {
        let data = try makeTarball(repo: "repo", extName: "", mode: 0o755) // Manifest ohne name-Feld
        let name = try await GitHubExtensions.installTarball(data, fallbackName: "repo", into: root.appendingPathComponent("c"))
        XCTAssertEqual(name, "repo")
    }

    func testInstallTarballRejectsRepoWithoutManifest() async throws {
        let data = try makeTarball(repo: "repo", extName: nil, mode: 0o755)
        do {
            _ = try await GitHubExtensions.installTarball(data, fallbackName: "repo", into: root.appendingPathComponent("c"))
            XCTFail("ohne kadrell.json darf nichts installiert werden")
        } catch GitHubExtensions.InstallError.empty {}
    }

    func testInstallTarballOverwritesOnUpdateFromSameOrigin() async throws {
        let catalog = root.appendingPathComponent("catalog")
        _ = try await GitHubExtensions.installTarball(try makeTarball(repo: "r", extName: "demo", version: "1.0.0"),
                                                      fallbackName: "r", into: catalog, origin: "a/x")
        _ = try await GitHubExtensions.installTarball(try makeTarball(repo: "r", extName: "demo", version: "2.0.0"),
                                                      fallbackName: "r", into: catalog, origin: "a/x", knownOrigins: ["demo": "a/x"])
        XCTAssertEqual(ExtensionCatalog.scan(catalog).first?.manifest?.version, "2.0.0")
    }

    /// H2: ein fremdes Repo darf einen schon belegten Namen nicht überschreiben.
    func testInstallTarballRefusesHijackFromDifferentOrigin() async throws {
        let catalog = root.appendingPathComponent("catalog")
        _ = try await GitHubExtensions.installTarball(try makeTarball(repo: "jira", extName: "jira", version: "1.0.0"),
                                                      fallbackName: "jira", into: catalog, origin: "a/jira")
        do {
            _ = try await GitHubExtensions.installTarball(try makeTarball(repo: "jira-fork", extName: "jira", version: "9.9.9"),
                                                          fallbackName: "jira-fork", into: catalog,
                                                          origin: "b/jira-fork", knownOrigins: ["jira": "a/jira"])
            XCTFail("fremde Herkunft darf den Namen nicht kapern")
        } catch GitHubExtensions.InstallError.occupied {}
        XCTAssertEqual(ExtensionCatalog.scan(catalog).first?.manifest?.version, "1.0.0", "das Original bleibt unangetastet")
    }

    /// Ein Manifest-Name mit Pfadtrennern bleibt im Katalog (safeName), bricht nicht aus.
    func testInstallTarballSanitizesTraversalName() async throws {
        let catalog = root.appendingPathComponent("catalog")
        let name = try await GitHubExtensions.installTarball(try makeTarball(repo: "r", extName: "../../evil"),
                                                             fallbackName: "r", into: catalog)
        XCTAssertEqual(name, "evil")
        XCTAssertTrue(FileManager.default.fileExists(atPath: catalog.appendingPathComponent("evil").path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("evil").path), "nichts außerhalb des Katalogs")
    }

    /// N6: Nicht-ASCII-Zeichen (Homoglyphen) fallen aus dem Ordnernamen.
    func testInstallTarballStripsNonAsciiFromName() async throws {
        let catalog = root.appendingPathComponent("catalog")
        let name = try await GitHubExtensions.installTarball(try makeTarball(repo: "r", extName: "café-x"),
                                                             fallbackName: "r", into: catalog)
        XCTAssertEqual(name, "caf-x")
    }

    /// Ein Name nur aus Punkten wird nach safeName leer und abgelehnt.
    func testInstallTarballRejectsDotOnlyName() async throws {
        do {
            _ = try await GitHubExtensions.installTarball(try makeTarball(repo: "r", extName: ".."),
                                                          fallbackName: "", into: root.appendingPathComponent("c"))
            XCTFail("leerer Name nach safeName muss abgelehnt werden")
        } catch GitHubExtensions.InstallError.empty {}
    }

    /// Baut ein `tar.gz` wie GitHub: ein oberster Ordner `<repo>-main/` mit `kadrell.json` und `init.lua`.
    /// `extName` nil lässt das Manifest weg, "" schreibt es ohne name-Feld; `mode` setzt die Rechte vor dem Packen.
    private func makeTarball(repo: String, extName: String?, mode: Int16 = 0o755, version: String = "1.0.0") throws -> Data {
        let fm = FileManager.default
        let work = root.appendingPathComponent("src-\(UUID().uuidString)")
        let top = work.appendingPathComponent("\(repo)-main")
        try fm.createDirectory(at: top, withIntermediateDirectories: true)
        if let extName {
            var manifest: [String: Any] = ["version": version, "apiVersion": 1]
            if !extName.isEmpty { manifest["name"] = extName }
            let file = top.appendingPathComponent("kadrell.json")
            try JSONSerialization.data(withJSONObject: manifest).write(to: file)
            try fm.setAttributes([.posixPermissions: mode], ofItemAtPath: file.path)
        }
        let initLua = top.appendingPathComponent("init.lua")
        try Data("-- demo".utf8).write(to: initLua)
        try fm.setAttributes([.posixPermissions: mode], ofItemAtPath: initLua.path)
        try fm.setAttributes([.posixPermissions: mode], ofItemAtPath: top.path)
        let tar = work.appendingPathComponent("out.tar.gz")
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
        p.arguments = ["-czf", tar.path, "-C", work.path, "\(repo)-main"]
        try p.run()
        p.waitUntilExit()
        return try Data(contentsOf: tar)
    }
}
