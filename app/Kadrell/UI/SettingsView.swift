import SwiftUI

@Observable
@MainActor
final class SettingsModel {
    /// Wird bei jeder Änderung hochgezählt. Die Bindings lesen ihn mit, damit SwiftUI nach dem Schreiben neu zeichnet.
    private var revision = 0
    /// Textfelder halten ungetrimmte Entwürfe, sonst schluckt das Trimmen beim Tippen jedes Leerzeichen.
    var startFolder = Settings.startFolder {
        didSet {
            let p = startFolder.trimmingCharacters(in: .whitespacesAndNewlines)
            if !p.isEmpty { Settings.startFolder = p }
            changed()
        }
    }
    var editorCommand = Settings.editorCommand {
        didSet { Settings.editorCommand = editorCommand.trimmingCharacters(in: .whitespacesAndNewlines); changed() }
    }
    var hotkeys: [HotkeyAction: Hotkey] {
        get { _ = revision; return Hotkeys.current }
        set { Hotkeys.current = newValue; changed() }
    }
    @ObservationIgnored lazy var fonts: [(name: String, display: String)] = {
        let list = Settings.monospaceFonts, current = Settings.terminalFontName
        return list.contains { $0.name == current } ? list : [(current, current)] + list
    }()
    /// Aktion, deren Kürzel gerade aufgenommen wird.
    var recording: HotkeyAction?
    /// Nach jeder Änderung: die App zieht Menü, Aussehen und Sessions nach.
    @ObservationIgnored var onApply: (() -> Void)?
    @ObservationIgnored var onClose: (() -> Void)?
    @ObservationIgnored private var monitor: Any?

    /// Sprache der Dialoge. Liest `revision` mit: nach der Umstellung zeichnet SwiftUI sofort in der neuen Sprache.
    var locale: Locale { _ = revision; return Localization.locale }

    /// Jede Änderung gilt sofort, ohne Speichern: das Binding schreibt direkt in `Settings`.
    func binding<T>(_ key: ReferenceWritableKeyPath<Settings.Type, T>) -> Binding<T> { binding(Settings.self, key) }

    func binding<Root, T>(_ root: Root, _ key: ReferenceWritableKeyPath<Root, T>) -> Binding<T> {
        Binding(get: { _ = self.revision; return root[keyPath: key] },
                set: { root[keyPath: key] = $0; self.changed() })
    }

    private func changed() {
        revision += 1
        Task { @MainActor [weak self] in self?.onApply?() }
    }

