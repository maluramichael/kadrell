import SwiftUI

@Observable
@MainActor
final class EditGroupModel {
    let group: Group
    /// Neue, noch leere Gruppe statt einer bestehenden: anderer Titel, kein Hinweis auf laufende Sessions.
    let isNew: Bool
    /// Die übrigen Gruppen: Hinweis, wenn der neue Ordner schon einer anderen gehört.
    let others: [Group]
    var name: String
    var color: Color
    /// Ordner wie getippt, Kürzel erlaubt (`~/d/p/kad`).
    var folder: String { didSet { if folder != oldValue { folderChanged() } } }
    /// Aufgelöster Ordner; nil, solange es ihn nicht gibt oder das Kürzel mehrdeutig ist.
    private(set) var resolvedFolder: String?
    /// Zuletzt gültiger Ordner: daran erkennt der Name, ob er noch der automatische ist.
    private var lastFolder: String
    var onSave: ((Group) -> Void)?

    /// Remote-Gruppe: der Ordner ist nur Platzhalter, sie hängt am ssh-Host.
    var isRemote: Bool { group.host != nil }
    /// Neuer Ordner gewählt: laufende Sessions bleiben, wo sie sind, neue starten dort.
    var folderMoved: Bool { !isNew && resolvedFolder.map { $0 != group.cwd } ?? false }
    /// Eine andere lokale Gruppe hat schon diesen Ordner. Erlaubt (auch per `kadrell new-group` möglich), nur ein Hinweis.
    var sharedWith: Group? { resolvedFolder.flatMap { f in others.first { $0.cwd == f && $0.host == nil } } }

    init(group: Group, others: [Group] = [], isNew: Bool = false) {
        self.group = group
        self.isNew = isNew
        self.others = others
        name = group.name
        color = Color(nsColor: NSColor(hexString: group.color))
        folder = Theme.shortPath(group.cwd)
        resolvedFolder = group.cwd
        lastFolder = group.cwd
    }

    /// Vorhandener Ordner gilt direkt, sonst ein eindeutiges Kürzel. Mehrdeutig: erst per Tab wählen.
    nonisolated static func resolve(_ typed: String) -> String? {
        let p = FolderIndex.normalize(typed)
        guard !p.isEmpty else { return nil }
        if FolderIndex.isDirectory(p) { return p }
        let hits = FolderIndex.expandAbbreviated(typed, limit: 2)
        return hits.count == 1 ? FolderIndex.normalize(hits[0]) : nil
    }

    /// Hieß die Gruppe wie ihr Ordner, zieht der Name mit dem Ordner mit; ein eigener Name bleibt.
    private func folderChanged() {
        resolvedFolder = Self.resolve(folder)
        guard let now = resolvedFolder, now != lastFolder else { return }
        if name == GroupStore.defaultName(cwd: lastFolder) { name = GroupStore.defaultName(cwd: now) }
        lastFolder = now
    }

    /// Tab: erster Treffer des Kürzels ausgeschrieben ins Feld, wie bei Neue Session.
    func completeFolder() {
        guard let p = FolderIndex.expandAbbreviated(folder).first else { NSSound.beep(); return }
        folder = Theme.shortPath(FolderIndex.normalize(p))
    }

    /// ⌘O oder Ordner-Taste: nativer Ordnerdialog.
    func browse() {
        chooseFolder(start: resolvedFolder ?? group.cwd) { [weak self] p in if let p { self?.folder = Theme.shortPath(p) } }
    }

    /// Ordner aus dem Finder auf den Dialog gezogen.
    func drop(_ urls: [URL]) -> Bool {
        guard !isRemote, let dir = urls.lazy.compactMap({ FolderIndex.folder(for: $0.path) }).first else { return false }
        folder = Theme.shortPath(dir)
        return true
    }

    /// Zeile unter dem Ordner: Fehler, sonst was der neue Ordner bewirkt, sonst die Eingabe-Hilfen.
    var folderHint: (text: String, error: Bool) {
        if isRemote { return ("ssh · \(group.host ?? "")", false) }
        guard let dir = resolvedFolder else {
            return (String(localized: "Ordner nicht gefunden · Tab vervollständigt, ⌘O wählt im Finder", bundle: Bundle.app), true)
        }
        // Kürzel getippt: der ausgeschriebene Ordner davor, sonst stünde er nirgends.
        var lines = FolderIndex.normalize(folder) == dir ? [] : ["→ " + Theme.shortPath(dir)]
        if folderMoved { lines.append(String(localized: "Neue Sessions starten im neuen Ordner, laufende bleiben in ihrem.", bundle: Bundle.app)) }
        if let other = sharedWith { lines.append(String(localized: "Den Ordner hat auch die Gruppe „\(other.name)“.", bundle: Bundle.app)) }
        guard !lines.isEmpty, isNew || folderMoved else {
            return (String(localized: "Tab vervollständigt · ⌘O Finder · Ordner hineinziehen", bundle: Bundle.app), false)
        }
        return (lines.joined(separator: "\n"), false)
    }

    /// Ohne gültigen Ordner kein Speichern: sonst stünde die Gruppe auf einem Pfad, in dem keine Session starten kann.
    func save() {
        var g = group
        let n = name.trimmingCharacters(in: .whitespaces)
        if !n.isEmpty { g.name = n }
        g.color = NSColor(color).hexString
        if !isRemote {
            guard let dir = resolvedFolder else { NSSound.beep(); return }
            g.cwd = dir
        }
        onSave?(g)
    }
}

