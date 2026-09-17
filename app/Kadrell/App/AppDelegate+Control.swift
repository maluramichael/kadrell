import AppKit

/// Führt `kadrell <befehl>` aus (siehe `ControlCommand.usage`). Alles läuft über dieselben Wege wie die Bedienung
/// per Maus und Tastatur, nur ohne Rückfragen.
extension AppDelegate {
    func startControlServer() {
        let server = ControlServer { [weak self] req, pid in
            await self?.handleControl(req, peer: pid) ?? .fail("Kadrell beendet sich")
        }
        do {
            try server.start()
            controlServer = server
        } catch {
            AppDelegate.log.error("Steuer-Socket: \(String(describing: error), privacy: .public)")
        }
    }

    /// `registry`/`attach` fehlen nur kurz beim Start (`boot()` läuft noch): eigener Status statt harter Fehler,
    /// den `ControlClient.run` mit abwartet statt sofort „läuft, lauscht aber nicht“ zu melden.
    func handleControl(_ req: ControlRequest, peer: pid_t) async -> ControlResponse {
        guard registry != nil, attach != nil else { return ControlResponse(status: ControlResponse.startingStatus, stdout: "", stderr: "Kadrell startet noch\n") }
        var req = req
        req.caller = ControlCaller.session(pid: peer, terminals: attach.pids)
        do {
            return try await runControl(ControlCommand.parse(req.argv), req)
        } catch let e as ControlError {
            return .fail(e.message)
        } catch {
            return .fail(String(describing: error))
        }
    }

    private func runControl(_ cmd: ControlCommand, _ req: ControlRequest) async throws -> ControlResponse {
        switch cmd {
        case .help: return .ok(ControlCommand.usage)
        case .list(let json): return .ok(try listOutput(json: json))
        case let .newGroup(dir, name, color): return .ok(try controlNewGroup(dir: dir, name: name, color: color, req))
        case let .newSession(t, dir, name, detached, prompt, resume):
            return .ok(try controlNewSession(target: t, dir: dir, name: name, detached: detached, prompt: prompt, resume: resume, req))
        case let .capture(t, all): return .ok(try controlCapture(target: t, all: all, req))
        default:
            try await performControl(cmd, req)
            return .ok()
        }
    }

    /// Befehle ohne Ausgabe.
    private func performControl(_ cmd: ControlCommand, _ req: ControlRequest) async throws {
        switch cmd {
        case let .select(t, add): try controlSelect(target: t, add: add, req)
        case .layout(let m): workspace.setMode(m)
        case .zoom(let t): try controlZoom(target: t, req)
        case let .rename(t, name): registry.rename(try session(t, req).id, to: name)
        case let .move(t, target):
            store.attach(sessionId: try session(t, req).id, to: try group(target, req).id)
            reloadViews()
        case let .setGroup(t, name, color, favorite): try controlSetGroup(target: t, name: name, color: color, favorite: favorite, req)
        case .stop(let t): try controlStop(target: t, req)
        case .resume(let t): attach.attachNow(try session(t, req))
        case .killSession(let t): closeSession(try session(t, req).id, force: true)
        case .killGroup(let t): closeGroup(try group(t, req).id, force: true)
        case let .send(t, text, enter, keys): try await controlSend(target: t, text: text, enter: enter, keys: keys, req)
        case let .status(t, state, sessionId, title, waitingFor, message, firstPrompt):
            try controlStatus(target: t, state: state, sessionId: sessionId, title: title, waitingFor: waitingFor, message: message, firstPrompt: firstPrompt, req)
        case .help, .list, .newGroup, .newSession, .capture: break
        }
    }

    /// Ohne -t meldet sich die aufrufende Kachel selbst; von außen muss das Ziel genannt sein.
    private func controlStatus(target t: String?, state: String, sessionId: String?, title: String?, waitingFor: String?, message: String?, firstPrompt: String?, _ req: ControlRequest) throws {
        guard t != nil || req.caller != nil else { throw ControlError("status braucht -t, wenn es nicht aus einer Session kommt") }
        registry.report(try session(t, req).id, state: state, sessionId: sessionId, title: title, waitingFor: waitingFor, message: message, firstPrompt: firstPrompt)
    }

    private func controlSelect(target t: String?, add: Bool, _ req: ControlRequest) throws {
        switch try ControlTarget.sessionOrGroup(t, groups: store.groups, sessions: registry.sessions, caller: req.caller, focused: workspace.focused) {
        case .session(let s): workspace.select([s.id], add: add)
        case .group(let g): workspace.select(g.sessionIds, add: add)
        }
    }

    private func controlStop(target t: String?, _ req: ControlRequest) throws {
        let s = try session(t, req)
        guard attach.isAttached(s.id) else { throw ControlError("„\(s.title)“ läuft nicht") }
        attach.stop(s.id)
    }

