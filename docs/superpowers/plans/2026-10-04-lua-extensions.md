# Lua-Extensions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Kadrell lädt Lua-Extensions als isolierte Kindprozesse, die Events bekommen, Kadrell per CLI-Befehlen steuern und eine rechte Sidebar sowie die Statusleiste befüllen, mit Live-Reload und einem Extensions-Dialog.

**Architecture:** Pro Extension startet Kadrell `Kadrell ext-host <ordner>` (gleiches Binary). Der Helper führt Lua 5.5.1 auf einer seriellen Queue aus, spricht JSON-Zeilen über stdin/stdout und erledigt `exec`, `http` und Timer selbst. Die Lua-Seite der API ist ein Lua-Prelude, die C-Schicht ist winzig (Zustand anlegen, Chunk laden, Nachricht zustellen, Nachricht senden). Auf der App-Seite trennt sich reine Logik (Manifest, Protokoll, Panel-Baum, Zustandsmaschine) von Prozessverwaltung und UI.

**Tech Stack:** Swift 6 / AppKit / SwiftUI, Lua 5.5.1 (C-Quellen im Repo), rxi/json.lua (MIT, eine Datei), XcodeGen 2.46.0, XCTest.

**Spec:** `docs/superpowers/specs/2026-10-04-lua-extensions-design.md`

## Global Constraints

- Lua 5.5.1 von https://www.lua.org/ftp/lua-5.5.1.tar.gz, SHA256 `1c4b4068d67061f2a2231ad2b5422e77acea1487ea9890f6320af614f4373dce`, vor dem Entpacken prüfen.
- json.lua von https://github.com/rxi/json.lua, auf einen Commit gepinnt, Commit-Hash im Dateikopf vermerken.
- Extensions-Ordner: `~/.config/kadrell/extensions/<name>/` mit `kadrell.json` und `init.lua`. Unterstützte `apiVersion`: `1`.
- Limits: `ready` binnen 3 s; `ping` alle 5 s, `pong` binnen 5 s; Abschalten `shutdown` → 1 s → SIGTERM → 1 s → SIGKILL; Backoff 1 s, 5 s, 30 s; 3 Abstürze in 120 s = bleibt aus; > 50 Nachrichten/s, Zeile > 1 MB, Panel > 2000 Knoten = Kill; Texte auf 500 Zeichen gekürzt; Statustext max. 40 Zeichen; Reload-Entprellung 300 ms; Log 200 Zeilen; `exec`/`http`-Timeout Default 30 s.
- Theme-Farbnamen: `accent`, `muted`, `ok`, `warn`, `err`.
- Kürzel-Defaults: ⌘3 Fokus rechte Sidebar, ⌘⌥B rechte Sidebar ein/aus, F4 Extensions-Dialog.
- Jeder Nutzertext in AppKit: `String(localized: "…", bundle: Bundle.app)`, Deutsch und Englisch in `app/Kadrell/Localization/Localizable.xcstrings`.
- Clean Build ohne eigene Warnungen; `cd app && uv tool run lizard Kadrell -w` leer (CCN ≤ 15). Lua-Quellen sind kein eigener Code und bekommen `-w`.
- Nie das eigene Kadrell anfassen. Tests: `TEST_RUNNER_KADRELL_PROFILE=tmp`. Manuell nur `open -n app/build/Build/Products/Debug/Kadrell.app --args --profile tmp`.
- Keine Em-Dashes in Texten, Doku und Commits. Kein Claude-Co-Author in Commits. Commits nur mit expliziten Pfaden.
- Kurzform `$TEST <Klasse>` steht für:
  `cd app && /opt/homebrew/bin/xcodegen generate && TEST_RUNNER_KADRELL_PROFILE=tmp xcodebuild -project Kadrell.xcodeproj -scheme Kadrell -configuration Debug -derivedDataPath build -skipPackagePluginValidation -skipMacroValidation test -only-testing:KadrellTests/<Klasse>`

## Review Focus

1. Eine Extension ruft `print("x")`: das darf das Protokoll nicht zerschießen, `print` geht ins Log (Test in Task 1).
2. Eine Extension schreibt kaputtes JSON oder Binärmüll auf stdout: Kadrell verwirft die Zeile mit Log-Eintrag und läuft weiter (Test in Task 3 und Task 5).
3. Kadrell endet, während eine Extension in einer Endlosschleife hängt: der Helper muss trotzdem sterben, kein Waisenprozess (Test in Task 5).
4. Der Ordner einer laufenden Extension wird gelöscht oder umbenannt: Prozess wird sauber gestoppt, Eintrag verschwindet aus dem Dialog, kein Absturz (Test in Task 6).
5. Eine Extension stirbt, während ihr Tab in der rechten Sidebar die Tastatur hat: Fokus geht an die Kachel zurück, kein Absturz (Test in Task 7).

---

### Task 1: Lua im Binary und `Kadrell ext-host` mit Prelude

**Files:**
- Create: `app/Vendor/lua/` (Inhalt von `lua-5.5.1/src`, ohne Änderungen), `app/Vendor/lua/LICENSE` (aus `doc/readme.html` den MIT-Text)
- Create: `app/Kadrell/Extensions/Host/LuaShim.h`, `LuaShim.c`, `Kadrell-Bridging-Header.h`
- Create: `app/Kadrell/Extensions/Host/prelude.lua`, `app/Kadrell/Extensions/Host/json.lua`
- Create: `app/Kadrell/Extensions/Host/ExtHost.swift`
- Modify: `app/project.yml` (Quellen, Bridging Header), `app/Kadrell/App/main.swift`
- Test: `app/KadrellTests/ExtHostTests.swift`, Hilfsdatei `app/KadrellTests/ExtFixture.swift`

