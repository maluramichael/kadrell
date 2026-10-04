import AppKit

// `kadrell <befehl>` (Symlink oder $KADRELL): Kommandozeile statt App. Xcode und LaunchServices übergeben nur Optionen mit „-“.
let cliArgs = Array(CommandLine.arguments.dropFirst())
// `Kadrell ext-host <ordner>`: Helper-Prozess einer Lua-Extension, von Kadrell selbst gestartet.
if cliArgs.first == "ext-host", cliArgs.count == 2 {
    ExtHost.run(dir: cliArgs[1])
}
if let first = cliArgs.first, !first.hasPrefix("-") || first == "-h" || first == "--help" {
    exit(ControlClient.run(cliArgs))
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