    /// Nächster Tastendruck wird das Kürzel. Esc bricht ab, ⌫ entfernt es. Läuft vor OverlayPanel und Terminal.
    func record(_ a: HotkeyAction) {
        stopRecording()
        recording = a
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, let a = self.recording else { return event }
            let plain = event.modifierFlags.intersection(Hotkey.modMask).isEmpty
            if plain, event.keyCode == KeyCode.escape { self.stopRecording(); return nil }
            if plain, event.keyCode == KeyCode.delete { self.hotkeys[a] = nil; self.stopRecording(); return nil }
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

/// ⌘, Einstellungen. ⏎ speichert, Esc bricht ab.
struct SettingsView: View {
    @Bindable var model: SettingsModel

    /// Bereichsüberschrift wie in der F1-Hilfe: Akzentfarbe, gesperrt, Großbuchstaben, optionaler Hinweis daneben.
    private func heading(_ s: String, note: String = "", first: Bool = false) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(s.uppercased()).kerning(0.6).font(Theme.ui(11, bold: true)).foregroundStyle(Theme.runningColor)
            if !note.isEmpty { Text(note).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor) }
        }
        .padding(.horizontal, 16).padding(.top, first ? 14 : 18).padding(.bottom, 6)
    }

    /// Kürzel nach denselben Bereichen wie die F1-Hilfe; die zweite Gruppe nimmt den Rest, damit keine Aktion verschwindet.
    private var hotkeyGroups: [(String, [HotkeyAction])] {
        let nav: [HotkeyAction] = [.focusLeft, .focusRight, .focusUp, .focusDown, .nextSession, .prevSession, .lastSession, .previewNext, .previewPrev, .nextWaiting]
            + HotkeyAction.allCases.filter { $0.tileIndex != nil } + [.focusSidebar, .focusWorkspace]
        return [(String(localized: "Navigation", bundle: Bundle.app), nav), (String(localized: "Kacheln verwalten", bundle: Bundle.app), HotkeyAction.allCases.filter { !nav.contains($0) })]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("EINSTELLUNGEN").kerning(0.6)
                Spacer()
                Text("Kadrell v\(Settings.version)")
            }
            .font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
            .padding(.horizontal, 16).padding(.top, 12)
            Divider().overlay(Theme.lineColor).padding(.top, 12)

            heading(String(localized: "Sessions", bundle: Bundle.app), first: true)
            table {
                setting(String(localized: "Projektordner für ⌘N (nach Git-Repos durchsucht)", bundle: Bundle.app)) {
                    HStack(spacing: 4) {
                        pathField(Binding(get: { Theme.shortPath(model.startFolder) }, set: { model.startFolder = $0 }),
                                  onTab: { if let p = FolderIndex.expandAbbreviated(model.startFolder).first { model.startFolder = p } }) { urls in
                            guard let d = urls.lazy.compactMap({ FolderIndex.folder(for: $0.path) }).first else { return false }
                            model.startFolder = d
                            return true
                        }
                        iconButton("folder") { chooseFolder(start: model.startFolder) { if let p = $0 { model.startFolder = p } } }.help("Im Finder wählen")
                    }
                }
                setting(String(localized: "Externer Editor (Kommando wie im Terminal)", bundle: Bundle.app)) {
                    TextField("z. B. code", text: $model.editorCommand)
                        .textFieldStyle(.plain).foregroundStyle(Theme.fgColor)
                        .padding(.horizontal, 8).padding(.vertical, 3).background(Theme.bgColor)
                }
                setting(String(localized: "Fertige Sessions automatisch trennen (0 = nie)", bundle: Bundle.app)) {
                    slider(Binding(get: { Double(model.binding(\.autoDetachMinutes).wrappedValue) }, set: { model.binding(\.autoDetachMinutes).wrappedValue = Int($0) }),
                           Settings.autoDetachRange, step: 5, unit: String(localized: "min", bundle: Bundle.app))
                }
                setting(String(localized: "Wenn Claude endet (zweimal ⌃C, /exit)", bundle: Bundle.app)) {
                    menu(model.binding(\.closeTileOnExit), [(false, String(localized: "Kachel bleibt, Klick setzt fort", bundle: Bundle.app)), (true, String(localized: "Kachel schließen", bundle: Bundle.app))])
                }
            }

            heading(String(localized: "Claude", bundle: Bundle.app), note: String(localized: "gilt für neu gestartete Claude-Prozesse", bundle: Bundle.app))
            table {
                setting(String(localized: "Bypass-Modus erlauben (--allow-dangerously-skip-permissions)", bundle: Bundle.app)) { onOff(model.binding(\.claudeAllowBypass)) }
                setting(String(localized: "Startmodus", bundle: Bundle.app)) { options(Settings.claudeModes, model.binding(\.claudeMode)) }
                setting(String(localized: "Modell", bundle: Bundle.app)) { options(Settings.claudeModels, model.binding(\.claudeModel)) }
                setting(String(localized: "Effort", bundle: Bundle.app)) { options(Settings.claudeEfforts, model.binding(\.claudeEffort)) }
                setting(String(localized: "Sessions dürfen andere Sessions steuern (kadrell send, capture, kill)", bundle: Bundle.app)) { onOff(model.binding(\.controlOtherSessions)) }
            }

            heading(String(localized: "Darstellung", bundle: Bundle.app))
            table {
                setting(String(localized: "Sprache", bundle: Bundle.app)) {
                    menu(model.binding(\.language), Localization.Language.allCases.map { ($0, "\($0.flag)  \($0.title)") })
                }
                setting(String(localized: "Farbschema", bundle: Bundle.app)) { menu(model.binding(\.colorTheme), ColorTheme.all.map { ($0.id, $0.name) }) }
                setting(String(localized: "UI-Größe", bundle: Bundle.app)) { slider(model.binding(\.uiScale), Settings.uiScalePercent, step: 5, factor: 100, unit: "%", live: false) }
                setting(String(localized: "Terminal-Schrift", bundle: Bundle.app)) { menu(model.binding(\.terminalFontName), model.fonts.map { ($0.name, $0.display) }) }
                setting(String(localized: "Terminal-Schriftgröße  ⌘+ ⌘- ⌘0  ⌘ Mausrad", bundle: Bundle.app)) { slider(model.binding(\.terminalFontSize), Settings.fontSizes, step: 1, unit: "pt") }
                setting(String(localized: "Zeilenabstand", bundle: Bundle.app)) { slider(model.binding(\.terminalLineSpacing), Settings.lineSpacingPercent, step: 5, factor: 100, unit: "%") }
                setting(String(localized: "Innenabstand der Kacheln", bundle: Bundle.app)) { slider(model.binding(\.terminalPadding), Settings.pixelRange, step: 1, unit: "px") }
                setting(String(localized: "Verlauf zum Zurückscrollen", bundle: Bundle.app)) { slider(Binding(get: { Double(model.binding(\.terminalScrollback).wrappedValue) }, set: { model.binding(\.terminalScrollback).wrappedValue = Int($0) }), Settings.scrollbackRange, step: 1_000, unit: String(localized: "Zeilen", bundle: Bundle.app)) }
                setting(String(localized: "Abstand zwischen Kacheln", bundle: Bundle.app)) { slider(model.binding(\.tileGap), Settings.pixelRange, step: 1, unit: "px") }
                setting(String(localized: "Terminal auf der GPU zeichnen (Metal)", bundle: Bundle.app)) { onOff(model.binding(\.terminalMetal)) }
            }

            heading(String(localized: "Anpassen", bundle: Bundle.app), note: String(localized: "Bild nur hinter den Kacheln", bundle: Bundle.app))
            table {
                setting(String(localized: "Hintergrundbild", bundle: Bundle.app)) {
                    let image = model.binding(\.backgroundImage)
                    HStack(spacing: 4) {
                        pathField(Binding(get: { Theme.shortPath(image.wrappedValue) }, set: { image.wrappedValue = FolderIndex.normalize($0) }),
                                  placeholder: String(localized: "kein Bild · Datei hineinziehen", bundle: Bundle.app)) { urls in
                            guard let u = urls.first(where: { NSImage(contentsOf: $0) != nil }) else { return false }
                            image.wrappedValue = u.path
                            return true
                        }
                        iconButton("photo") {
                            chooseFolder(start: image.wrappedValue.isEmpty ? "~/Pictures" : (image.wrappedValue as NSString).deletingLastPathComponent, images: true) { if let p = $0 { image.wrappedValue = p } }
                        }.help("Bild wählen")
                        iconButton("xmark") { image.wrappedValue = "" }.help("Kein Bild")
                    }
                }
                setting(String(localized: "Deckkraft der Kacheln", bundle: Bundle.app)) { slider(model.binding(\.tileOpacity), 0...100, step: 1, factor: 100, unit: "%") }
            }

            heading(String(localized: "Baum und Kacheln", bundle: Bundle.app))
            table {
                setting(String(localized: "Design des Baums", bundle: Bundle.app)) { menu(model.binding(\.sidebarStyle), SidebarStyle.allCases.map { ($0, $0.title) }) }
                setting(String(localized: "Laufzeit im Baum", bundle: Bundle.app)) { onOff(model.binding(\.sidebarShowAge)) }
                setting(String(localized: "Pfad in Stack-Zeilen", bundle: Bundle.app)) { onOff(model.binding(\.stackShowPath)) }
                setting(String(localized: "Letzte Antwort von Claude im Baum", bundle: Bundle.app)) { onOff(model.binding(\.showLastMessage)) }
                setting(String(localized: "Sounds", bundle: Bundle.app)) { menu(model.binding(\.sounds), Feedback.Level.allCases.map { ($0, $0.title) }) }
                setting(String(localized: "Systembenachrichtigungen", bundle: Bundle.app)) { menu(model.binding(\.notifications), Notifications.Level.allCases.map { ($0, $0.title) }) }
            }

            heading(String(localized: "Updates", bundle: Bundle.app))
            table {
                setting(String(localized: "Nach Updates suchen", bundle: Bundle.app)) { onOff(model.binding(\.checkForUpdates)) }
            }

            heading(String(localized: "Auto-Modus", bundle: Bundle.app), note: String(localized: "AUTO in der Leiste", bundle: Bundle.app))
            table {
                setting(String(localized: "Gilt für", bundle: Bundle.app)) { menu(model.binding(\.autoAllSessions), [(true, String(localized: "alle Sessions", bundle: Bundle.app)), (false, String(localized: "nur die Auswahl im Baum", bundle: Bundle.app))]) }
                setting(String(localized: "Zeigt", bundle: Bundle.app)) {
                    let waiting = model.binding(\.autoWaiting), running = model.binding(\.autoRunning)
                    menu(Binding(get: { AutoShow(waiting: waiting.wrappedValue, running: running.wrappedValue) },
                                 set: { waiting.wrappedValue = $0.waiting; running.wrappedValue = $0.running }),
                         [(AutoShow(waiting: true, running: false), String(localized: "wartende", bundle: Bundle.app)), (AutoShow(waiting: false, running: true), String(localized: "arbeitende", bundle: Bundle.app)),
                          (AutoShow(waiting: true, running: true), String(localized: "wartende und arbeitende", bundle: Bundle.app)), (AutoShow(waiting: false, running: false), String(localized: "keine", bundle: Bundle.app))])
                }
            }

            heading(String(localized: "Rückfragen", bundle: Bundle.app), note: String(localized: "aus = ohne Nachfrage ausführen", bundle: Bundle.app))
            table {
                ForEach(Settings.Ask.allCases, id: \.self) { a in
                    setting(a.title) { onOff(model.binding(a, \.enabled)) }
                }
            }

            heading(String(localized: "Tastenkürzel", bundle: Bundle.app), note: String(localized: "Kürzel anklicken, Tasten drücken · ⌫ entfernt", bundle: Bundle.app))
            ForEach(hotkeyGroups, id: \.0) { title, actions in
                Text(title).font(Theme.ui(11, bold: true)).foregroundStyle(Theme.mutedColor)
                    .padding(.horizontal, 16).padding(.top, title == hotkeyGroups.first?.0 ? 0 : 12).padding(.bottom, 4)
                table { ForEach(actions, id: \.self) { a in row(a) } }
            }
            Color.clear.frame(height: 12)
            DialogFoot(hint: String(localized: "Änderungen gelten sofort · Esc schließt", bundle: Bundle.app), button: String(localized: "Fertig", bundle: Bundle.app)) { model.stopRecording(); model.onClose?() }
        }
        .frame(width: 900 * Theme.scale, alignment: .leading)
        .background(Theme.panelColor)
        .environment(\.locale, model.locale)
        .onDisappear { model.stopRecording() }
    }

    /// Breite der rechten Spalte: jedes Dropdown, Feld und Kürzel ist gleich breit, die Einstellungen lesen sich wie eine Tabelle.
    private var controlWidth: CGFloat { 300 * Theme.scale }

    /// Zeilen einer Tabelle mit feiner Linie dazwischen.
    private func table<C: View>(@ViewBuilder _ rows: () -> C) -> some View {
        VStack(spacing: 0) {
            SwiftUI.Group(subviews: rows()) { subviews in
                ForEach(subviews.indices, id: \.self) { i in
                    if i > 0 { Divider().overlay(Theme.lineColor) }
                    subviews[i]
                }
            }
        }
        .overlay(Rectangle().stroke(Theme.lineColor, lineWidth: 1))
        .padding(.horizontal, 16)
    }

    private func setting<C: View>(_ title: String, @ViewBuilder _ control: () -> C) -> some View {
        HStack(spacing: 12) {
            Text(title).foregroundStyle(Theme.fgColor).lineLimit(1)
            Spacer(minLength: 12)
            control().frame(width: controlWidth)
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 10).padding(.vertical, 5)
    }

    /// Dropdown im App-Stil, füllt die rechte Spalte. Das native Popup nimmt nur die Breite seines Textes.
    private func menu<T: Hashable>(_ selection: Binding<T>, _ items: [(T, String)]) -> some View {
        Menu {
            Picker("", selection: selection) {
                ForEach(items, id: \.0) { Text($0.1).tag($0.0) }
            }
            .pickerStyle(.inline).labelsHidden()
        } label: {
            HStack(spacing: 4) {
                Text(items.first { $0.0 == selection.wrappedValue }?.1 ?? "–").lineLimit(1).foregroundStyle(Theme.fgColor)
                Spacer(minLength: 4)
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 9 * Theme.scale)).foregroundStyle(Theme.mutedColor)
            }
            .padding(.horizontal, 8).padding(.vertical, 3)
            .frame(width: controlWidth).background(Theme.bgColor).contentShape(Rectangle())
        }
        .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).kbdFocusRing()
        .frame(width: controlWidth)
    }

    /// Ganzzahliger Schieberegler mit Wert rechts daneben. `factor` rechnet gespeicherte Faktoren in Prozent um.
    /// `live: false` übernimmt erst beim Loslassen (UI-Größe: sonst skaliert der Dialog unter der Maus mit).
    private func slider(_ value: Binding<Double>, _ range: ClosedRange<Double>, step: Double, factor: Double = 1, unit: String, live: Bool = true) -> some View {
        IntSlider(value: value, range: range, step: step, factor: factor, unit: unit, live: live)
    }

    /// Pfadfeld der rechten Spalte, nimmt Dateien per Drag & Drop an.
    private func pathField(_ text: Binding<String>, placeholder: String = "", onTab: @escaping () -> Void = {}, drop: @escaping ([URL]) -> Bool) -> some View {
        PathField(text: text, placeholder: placeholder, autofocus: false, onTab: onTab, onSubmit: {}, onMove: { _ in })
            .frame(height: 18 * Theme.scale).padding(.horizontal, 8).padding(.vertical, 2)
            .background(Theme.bgColor)
            .dropDestination(for: URL.self) { urls, _ in drop(urls) }
    }

    private func iconButton(_ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).foregroundStyle(Theme.mutedColor).frame(width: 22 * Theme.scale, height: 22 * Theme.scale)
        }
        .buttonStyle(.plain).kbdFocusRing()
    }

    private func onOff(_ value: Binding<Bool>) -> some View { menu(value, [(true, String(localized: "an", bundle: Bundle.app)), (false, String(localized: "aus", bundle: Bundle.app))]) }

    /// Texte als Dropdown. "" heißt Claude-Default.
    private func options(_ values: [String], _ value: Binding<String>) -> some View {
        menu(value, values.map { ($0, $0.isEmpty ? String(localized: "Standard", bundle: Bundle.app) : $0) })
    }

    private func row(_ a: HotkeyAction) -> some View {
        let key = model.hotkeys[a]
        let isRecording = model.recording == a
        return HStack(spacing: 8) {
            Text(a.title).foregroundStyle(Theme.fgColor).lineLimit(1)
            Spacer(minLength: 12)
            if key != a.defaultKey, !isRecording {
                Button { model.hotkeys[a] = a.defaultKey } label: { Text("Standard").foregroundStyle(Theme.mutedColor) }
                    .buttonStyle(.plain).kbdFocusRing().help("Zurück auf \(a.defaultKey.display)")
            }
            Button { isRecording ? model.stopRecording() : model.record(a) } label: {
                Text(isRecording ? String(localized: "Tasten drücken …", bundle: Bundle.app) : key?.display ?? "–")
                    .font(Theme.ui(12, bold: true))
                    .foregroundStyle(isRecording ? Theme.bgColor : Theme.fgColor)
                    .padding(.horizontal, 8).padding(.vertical, 3).frame(width: controlWidth, alignment: .leading)
                    .background(isRecording ? Theme.runningColor : Theme.bgColor)
            }
            .buttonStyle(.plain).kbdFocusRing()
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 10).padding(.vertical, 5)
    }
}

private struct IntSlider: View {
    @Binding var value: Double
    let range: ClosedRange<Double>, step: Double, factor: Double, unit: String, live: Bool
    /// Wert während des Ziehens, solange er noch nicht übernommen ist.
    @State private var draft: Double?

    var body: some View {
        let shown = draft ?? (value * factor).rounded()
        HStack(spacing: 10) {
            Slider(value: Binding(get: { shown }, set: { v in
                let r = (v / step).rounded() * step
                if live { value = r / factor } else { draft = r }
            }), in: range, step: step) { editing in
                if !editing, let d = draft { value = d / factor; draft = nil }
            }
            .controlSize(.small).tint(Theme.runningColor)
            Text("\(Int(shown)) \(unit)").monospacedDigit().foregroundStyle(Theme.fgColor)
                .frame(width: 60 * Theme.scale, alignment: .trailing)
        }
    }
}

/// Auswahl „Auto-Modus zeigt“: zwei Schalter als ein Dropdown.
private struct AutoShow: Hashable {
    let waiting: Bool
    let running: Bool
}