**Interfaces:**
- Produces (C): `lua_State *kl_new(size_t memLimitBytes)`; `void kl_set_sender(void (*fn)(const char *json, size_t len))`; `int kl_run_file(lua_State *L, const char *path, char *err, size_t errlen)`; `int kl_dispatch(lua_State *L, const char *json, char *err, size_t errlen)`. Rückgabe 0 = ok, sonst Fehlertext in `err`.
- Produces (Swift): `enum ExtHost { static func run(dir: String) -> Never }`.
- Produces (Test-Hilfe): `struct ExtFixture { static func make(name: String, initLua: String, manifest: [String: Any]? = nil) throws -> URL }` legt einen Extension-Ordner in einem Temp-Verzeichnis an; `final class HostPipe { init(dir: URL) throws; func send(_ obj: [String: Any]); func next(timeout: TimeInterval) -> [String: Any]?; func closeStdin(); var process: Process }` startet `Bundle.main.executablePath` mit `ext-host <dir>`.
- Protokoll (Feld `t` als Diskriminator), siehe Spec. Lokal im Helper behandelte Nachrichten von Lua: `exec`, `http`, `timer` (Task 2).

- [ ] **Step 1:** Tarball laden, SHA256 mit `shasum -a 256` gegen den Wert aus Global Constraints prüfen, `src/` nach `app/Vendor/lua/` kopieren. json.lua am aktuellen master-Commit laden, Hash in die erste Zeile als Kommentar.
- [ ] **Step 2:** `project.yml` im Target `Kadrell`: Quelle `Vendor/lua` mit `excludes: [lua.c, luac.c]` und `compilerFlags: ["-w", "-DLUA_USE_POSIX"]`; `.lua`-Dateien unter `Kadrell/Extensions/Host` als `buildPhase: resources`; Setting `SWIFT_OBJC_BRIDGING_HEADER: Kadrell/Extensions/Host/Kadrell-Bridging-Header.h` (importiert nur `LuaShim.h`). `xcodegen generate` und Build müssen ohne neue Warnungen durchlaufen.
- [ ] **Step 3: Failing tests schreiben** in `ExtHostTests`:

```swift
func testHelloReadyPingPong() throws {
    let dir = try ExtFixture.make(name: "hello", initLua: "kadrell.log('geladen')")
    let p = try HostPipe(dir: dir)
    p.send(["t": "hello", "api": 1, "name": "hello", "dir": dir.path, "storageDir": NSTemporaryDirectory(), "config": [:], "locale": "de", "sessions": []])
    XCTAssertEqual(p.next(timeout: 3)?["t"] as? String, "log")      // kadrell.log aus init.lua
    XCTAssertEqual(p.next(timeout: 3)?["t"] as? String, "ready")
    p.send(["t": "ping", "id": 7])
    let pong = p.next(timeout: 3)
    XCTAssertEqual(pong?["t"] as? String, "pong"); XCTAssertEqual(pong?["id"] as? Int, 7)
}
func testPrintGoesToLog() throws          // init.lua: print("x", 1) → {"t":"log","level":"info","text":"x\t1"}, danach ready
func testLoadErrorIsReportedThenExit()     // init.lua: "error('kaputt')" → log level "error" mit "init.lua:1" und "kaputt", Prozess endet mit Status != 0 binnen 3 s
func testBlockedFunctionsAreGone()         // init.lua: kadrell.log(tostring(os.execute)..tostring(io.popen)..tostring(package.loadlib)) → "nilnilnil"
func testJsonRoundtrip()                   // init.lua: kadrell.log(kadrell.json.encode(kadrell.json.decode('{"a":[1,2],"b":"ü"}'))) → Text enthält "\"a\":[1,2]" und "ü"
func testStdinEofEndsHelper()              // nach ready: closeStdin() → process.isRunning == false binnen 1 s
```

- [ ] **Step 4:** Run `$TEST ExtHostTests`. Expected: FAIL (Binary kennt `ext-host` nicht).
- [ ] **Step 5: C-Schicht** in `LuaShim.c`: `kl_new` mit eigenem Allocator, der über `memLimitBytes` hinaus `NULL` liefert (Default-Aufruf mit 64 MB); `luaL_openlibs`, danach `os.execute`, `io.popen`, `package.loadlib` auf `nil`, `package.cpath = ""`, C-Searcher (Index 3 und 4 in `package.searchers`) entfernen. Globale Lua-Funktion `__kadrell_send(str)` ruft den Sender. `kl_run_file` und `kl_dispatch` laufen komplett unter `lua_pcall` mit `debug.traceback` als Message-Handler; `kl_dispatch` ruft die globale Lua-Funktion `__kadrell_dispatch(json)`. Kein `lua_error` außerhalb von C-Frames.
- [ ] **Step 6: Prelude** (`prelude.lua`): legt `kadrell` an mit `json` (aus json.lua), `log`/`warn` (sendet `log`), `print` ersetzt (Argumente mit Tab verbunden, `level = "info"`), `on(name, fn)`, `__kadrell_dispatch(json)`: `hello` speichert `config`, `name`, `dir`, `storageDir`, setzt `package.path` auf `<dir>/?.lua`, lädt `init.lua` und sendet danach `ready`; `ping` → `pong` mit gleicher `id`; `event` ruft die registrierten Handler; `shutdown` → `os.exit(0)`. Ladefehler von `init.lua` → `log` mit `level = "error"` und Traceback, dann `os.exit(1)`.
- [ ] **Step 7: `ExtHost.run(dir:)`**: serielle `DispatchQueue` `lua`; `kl_new`, `kl_set_sender` mit `@convention(c)`-Funktion, die Zeilen in Task 1 alle unverändert mit `\n` nach stdout schreibt (unter Lock, `fflush`); Prelude aus `Bundle.main` laden; eigener Thread liest stdin zeilenweise und reicht jede Zeile per `lua.async` an `kl_dispatch` weiter; bei EOF ruft dieser Lese-Thread selbst `exit(0)`, damit auch eine hängende Lua-Schleife den Prozess nicht festhält. `main.swift`: vor dem CLI-Zweig `if cliArgs.first == "ext-host", cliArgs.count == 2 { ExtHost.run(dir: cliArgs[1]) }`.
- [ ] **Step 8:** Run `$TEST ExtHostTests`. Expected: PASS (6 Tests).
- [ ] **Step 9: Commit**