    /// Ziel einer Session-Aktion, aus einer Kachel heraus nur im erlaubten Bereich (`ControlCaller.allowed`).
    private func session(_ t: String?, _ req: ControlRequest) throws -> Session {
        let s = try ControlTarget.session(t, sessions: registry.sessions, caller: req.caller, focused: workspace.focused)
        guard ControlCaller.allowed(session: s.id, groups: store.groups, caller: req.caller, othersAllowed: Settings.controlOtherSessions) else {
            throw ControlError("„\(s.title)“ gehört nicht zur Gruppe dieser Session (Einstellung „Sessions dürfen andere Sessions steuern“)")
        }
        return s
    }

    private func group(_ t: String?, _ req: ControlRequest, restricted: Bool = true) throws -> Group {
        let g = try ControlTarget.group(t, groups: store.groups, sessions: registry.sessions, caller: req.caller, focused: workspace.focused)
        guard !restricted || ControlCaller.allowed(group: g, caller: req.caller, othersAllowed: Settings.controlOtherSessions) else {
            throw ControlError("Gruppe „\(g.name)“ ist nicht die dieser Session (Einstellung „Sessions dürfen andere Sessions steuern“)")
        }
        return g
    }

    private func existingDir(_ p: String, _ req: ControlRequest) throws -> String {
        let path = ControlTarget.path(p, cwd: req.cwd)
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDir), isDir.boolValue else { throw ControlError("kein Ordner: \(path)") }
        return path
    }

    /// Favorit, sonst räumt `GroupStore.assign` die leere Gruppe beim nächsten Abgleich wieder weg.
    private func controlNewGroup(dir: String, name: String?, color: String?, _ req: ControlRequest) throws -> String {
        var g = store.makeGroup(cwd: try existingDir(dir, req), name: name)
        if let color { g.color = color }
        g.favorite = true
        store.add(g)
        reloadViews()
        return g.id
    }

    /// Ohne -t und -c landet die Session bei der aufrufenden: gleiche Gruppe, gleicher Ordner (wie tmux new-window).
    private func controlNewSession(target: String?, dir: String?, name: String?, detached: Bool, prompt: String?, resume: String?, _ req: ControlRequest) throws -> String {
        if let resume { try checkResumable(resume) }
        var g = try target.map { try group($0, req, restricted: false) }
        let caller = req.caller.flatMap { c in registry.sessions.first { $0.id == c } }
        if target == nil, dir == nil, let caller { g = store.group(forSession: caller.id) }
        let cwd = try dir.map { try existingDir($0, req) } ?? (target == nil ? caller?.cwd : nil) ?? g?.cwd ?? existingDir(req.cwd, req)
        let key = startSession(group: g, cwd: cwd, show: !detached, prompt: prompt, sessionId: resume)
        if let name { registry.rename(key, to: name) }
        return key
    }

    /// Dieselbe Konversation in zwei Prozessen schreibt durcheinander ins Transcript: nur übernehmen, was nirgends mehr läuft.
    private func checkResumable(_ sessionId: String) throws {
        guard !registry.sessions.contains(where: { $0.sessionId == sessionId || $0.id == sessionId }) else { throw ControlError("\(sessionId) ist schon in Kadrell") }
        guard Transcript.path(sessionId: sessionId, configDir: registry.cli.configDir) != nil else { throw ControlError("kein Transcript für \(sessionId)") }
        let dir = registry.cli.configDir + "/sessions"
        let files = (try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? []
        let pids = files.compactMap { Int($0.replacingOccurrences(of: ".json", with: "")) }.filter { kill(pid_t($0), 0) == 0 }
        if let live = Agent.local(pids: pids, configDir: registry.cli.configDir).first(where: { $0.sessionId == sessionId }) {
            throw ControlError("\(sessionId) läuft noch (pid \(live.pid ?? 0)), erst dort beenden")
        }
    }

    private func controlSetGroup(target: String?, name: String?, color: String?, favorite: Bool?, _ req: ControlRequest) throws {
        var g = try group(target, req)
        g.name = name ?? g.name
        g.color = color ?? g.color
        if let favorite { g.favorite = favorite }
        store.update(g)
        reloadViews()
    }

    private func controlZoom(target: String?, _ req: ControlRequest) throws {
        if target != nil {
            let s = try session(target, req)
            if !workspace.selected.contains(s.id) { workspace.select([s.id], add: true) }
            workspace.setFocus(s.id)
        }
        guard workspace.focused != nil else { throw ControlError("keine Kachel fokussiert") }
        workspace.toggleZen()
    }

    /// Text geht als Einfügen (bracketed paste), wenn Claude das eingeschaltet hat; ⏎ kommt getrennt hinterher,
    /// sonst hält die Eingabe es für einen Zeilenumbruch im eingefügten Text. Wartet das Enter ab, bevor die
    /// Antwort rausgeht: sonst startet ein direkt folgender `kadrell send` sein Enter noch vor diesem.
    private func controlSend(target: String?, text: String, enter: Bool, keys: Bool, _ req: ControlRequest) async throws {
        let s = try session(target, req)
        guard let term = attach.terminal(for: s.id) else { throw ControlError("„\(s.title)“ läuft nicht, erst kadrell resume -t \(s.id.prefix(8))") }
        if keys {
            term.send(txt: text.split(separator: " ").compactMap { ControlCommand.keyNames[$0.lowercased()] }.joined())
            return
        }
        let clean = text.replacingOccurrences(of: "\u{1b}[201~", with: "")
        term.send(txt: term.terminalStateSnapshot().bracketedPasteMode ? "\u{1b}[200~" + clean + "\u{1b}[201~" : clean)
        guard enter else { return }
        try? await Task.sleep(for: .milliseconds(150))
        term.send(txt: "\r")
    }

    private func controlCapture(target: String?, all: Bool, _ req: ControlRequest) throws -> String {
        let s = try session(target, req)
        guard let term = attach.terminal(for: s.id) else { return attach.lines(for: s.id).joined(separator: "\n") }
        if all { return String(decoding: term.getBufferAsData(kind: .active), as: UTF8.self) }
        var rows = term.terminalStateSnapshot().visibleRows.map { $0.text.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression) }
        while rows.last?.isEmpty == true { rows.removeLast() }
        return rows.joined(separator: "\n")
    }

    private func listOutput(json: Bool) throws -> String {
        struct S: Encodable { let key, sessionId, title, cwd, status: String; let branch: String?; let running, shown, focused: Bool }
        struct G: Encodable { let id, name, color, cwd: String; let favorite: Bool; let sessions: [S] }
        struct Out: Encodable { let layout: String; let focused: String?; let groups: [G] }
        let byKey = Dictionary(registry.sessions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let groups = store.groups.map { g in
            G(id: g.id, name: g.name, color: g.color, cwd: g.cwd, favorite: g.isFavorite, sessions: g.sessionIds.compactMap { byKey[$0] }.map { s in
                let running = attach.isAttached(s.id)
                return S(key: s.id, sessionId: s.sessionId, title: s.title, cwd: s.cwd,
                         status: running ? s.status.rawValue : attach.isEnded(s.id) ? "ended" : "stopped",
                         branch: s.branch, running: running, shown: workspace.selected.contains(s.id), focused: workspace.focused == s.id)
            })
        }
        if json {
            let enc = JSONEncoder()
            enc.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            return String(decoding: try enc.encode(Out(layout: workspace.mode.rawValue, focused: workspace.focused, groups: groups)), as: UTF8.self)
        }
        return groups.map { g in
            (["\(g.id.prefix(8))  \(g.name)\(g.favorite ? " ★" : "")  \(g.cwd)"] + g.sessions.map { s in
                let mark = s.focused ? "*" : s.shown ? "+" : " "
                return "  \(mark) \(s.key.prefix(8))  \(s.status.padding(toLength: 8, withPad: " ", startingAt: 0))  \(s.title)"
            }).joined(separator: "\n")
        }.joined(separator: "\n")
    }

    /// Symlink `~/.local/bin/kadrell` auf das Binary dieser App. Ein fremdes Programm unter dem Namen bleibt unangetastet.
    @objc func menuInstallCLI() {
        let link = NSHomeDirectory() + "/.local/bin/kadrell"
        let fm = FileManager.default
        do {
            guard let target = Bundle.main.executablePath else { throw ControlError("Pfad der App unbekannt") }
            try fm.createDirectory(atPath: (link as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
            if let existing = try? fm.destinationOfSymbolicLink(atPath: link) {
                if existing != target { try fm.removeItem(atPath: link) }
            } else if fm.fileExists(atPath: link) {
                throw ControlError("\(link) existiert schon und ist kein Symlink")
            }
            if (try? fm.destinationOfSymbolicLink(atPath: link)) == nil { try fm.createSymbolicLink(atPath: link, withDestinationPath: target) }
            sheets.inform(String(localized: "Kommandozeilen-Tool installiert", bundle: Bundle.app), String(localized: "\(link) zeigt auf diese App. `kadrell help` listet die Befehle. In Sessions dieser App steht der Pfad zusätzlich in $KADRELL.", bundle: Bundle.app))
        } catch {
            sheets.inform(String(localized: "Installation fehlgeschlagen", bundle: Bundle.app), (error as? ControlError)?.message ?? String(describing: error))
        }
    }
}