/// Stift am Gruppen-Header: Name, Ordner und Farbe aus einer Palette. Die Paletten stehen immer offen.
struct EditGroupView: View {
    @Bindable var model: EditGroupModel
    @FocusState private var focused: Bool
    @State private var dropping = false

    var body: some View {
        VStack(spacing: 0) {
            (model.isNew ? Text("NEUE GRUPPE") : Text("GRUPPE BEARBEITEN")).dialogTitle()
            HStack(spacing: 12) {
                TextField("Name", text: $model.name).textFieldStyle(.plain).font(Theme.ui(15))
                    .focused($focused)
                    .onSubmit { model.save() }
                    .instrumentWell(focused: focused)
                Capsule().fill(model.color).frame(width: 44 * Theme.scale, height: 20 * Theme.scale)
                    .accessibilityHidden(true)
            }
            .padding(.horizontal, 14).padding(.top, 14)
            if !model.isRemote { folderRow.padding(.horizontal, 14).padding(.top, 10) }
            let hint = model.folderHint
            Text(hint.text).font(Theme.ui(11)).foregroundStyle(hint.error ? Theme.errorTextColor : Theme.mutedColor)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.top, 6).padding(.bottom, 14)
            palettes
            DialogFoot(hint: String(localized: "Esc abbrechen", bundle: Bundle.app),
                       button: model.isNew ? String(localized: "Anlegen", bundle: Bundle.app) : String(localized: "Speichern", bundle: Bundle.app)) { model.save() }
        }
        .dialogFrame()
        .overlay { if dropping { RoundedRectangle(cornerRadius: Instrument.dialogRadius * Theme.scale).strokeBorder(Theme.accentColor, lineWidth: 2) } }
        .dropDestination(for: URL.self) { urls, _ in model.drop(urls) } isTargeted: { dropping = $0 && !model.isRemote }
        .background { Button("", action: model.browse).keyboardShortcut("o", modifiers: .command).opacity(0).accessibilityHidden(true) }
        .onAppear { focused = true }
    }

    /// Wie bei Neue Session: Kürzel tippen, Tab schreibt aus, die Taste daneben (⌘O) öffnet den Finder.
    private var folderRow: some View {
        HStack(spacing: 6) {
            PathField(text: $model.folder, placeholder: String(localized: "Ordner (~/d/p/kad kürzt ab)", bundle: Bundle.app), autofocus: false,
                      onTab: model.completeFolder, onSubmit: model.save, onMove: { _ in })
            Button(action: model.browse) {
                Image(systemName: "folder").frame(width: 22 * Theme.scale, height: 16 * Theme.scale)
            }
            .buttonStyle(InstrumentKeyStyle(.neutral, compact: true)).kbdFocusRing()
            .help(String(localized: "Im Finder wählen (⌘O)", bundle: Bundle.app))
            .accessibilityLabel(String(localized: "Ordner im Finder wählen", bundle: Bundle.app))
        }
    }

    private var palettes: some View {
        let current = NSColor(model.color).hexString
        let size = 18 * Theme.scale
        return VStack(alignment: .leading, spacing: 8) {
            ForEach(Theme.palettes, id: \.name) { p in
                HStack(spacing: 4) {
                    Text(p.name).foregroundStyle(Color(nsColor: Theme.sub)).frame(width: 110 * Theme.scale, alignment: .leading)
                    ForEach(p.colors, id: \.self) { hex in
                        Button { model.color = Color(nsColor: NSColor(hexString: hex)) } label: {
                            let swatch = RoundedRectangle(cornerRadius: 3 * Theme.scale)
                            swatch.fill(Color(nsColor: NSColor(hexString: hex))).frame(width: size, height: size)
                                .overlay(swatch.strokeBorder(Theme.fgColor, lineWidth: hex == current ? 1.5 : 0))
                        }
                        .buttonStyle(.plain).kbdFocusRing()
                        .accessibilityLabel("\(p.name) \(hex)")
                        .accessibilityAddTraits(hex == current ? .isSelected : [])
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16).padding(.bottom, 12)
    }
}

@Observable
@MainActor
final class RenameSessionModel {
    let session: Session
    var name: String
    var onSave: ((String) -> Void)?

    init(session: Session) {
        self.session = session
        name = session.title
    }

    func save() { onSave?(name) }
}

/// F2 oder Stift an der Session: eigener Name. Leer lassen = wieder der Titel von Claude Code.
struct RenameSessionView: View {
    @Bindable var model: RenameSessionModel
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            Text("SESSION UMBENENNEN").dialogTitle()
            TextField(model.session.autoTitle, text: $model.name).textFieldStyle(.plain).font(Theme.ui(15))
                .focused($focused)
                .onSubmit { model.save() }
                .instrumentWell(focused: focused)
                .padding(14)
            Text("Leer lassen: wieder der Titel von Claude Code (\(model.session.autoTitle))").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.bottom, 12)
            DialogFoot(hint: String(localized: "Esc abbrechen", bundle: Bundle.app), button: String(localized: "Speichern", bundle: Bundle.app)) { model.save() }
        }
        .dialogFrame()
        .onAppear { focused = true }
    }
}