```bash
git add app/Vendor/lua app/Kadrell/Extensions/Host app/Kadrell/App/main.swift app/project.yml app/KadrellTests/ExtHostTests.swift app/KadrellTests/ExtFixture.swift
git commit -m "Extensions: Lua 5.5.1 im Binary, ext-host mit Prelude, hello/ready/ping"
```

### Task 2: Helper-API: Coroutines, exec, http, Timer, run, Storage

**Files:**
- Modify: `app/Kadrell/Extensions/Host/prelude.lua`, `app/Kadrell/Extensions/Host/ExtHost.swift`
- Test: `app/KadrellTests/ExtHostTests.swift`

**Interfaces:**
- Consumes: `HostPipe`, `ExtFixture`, Protokoll aus Task 1.
- Produces (Lua, siehe Spec Abschnitt 2): `kadrell.exec(argv, opts)`, `kadrell.http(req)`, `kadrell.every(s, fn)` / `kadrell.after(s, fn)` mit `handle:cancel()`, `kadrell.run(...)`, `kadrell.sessions()`, `kadrell.storage.get/set`, `kadrell.config`, `kadrell.panel.set/clear`, `kadrell.status.set/clear`.
- Produces (Protokoll Lua → Helper, nie nach stdout): `{"t":"exec","id","argv","cwd","timeout"}`, `{"t":"http","id","method","url","headers","body","timeout"}`, `{"t":"timer","id","after"}`. Helper → Lua: `{"t":"result","id",...}` bzw. `{"t":"timer","id"}`.
- Produces (Protokoll Extension → Kadrell): `run` mit `id`, `argv`; `panel` mit `tree` oder `null`; `status` mit `item` oder `null`. Kadrell antwortet auf `run` mit `result` (`id`, `status`, `stdout`, `stderr`).

- [ ] **Step 1: Failing tests** in `ExtHostTests`:

```swift
func testExecInsideHandler()      // on("app.ready"): r = kadrell.exec({"/bin/echo","hi"}); kadrell.log(r.status..":"..r.stdout) → Log "0:hi\n"
func testExecMissingBinary()      // exec({"/nope"}) → status 127, Extension läuft weiter (ping → pong)
func testHandlerErrorIsLoggedNotFatal() // on("app.ready", error("boom")) → log level "error" mit "boom"; danach ping → pong
func testCallOutsideHandlerFails() // init.lua ruft kadrell.exec direkt → Ladefehler "nur in Handlern" (log error), Exit != 0
func testTimerFires()             // on app.ready: kadrell.after(0.1, function() kadrell.log("t") end) → Log "t" binnen 1 s
func testRunRoundtrip()           // on app.ready: r = kadrell.run("ls","--json") → Pipe sieht {"t":"run","argv":["ls","--json"]}; Test antwortet result status 0 stdout "{}" → Log "0"
func testStorageSurvivesRestart() // erster Prozess: storage.set("k", {a=1}); zweiter Prozess mit gleichem storageDir: storage.get("k").a == 1
func testPanelAndStatusAreForwarded() // panel.set{title="T"} → {"t":"panel","tree":{"title":"T"}}; status.clear() → {"t":"status","item":null}
```

`app.ready` schickt der Test als `{"t":"event","name":"app.ready","data":{}}`.

- [ ] **Step 2:** Run `$TEST ExtHostTests`. Expected: die 8 neuen Tests FAIL.
- [ ] **Step 3: Prelude:** jeder Handler (Event, Timer) läuft in einer neuen Coroutine mit `xpcall`/`debug.traceback`; Host-Aufrufe vergeben eine fortlaufende `id`, legen die laufende Coroutine in `pending[id]`, senden und `coroutine.yield()`; `result`/`timer` setzen fort. Aufruf außerhalb einer Coroutine wirft `"kadrell.<fn>: nur in Handlern (kadrell.on, every, after)"`. `every` plant sich nach jedem Lauf neu. `storage` liest und schreibt `<storageDir>/storage.json` per `io.open` und `kadrell.json`.
- [ ] **Step 4: Helper (`ExtHost.swift`):** der Sender unterscheidet nach `t`: `exec` per `ProcessRunner.run(argv[0], rest, environment: ProcessInfo.processInfo.environment, cwd:, timeout:, mergeStderr: false)`, stderr separat (bei Startfehler `status = 127`, Fehlertext in `stderr`); `http` per `URLSession.shared.data(for:)` mit `timeoutInterval`; `timer` per `lua.asyncAfter`; alles andere nach stdout. Ergebnisse gehen als JSON-Zeile per `lua.async` an `kl_dispatch`.
- [ ] **Step 5:** Run `$TEST ExtHostTests`. Expected: PASS (14 Tests).
- [ ] **Step 6: Commit** (`prelude.lua`, `ExtHost.swift`, `ExtHostTests.swift`), Message: `Extensions: exec, http, Timer, run, Storage im Helper`.

