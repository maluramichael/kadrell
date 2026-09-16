import XCTest
@testable import Kadrell

final class SSHConfigTests: XCTestCase {
    func testHostsFollowIncludesAndSkipPatterns() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-ssh-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("config.d"), withIntermediateDirectories: true)
        try """
        # Kommentar
        Host 192.168.2.69
          HostName 192.168.2.69
        Host *
          ServerAliveInterval 60
        Host examplehost vault
        Host !x
        Match all
        include config.d/*
        Host dup
        """.write(to: dir.appendingPathComponent("config"), atomically: true, encoding: .utf8)
        try "host peon\nInclude ../config\nHost dup\n".write(to: dir.appendingPathComponent("config.d/a"), atomically: true, encoding: .utf8)
        try "Host b?\n Host zed\n".write(to: dir.appendingPathComponent("config.d/b"), atomically: true, encoding: .utf8)
        XCTAssertEqual(SSHConfig.hosts(config: dir.appendingPathComponent("config")),
                       ["192.168.2.69", "examplehost", "vault", "peon", "dup", "zed"])
    }

    func testMissingConfigIsEmpty() {
        XCTAssertEqual(SSHConfig.hosts(config: URL(fileURLWithPath: "/nonexistent/config")), [])
    }
}
