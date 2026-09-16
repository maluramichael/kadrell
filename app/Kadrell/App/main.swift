import AppKit

// `kadrell <befehl>` (Symlink oder $KADRELL): Kommandozeile statt App. Xcode und LaunchServices übergeben nur Optionen mit „-“.
let cliArgs = Array(CommandLine.arguments.dropFirst())
if let first = cliArgs.first, !first.hasPrefix("-") || first == "-h" || first == "--help" {
    exit(ControlClient.run(cliArgs))
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