### Task 3: Manifest, Katalog, Protokoll, Panel-Baum (reine Logik)

**Files:**
- Create: `app/Kadrell/Extensions/ExtensionManifest.swift`, `ExtensionMessage.swift`, `PanelTree.swift`
- Test: `app/KadrellTests/ExtensionModelTests.swift`

**Interfaces:**
- Produces:
  - `struct ExtensionManifest: Codable, Equatable { name, version, description: String; apiVersion: Int; permissions: [String]; settings: [Setting] }` mit `struct Setting: Codable, Equatable { key, type, label: String; default: JSONValue? }` (`type`: `string|secret|bool`). `static let supportedAPI = 1`.
  - `enum JSONValue: Codable, Equatable` (string, number, bool, null, array, object) für freie Felder.
  - `struct FoundExtension: Equatable { name: String; dir: URL; manifest: ExtensionManifest?; problem: String? }` und `enum ExtensionCatalog { static let dir: URL; static func scan(_ dir: URL = dir) -> [FoundExtension] }`. `problem` ist ein lokalisierter Klartext: Manifest fehlt/kaputt, Name ≠ Ordner, `apiVersion` zu hoch („braucht neueres Kadrell“), `init.lua` fehlt, Besitz/Schreibrechte (über `Hooks.trusted` für Ordner, `kadrell.json`, `init.lua`). Sortiert nach Name, Ordner mit Punkt am Anfang übersprungen.
  - `enum ExtensionMessage { case ready, pong(Int), panel(JSONValue?), status(JSONValue?), run(id: Int, argv: [String]), log(level: String, text: String); static func decode(_ line: Data) -> ExtensionMessage? }` (unbekanntes `t` oder kaputtes JSON → `nil`).
  - `enum HostMessage: Encodable { case hello(api: Int, name: String, dir: String, storageDir: String, config: [String: JSONValue], locale: String, sessions: [JSONValue]), event(name: String, data: JSONValue), result(id: Int, status: Int32, stdout: String, stderr: String), ping(Int), shutdown }` mit `func line() -> Data` (JSON + `\n`).
  - `indirect enum PanelNode: Equatable { case section(title: String, collapsed: Bool, children: [PanelNode]), item(text: String, detail: String?, color: ThemeColor?, actions: [PanelAction]), text(String, ThemeColor?), button(label: String, action: String) }`, `struct PanelAction: Equatable { id, label: String }`, `enum ThemeColor: String { case accent, muted, ok, warn, err }`, `struct PanelTree: Equatable { title: String; nodes: [PanelNode] }`.
  - `enum PanelValidation { static func tree(_ v: JSONValue) -> Result<(PanelTree, warnings: [String]), PanelError>; static func status(_ v: JSONValue) -> (text: String, color: ThemeColor?, action: String?)? }` mit `enum PanelError: Error { case tooLarge(Int) }`. Panel-Wurzel: Objekt mit `title` und numerischen Kindern (Lua-Array-Teil, json.lua kodiert gemischte Tabellen nicht). **Festlegung:** In Lua heißt die Kinderliste der Wurzel `children`; die Doku-Beispiele und Task 2 verwenden `kadrell.panel.set{title=..., children={...}}`.

- [ ] **Step 1: Failing tests** in `ExtensionModelTests`:

```swift
func testScanFindsValidAndReportsProblems() // Fixtures: ok, nameMismatch, apiTooHigh (apiVersion 2), noInit, groupWritable (chmod 0775) → genau "ok" ohne problem, die vier anderen mit problem != nil
func testScanSkipsDotFolders()
func testDecodeIgnoresGarbage()   // decode(Data("nicht json".utf8)) == nil; decode(Data([0xff,0xfe])) == nil; {"t":"wat"} == nil
func testDecodeRunAndLog()        // {"t":"run","id":3,"argv":["ls"]} == .run(id: 3, argv: ["ls"])
func testHostMessageLineEndsWithNewline()
func testPanelValidation()        // unbekannter type "video" wird übersprungen + 1 warning; color "pink" → nil + warning; 600 Zeichen Text → 500
func testPanelTooLarge()          // 2001 items → .failure(.tooLarge(2001))
func testStatusTruncatedTo40()
```

- [ ] **Step 2:** Run `$TEST ExtensionModelTests`. Expected: FAIL (Typen fehlen).
- [ ] **Step 3:** Typen wie in Interfaces umsetzen. Nutzertexte in `problem` mit `String(localized:bundle:)`, Deutsch und Englisch in `Localizable.xcstrings`.
- [ ] **Step 4:** Run `$TEST ExtensionModelTests` und `$TEST LocalizationTests`. Expected: PASS.
- [ ] **Step 5: Commit**, Message: `Extensions: Manifest, Katalog, Protokoll, Panel-Baum`.

### Task 4: Zustandsmaschine (reine Logik)

**Files:**
- Create: `app/Kadrell/Extensions/ExtensionSupervisor.swift`
- Test: `app/KadrellTests/ExtensionSupervisorTests.swift`

