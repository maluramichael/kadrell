import SwiftUI

enum Settings {
    static let startFolderKey = "startFolder"
    /// Startordner für ⌘N, Default Home. Immer mit abschließendem Slash.
    static var startFolder: String {
        get {
            var p = UserDefaults.standard.string(forKey: startFolderKey) ?? NSHomeDirectory()
            if p.hasPrefix("~") { p = NSHomeDirectory() + p.dropFirst() }
            return p.hasSuffix("/") ? p : p + "/"
        }
        set { UserDefaults.standard.set(newValue, forKey: startFolderKey) }
    }
    static var version: String {
        (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "0"
    }
}

@Observable
@MainActor
final class SettingsModel {
    var startFolder = Settings.startFolder
    var compSelected = 0
    var hotkeys = Hotkeys.current
    /// Aktion, deren Kürzel gerade aufgenommen wird.
    var recording: HotkeyAction?
    var onDone: (() -> Void)?
    @ObservationIgnored private var monitor: Any?

    func save() {
        stopRecording()
        let p = startFolder.trimmingCharacters(in: .whitespacesAndNewlines)
        if !p.isEmpty { Settings.startFolder = p }
        Hotkeys.current = hotkeys
        onDone?()
    }

    /// Nächster Tastendruck wird das Kürzel. Esc bricht ab, ⌫ entfernt es. Läuft vor OverlayPanel und Terminal.
    func record(_ a: HotkeyAction) {
        stopRecording()
        recording = a
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, let a = self.recording else { return event }
            let plain = event.modifierFlags.intersection(Hotkey.modMask).isEmpty
            if plain, event.keyCode == 53 { self.stopRecording(); return nil }
            if plain, event.keyCode == 51 { self.hotkeys[a] = nil; self.stopRecording(); return nil }
            guard let k = Hotkey(event: event), k.isUsable else { NSSound.beep(); return nil }
            for (other, v) in self.hotkeys where v == k { self.hotkeys[other] = nil }   // doppelt vergeben geht nicht
            self.hotkeys[a] = k
            self.stopRecording()
            return nil
        }
    }

    func stopRecording() {
        if let m = monitor { NSEvent.removeMonitor(m) }
        monitor = nil
        recording = nil
    }
}

/// ⌘, Einstellungen. ⌘⏎ speichert, Esc bricht ab.
struct SettingsView: View {
    @Bindable var model: SettingsModel

    private func label(_ s: String) -> some View {
        Text(s).font(.custom("JetBrainsMonoNF-Regular", size: 12)).foregroundStyle(Theme.fgColor)
            .padding(.horizontal, 16).padding(.top, 14)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("EINSTELLUNGEN").kerning(0.6)
                Spacer()
                Text("Kadrell v\(Settings.version)")
            }
            .font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
            .padding(.horizontal, 16).padding(.top, 12)
            label("Startordner für ⌘N")
            FolderInput(path: $model.startFolder, selected: $model.compSelected) { }
            label("Tastenkürzel")
            ScrollView {
                VStack(spacing: 2) {
                    ForEach(HotkeyAction.allCases, id: \.self) { a in row(a) }
                }
                .padding(.horizontal, 16).padding(.vertical, 8)
            }
            .frame(height: 380)
            DialogFoot(hint: "Kürzel anklicken, Tasten drücken · ⌫ entfernt · Esc abbrechen", button: "Speichern") { model.save() }
        }
        .frame(width: 640, alignment: .leading)
        .background(Theme.panelColor)
        .onDisappear { model.stopRecording() }
    }

    private func row(_ a: HotkeyAction) -> some View {
        let key = model.hotkeys[a]
        let isRecording = model.recording == a
        return HStack(spacing: 8) {
            Text(a.title).foregroundStyle(Theme.fgColor)
            Spacer()
            if key != a.defaultKey, !isRecording {
                Button { model.hotkeys[a] = a.defaultKey } label: { Text("Standard").foregroundStyle(Theme.mutedColor) }
                    .buttonStyle(.plain).help("Zurück auf \(a.defaultKey.display)")
            }
            Button { isRecording ? model.stopRecording() : model.record(a) } label: {
                Text(isRecording ? "Tasten drücken …" : key?.display ?? "–")
                    .font(.custom("JetBrainsMonoNF-Bold", size: 12))
                    .foregroundStyle(isRecording ? Theme.bgColor : Theme.fgColor)
                    .frame(width: 150).padding(.vertical, 3)
                    .background(isRecording ? Theme.runningColor : Theme.bgColor)
            }
            .buttonStyle(.plain)
        }
        .font(.custom("JetBrainsMonoNF-Regular", size: 12))
    }
}