**Interfaces:**
- Produces:
  - `enum ExtensionState: Equatable { case off, starting, running, failed(String), reloading }`
  - `enum SupervisorInput { case enable, disable, reload, spawned, ready, exited(expected: Bool, reason: String), pingTimeout, readyTimeout, flood(String), restartDue }`
  - `enum SupervisorAction: Equatable { case spawn, terminate, scheduleRestart(TimeInterval), clearUI }`
  - `struct ExtensionSupervisor { private(set) var state: ExtensionState; mutating func handle(_ input: SupervisorInput, now: Date) -> [SupervisorAction] }`. Unerwartetes Ende, `pingTimeout`, `readyTimeout`, `flood` zählen als Absturz; Backoff `[1, 5, 30]`; der 3. Absturz innerhalb von 120 s → `.failed` mit Grund „3 Abstürze in 2 min, bleibt aus“ und keine weitere Aktion; `reload` zählt nie als Absturz; `enable` aus `.failed` setzt den Zähler zurück. Jeder Absturz liefert `.clearUI`.

- [ ] **Step 1: Failing tests**:

```swift
func testEnableSpawnsAndReadyRuns()     // enable → [.spawn], state .starting; ready → state .running
func testCrashBackoffSequence()         // drei Abstürze im Abstand von 10 s: scheduleRestart(1), scheduleRestart(5), dann state .failed(...) ohne scheduleRestart
func testCrashesOutsideWindowReset()    // Abstürze bei t=0, t=200 s, t=400 s → jeweils scheduleRestart(1)
func testReloadDoesNotCount()           // 5x reload hintereinander → nie .failed
func testDisableTerminatesAndClears()   // running + disable → [.terminate, .clearUI], state .off; danach exited(expected: true) → keine Aktion
func testPingTimeoutCountsAsCrash()     // reason enthält "5 s"
```

- [ ] **Step 2:** Run `$TEST ExtensionSupervisorTests`. Expected: FAIL.
- [ ] **Step 3:** `handle` umsetzen, mit einer kleinen privaten Funktion pro Eingang, damit lizard unter CCN 15 bleibt. Gründe lokalisiert.
- [ ] **Step 4:** Run `$TEST ExtensionSupervisorTests`. Expected: PASS.
- [ ] **Step 5: Commit**, Message: `Extensions: Zustandsmaschine mit Backoff und Absturzgrenze`.

### Task 5: Prozess-Verwaltung (`ExtensionProcess`)

**Files:**
- Create: `app/Kadrell/Extensions/ExtensionProcess.swift`
- Test: `app/KadrellTests/ExtensionProcessTests.swift`

**Interfaces:**
- Consumes: `ExtensionMessage`, `HostMessage`, `ExtFixture`.
- Produces: `@MainActor final class ExtensionProcess { init(dir: URL, executable: String = Bundle.main.executablePath!, environment: [String: String]); var onMessage: (ExtensionMessage) -> Void; var onExit: (_ expected: Bool, _ reason: String) -> Void; var onStderr: (String) -> Void; func start(hello: HostMessage) throws; func send(_ m: HostMessage); func stop() }`. Intern: nicht blockierendes Schreiben mit Puffer bis 1 MB (voll = Hänger), Lesen auf eigenem Thread, `ready`-Timeout 3 s, `ping` alle 5 s mit `pong`-Frist 5 s, Flutgrenze 50 Nachrichten/s, Zeile > 1 MB = Kill, `stop()` = `shutdown` → 1 s → SIGTERM → 1 s → SIGKILL. Kaputte Zeilen: `onStderr("ungültige Nachricht verworfen")`, kein Kill. Gründe in `onExit` lokalisiert.

- [ ] **Step 1: Failing tests** (Fixtures als Lua-Text über `ExtFixture`):

```swift
func testHelloDeliversReadyAndPanel()   // init.lua: on("app.ready", panel.set{title="A", children={}}) → onMessage .ready, nach event app.ready .panel(...)
func testCrashReportsUnexpectedExit()   // init.lua: error("x") → onExit(expected: false, reason enthält "x")
func testInfiniteLoopIsKilledByPing()   // on("app.ready", while true do end) → onExit(false, reason enthält "5 s") binnen 12 s, Prozess weg
func testFloodIsKilled()                // on("app.ready", for i=1,1000 do kadrell.log(i) end) → onExit(false, reason Flut)
func testGarbageLineIsIgnored()         // init.lua: io.stdout:write("kaputt\n"); io.stdout:flush() → onStderr-Meldung, danach ready, Prozess läuft
func testStopIsGracefulAndThenHard()    // stop() bei hängender Schleife → Prozess weg binnen 3 s, onExit(expected: true, ...)
func testParentGoneKillsLoopingHelper() // Review Focus 3: hängende Schleife, dann nur stdin schließen → Prozess weg binnen 1 s
```

- [ ] **Step 2:** Run `$TEST ExtensionProcessTests`. Expected: FAIL.
- [ ] **Step 3:** `ExtensionProcess` umsetzen. `Process` mit `standardInput`/`standardOutput`/`standardError` als `Pipe`, Umgebung aus dem Konstruktor.
- [ ] **Step 4:** Run `$TEST ExtensionProcessTests`. Expected: PASS.
- [ ] **Step 5: Commit**, Message: `Extensions: Prozess mit Ping, Flutgrenze und sanftem Stopp`.

### Task 6: `ExtensionManager`: an/aus, Events, Befehle, Einstellungen, Live-Reload

**Files:**
- Create: `app/Kadrell/Extensions/ExtensionManager.swift`, `app/Kadrell/Extensions/ExtensionSettings.swift`
- Modify: `app/Kadrell/App/AppDelegate.swift` (Start, Events), `app/Kadrell/App/AppDelegate+Control.swift` (`runControl` von `private` auf `internal`), `app/Kadrell/App/Settings.swift`
- Test: `app/KadrellTests/ExtensionManagerTests.swift`

**Interfaces:**
- Consumes: `ExtensionCatalog`, `ExtensionSupervisor`, `ExtensionProcess`, `PanelValidation`, `Keychain`, `ControlCommand.parse`, `AppDelegate.runControl(_:_:)`.
- Produces:
  - `Settings.enabledExtensions: [String]` (Key `extensions.enabled`), `ExtensionSettings.values(for: FoundExtension) async -> [String: JSONValue]` (Defaults aus Manifest, normale Werte unter `extensions.<name>.<key>`, `secret` per `Keychain.read(service: "de.malura.kadrell.extension", account: "<Profile.name ?? "default">/<name>/<key>")`), `ExtensionSettings.set(_:for:key:) async`.
  - `@MainActor final class ExtensionManager { init(environment: [String: String], control: @escaping ([String]) async -> ControlResponse, sessions: @escaping () -> [JSONValue]); private(set) var found: [FoundExtension]; func state(_ name: String) -> ExtensionState; func log(_ name: String) -> [String]; var panels: [(name: String, tree: PanelTree)]; var statusItems: [(name: String, text: String, color: ThemeColor?, action: String?)]; var onChange: () -> Void; func start(); func setEnabled(_ name: String, _ on: Bool); func reload(_ name: String); func emit(_ event: String, _ data: JSONValue); func action(_ name: String, id: String); func shutdownAll() }`.
  - Session als Event-Daten: `{key, title, cwd, branch, sessionId, state, group}`, `state` = `SessionStatus.rawValue`.
- Wiring in `AppDelegate`: Manager nach `boot()` starten mit `cli.environment`; `control` ruft `runControl(try ControlCommand.parse(argv), ControlRequest(argv: argv, cwd: NSHomeDirectory(), caller: nil))`, Fehler als `.fail`; `registry.onChange` vergleicht alte und neue Sessions und sendet `session.new`, `session.remove`, `session.status` (nur bei geändertem `status`); `attention.fireFocusHook` sendet zusätzlich `session.focus`; `app.ready` nach dem ersten `pollNow`; `applicationWillTerminate` ruft `shutdownAll()`.
- Live-Reload: `DispatchSource.makeFileSystemObjectSource` auf `ExtensionCatalog.dir` und je Extension-Ordner (`.write`, `.rename`, `.delete`), 300 ms entprellt, danach `scan()` neu; geänderte, eingeschaltete Extension → `reload`; verschwundene → stoppen und aus `found` entfernen; neue erscheinen aus.

- [ ] **Step 1: Failing tests** (Manager mit Fake-`control` und Temp-Katalog; `ExtensionCatalog.dir` per Init-Parameter `catalogDir: URL` überschreibbar machen):

```swift
func testEnableStartsAndPanelArrives()   // Fixture panel.set on app.ready; setEnabled(true); emit("app.ready") → panels.first?.tree.title == "A" binnen 3 s
func testDisableClearsPanelImmediately() // danach setEnabled(false) → panels leer sofort, Settings.enabledExtensions ohne Namen
func testRunGoesThroughControl()         // Fixture ruft kadrell.run("ls") → Fake-control bekommt ["ls"], Lua loggt status
func testFileChangeReloads()             // init.lua neu schreiben → log enthält Ausgabe der neuen Version binnen 2 s, state .running
func testDeletedFolderStopsCleanly()     // Review Focus 4: Ordner löschen → found ohne Eintrag, kein Prozess mehr, kein Absturz
func testSecretSettingIsPassedInConfig() // Manifest mit secret "token"; set("abc") → Fixture loggt kadrell.config.token == "abc"; Keychain-Eintrag am Ende löschen
```

- [ ] **Step 2:** Run `$TEST ExtensionManagerTests`. Expected: FAIL.
- [ ] **Step 3:** Manager, Settings und Wiring umsetzen. Log pro Extension als Ringpuffer mit 200 Zeilen (stderr, `log`-Nachrichten, Zustandswechsel).
- [ ] **Step 4:** Run `$TEST ExtensionManagerTests`, danach den gesamten Testlauf (`-only-testing` weglassen). Expected: PASS, keine bestehenden Tests rot.
- [ ] **Step 5: Commit**, Message: `Extensions: Manager mit Events, Steuerbefehlen, Einstellungen und Live-Reload`.

### Task 7: Rechte Sidebar

**Files:**
- Create: `app/Kadrell/Extensions/UI/ExtensionPanelView.swift`
- Modify: `app/Kadrell/App/MainWindow.swift`, `app/Kadrell/App/Hotkeys.swift`, `app/Kadrell/App/AppDelegate.swift` (`perform`, Manager-`onChange` → alle Fenster), `app/Kadrell/UI/AboutView.swift` (Kürzel ⌘3, ⌘⌥B)
- Test: `app/KadrellTests/ExtensionPanelTests.swift`

**Interfaces:**
- Consumes: `PanelTree`, `PanelNode`, `ThemeColor`, `ExtensionManager.panels`, `ExtensionManager.action(_:id:)`.
- Produces:
  - `final class ExtensionPanelView: NSView` (custom gezeichnet wie `SidebarView`, Theme-Farben, UI-Größe): `var panels: [(name: String, tree: PanelTree)]`, `var selectedTab: String?`, `var onAction: (_ name: String, _ id: String) -> Void`, `var onLeave: () -> Void`; Tastatur: ↑↓ Eintrag, ←→ Tab, ⏎ erste Aktion, ⌥⏎ Menü aller Aktionen, Esc → `onLeave`; Klick = erste Aktion, Rechtsklick = Menü.
  - `MainWindowController`: dritter Bereich `rightScroll` im `split`; `var isRightSidebarHidden: Bool`; `func toggleRightSidebar()`; `func updateExtensionPanels(_ panels:)` blendet den Bereich ohne Panels ganz aus; Breite und Sichtbarkeit unter `rightSidebar.width<suffix>` / `rightSidebar.visible<suffix>` in `Profile.defaults`; Tab-Auswahl pro Fenster.
  - `HotkeyAction.focusRightSidebar` (Default `Hotkey(.command, "3")`), `HotkeyAction.toggleRightSidebar` (Default `Hotkey([.command, .option], "b")`), Titel lokalisiert.
- Fällt der Tab der fokussierten Sidebar weg, bekommt `workspace` die Tastatur.

- [ ] **Step 1:** Prüfen, dass ⌘3 und ⌘⌥B weder in `Hotkeys` noch als Menü-`keyEquivalent` belegt sind: `grep -n '"3"\|"b"' app/Kadrell/App/*.swift` und Modifier der Treffer lesen. Belegt → in diesem Task einen freien Default wählen und im Plan vermerken.
- [ ] **Step 2: Failing tests**:

```swift
func testHotkeyDefaultsStayUnique()       // bestehender HotkeyTests.testDefaultsAreUniqueAndUsable bleibt grün mit den 2 neuen Aktionen
func testPanelKeyboardNavigation()        // 2 Tabs, 3 Items: ↓↓⏎ → onAction(name: "a", id: <erste Aktion von Item 2>); → wechselt selectedTab auf "b"
func testAreaHiddenWithoutPanels()        // updateExtensionPanels([]) → rightScroll nicht im split
func testFocusFallsBackWhenTabVanishes()  // Review Focus 5: Panel-View ist firstResponder, Panels → [] → firstResponder ist workspace
func testRenderSmoke()                    // View mit allen 4 Knotentypen und allen 5 Farben zeichnet ohne Absturz in ein Bitmap (Muster wie StatsViewRenderTests)
```

- [ ] **Step 3:** Run `$TEST ExtensionPanelTests` und `$TEST HotkeyTests`. Expected: FAIL.
- [ ] **Step 4:** Umsetzen. Neue Nutzertexte zweisprachig.
- [ ] **Step 5:** Run `$TEST ExtensionPanelTests`, `$TEST HotkeyTests`, `$TEST MultiWindowTests`, `$TEST LocalizationTests`. Expected: PASS.
- [ ] **Step 6: Sichtprüfung** im Temp-Profil mit einer Test-Extension (Panel mit allen Knotentypen): Screenshot nach `~/.claude/screenshots/kadrell/ext-panel.png`, nie löschen.
- [ ] **Step 7: Commit**, Message: `Extensions: rechte Sidebar mit Tabs und Tastatur`.

### Task 8: Statusleiste

**Files:**
- Modify: `app/Kadrell/Bar/StatusBarView.swift`, `app/Kadrell/App/AppDelegate.swift`
- Test: `app/KadrellTests/ExtensionPanelTests.swift`

**Interfaces:**
- Consumes: `ExtensionManager.statusItems`, `ExtensionManager.action(_:id:)`.
- Produces: `StatusBarView.extensionItems: [(name: String, text: String, color: ThemeColor?, action: String?)]`, `StatusBarView.onExtensionAction: (_ name: String, _ id: String) -> Void`. Gezeichnet über das vorhandene `module(...)` rechts vor `drawCounts`; Klick auf ein Element mit `action` ruft `onExtensionAction`.

- [ ] **Step 1: Failing test** `testStatusItemClickSendsAction`: Bar mit einem Eintrag `("ddev", "ddev 3 up", .ok, "ddev:list")`, Klick in dessen Rechteck → `onExtensionAction("ddev", "ddev:list")`.
- [ ] **Step 2:** Run `$TEST ExtensionPanelTests`. Expected: FAIL.
- [ ] **Step 3:** Umsetzen, Rechtecke der Einträge beim Zeichnen merken (wie `badge(_:_:)`).
- [ ] **Step 4:** Run `$TEST ExtensionPanelTests` und `$TEST StatusMenuTests`. Expected: PASS.
- [ ] **Step 5: Commit**, Message: `Extensions: Einträge in der Statusleiste`.

### Task 9: Extensions-Dialog

**Files:**
- Create: `app/Kadrell/Extensions/UI/ExtensionsView.swift`
- Modify: `app/Kadrell/App/SheetPresenter.swift` (`Panel.extensions`), `app/Kadrell/App/AppDelegate.swift` (F4 in `handleKey` neben F1/F3, Menü „Extensions …“ im App-Menü, Palette-Kommando „Extensions“ in der Liste bei `commands`), `app/Kadrell/UI/AboutView.swift` (F4 und Link auf `https://kadrell.malura.de/extensions`)
- Test: `app/KadrellTests/ExtensionsViewTests.swift`

**Interfaces:**
- Consumes: `ExtensionManager` (`found`, `state`, `log`, `setEnabled`, `reload`), `ExtensionSettings`.
- Produces: `struct ExtensionsView: View` mit `@ObservedObject var model: ExtensionsModel`; `@MainActor final class ExtensionsModel: ObservableObject` als dünne Hülle um den Manager (aktualisiert sich über `manager.onChange`). Inhalt laut Spec Abschnitt 4: Liste mit Name, Version, Beschreibung, Zustandspunkt, `permissions`; an/aus, „Neu laden“, „Ordner öffnen“ (`NSWorkspace.shared.open`), Log, Einstellungsformular (`string` → TextField, `secret` → SecureField, `bool` → Toggle), Fehlergrund im Klartext; oben „Extensions-Ordner öffnen“ (legt ihn bei Bedarf an) und Link zur Doku `https://kadrell.malura.de/extensions`. Tastatur: ↑↓, Leertaste, R, L, Esc.

- [ ] **Step 1: Failing tests**:

```swift
func testModelListsFoundAndToggles()  // Temp-Katalog mit 2 Extensions, eine mit problem → beide gelistet, Toggle bei der kaputten deaktiviert
func testSpaceTogglesSelected()       // Model: select(0); handleKey(" ") → manager.state(name) == .starting
func testDialogRenderSmoke()          // ExtensionsView in NSHostingView, deutsch und englisch, kein Absturz
```

- [ ] **Step 2:** Run `$TEST ExtensionsViewTests`. Expected: FAIL.
- [ ] **Step 3:** Umsetzen. Texte über `Text("…")` (Sprache via `\.locale`), AppKit-Texte mit `Bundle.app`, alles zweisprachig.
- [ ] **Step 4:** Run `$TEST ExtensionsViewTests` und `$TEST LocalizationTests`. Expected: PASS.
- [ ] **Step 5: Sichtprüfung** im Temp-Profil: Dialog mit einer laufenden, einer kaputten und einer abgeschalteten Extension, Screenshot nach `~/.claude/screenshots/kadrell/ext-dialog.png`.
- [ ] **Step 6: Commit**, Message: `Extensions: Dialog zum An- und Abschalten (F4)`.

### Task 10: Doku-Seite und README

**Files:**
- Create: `~/development/projects/kadrell.malura.de/extensions.html`
- Modify: `~/development/projects/kadrell.malura.de/index.html` (Link), `sitemap.xml`
- Modify: `app/README.md` (Abschnitt Extensions, kurz, verweist auf die Doku-Seite)

**Interfaces:**
- Consumes: die API, wie sie nach Task 1 bis 9 tatsächlich gebaut ist (Funktionsnamen aus `prelude.lua`, Knoten aus `PanelTree.swift`, Limits aus Global Constraints).

- [ ] **Step 1:** `extensions.html` auf Englisch, gleiche Struktur und `styles.css` wie `datenschutz.html`. Abschnitte laut Spec Abschnitt 5: Quick start, Manifest reference, Lifecycle and limits, API reference, Publishing (Topic `kadrell-extension`). Das Quick-start-Beispiel wird vorher als echte Extension im Temp-Profil ausgeführt und muss Panel und Statuseintrag zeigen.
- [ ] **Step 2:** Prüfen: `grep -n "$(printf '\342\200\224')" extensions.html` leer (keine Em-Dashes); Text mit dem humanizer-Skill gegen AI-Slop lesen; lokal mit `python3 -m http.server 8000` ansehen, Screenshot nach `~/.claude/screenshots/kadrell.malura.de/extensions.png`.
- [ ] **Step 3:** Link in `index.html` und Eintrag in `sitemap.xml`.
- [ ] **Step 4: Commit** im Website-Repo (`extensions.html`, `index.html`, `sitemap.xml`), Message: `Doku-Seite für Extensions`. Push und Deploy (`git push home_vault master`) erst nach Task 11, wenn die App-Version mit Extensions veröffentlicht ist. README-Änderung im App-Repo mit Task 11 committen.

### Task 11: Changelog, Version, Abschlussprüfung

**Files:**
- Modify: `CHANGELOG.md`, `app/project.yml`, `app/Kadrell/Info.plist` (über das Skript), `app/README.md`

- [ ] **Step 1:** Unter `## Unreleased` oben in `CHANGELOG.md`:
  - `- Feature: Extensions in Lua erweitern Kadrell um eigene Funktionen, an- und abschaltbar im neuen Extensions-Dialog (F4).`
  - `- Feature: Rechte Seitenleiste für Inhalte aus Extensions, Fokus mit ⌘3, ein- und ausblenden mit ⌘⌥B.`
  - `- Feature: Extensions können eigene Einträge in der Statusleiste zeigen.`
- [ ] **Step 2:** `python3 tools/bump-version.py minor`.
- [ ] **Step 3:** Clean Build ohne eigene Warnungen (`xcodebuild ... clean build`, Ausgabe nach `warning:` filtern, nur der bekannte `appintentsmetadataprocessor`-Hinweis erlaubt); `cd app && uv tool run lizard Kadrell -w` leer; kompletter Testlauf grün.
- [ ] **Step 4:** Manuell im Temp-Profil: Hello-Extension aus der Doku an, Datei ändern (Panel ändert sich ohne Neustart), Endlosschleife einbauen (Dialog zeigt Fehler, Kadrell bleibt bedienbar), wieder reparieren und anschalten.
- [ ] **Step 5: Commit und Push** (`CHANGELOG.md`, `app/project.yml`, `app/Kadrell/Info.plist`, `app/README.md`), Message: `Version <neu>: Lua-Extensions, rechte Seitenleiste, Statusleiste`. Danach deutlich sagen: gepusht, aber noch nicht als Release veröffentlicht und die Doku-Seite noch nicht deployed.
