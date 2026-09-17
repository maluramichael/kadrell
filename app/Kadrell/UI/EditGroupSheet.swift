import SwiftUI

@Observable
@MainActor
final class EditGroupModel {
    let group: Group
    var name: String
    var color: Color
    var onSave: ((Group) -> Void)?

    init(group: Group) {
        self.group = group
        name = group.name
        color = Color(nsColor: NSColor(hexString: group.color))
    }

    func save() {
        var g = group
        let n = name.trimmingCharacters(in: .whitespaces)
        if !n.isEmpty { g.name = n }
        g.color = NSColor(color).hexString
        onSave?(g)
    }
}

/// Stift am Gruppen-Header: Name und Farbe. Klick auf die Farbe klappt die Paletten auf. Der Ordner bleibt fest.
struct EditGroupView: View {
    @Bindable var model: EditGroupModel
    @FocusState private var focused: Bool
    @State private var showPalettes = false

    var body: some View {
        VStack(spacing: 0) {
            Text("GRUPPE BEARBEITEN").dialogTitle()
            HStack(spacing: 12) {
                TextField("Name", text: $model.name).textFieldStyle(.plain).font(Theme.ui(15))
                    .focused($focused)
                    .onSubmit { model.save() }
                Button { showPalettes.toggle() } label: {
                    Capsule().fill(model.color).frame(width: 44 * Theme.scale, height: 20 * Theme.scale)
                        .overlay(Capsule().stroke(Theme.mutedColor, lineWidth: showPalettes ? 1.5 : 0))
                }
                .buttonStyle(.plain).kbdFocusRing()
                .accessibilityLabel(String(localized: "Farbe wählen", bundle: Bundle.app))
                .help("Farbe aus einer Palette wählen")
            }
            .padding(14)
            if showPalettes { palettes }
            Text(Theme.shortPath(model.group.cwd)).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.bottom, 12)
            DialogFoot(hint: String(localized: "Esc abbrechen", bundle: Bundle.app), button: String(localized: "Speichern", bundle: Bundle.app)) { model.save() }
        }
        .dialogFrame()
        .onAppear { focused = true }
    }

    private var palettes: some View {
        let current = NSColor(model.color).hexString
        let size = 18 * Theme.scale
        return VStack(alignment: .leading, spacing: 8) {
            ForEach(Theme.palettes, id: \.name) { p in
                HStack(spacing: 4) {
                    Text(p.name).foregroundStyle(Color(nsColor: Theme.sub)).frame(width: 110 * Theme.scale, alignment: .leading)
                    ForEach(p.colors, id: \.self) { hex in
                        Button { model.color = Color(nsColor: NSColor(hexString: hex)); showPalettes = false } label: {
                            Rectangle().fill(Color(nsColor: NSColor(hexString: hex))).frame(width: size, height: size)
                                .overlay(Rectangle().stroke(Theme.fgColor, lineWidth: hex == current ? 2 : 0))
                        }
                        .buttonStyle(.plain).kbdFocusRing()
                        .accessibilityLabel("\(p.name) \(hex)")
                        .accessibilityAddTraits(hex == current ? .isSelected : [])
                    }
                }
            }
            HStack(spacing: 4) {
                Text("Eigene").foregroundStyle(Color(nsColor: Theme.sub)).frame(width: 110 * Theme.scale, alignment: .leading)
                ColorPicker(String(localized: "Eigene Farbe", bundle: Bundle.app), selection: $model.color, supportsOpacity: false).labelsHidden()
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
                .padding(14)
            Text("Leer lassen: wieder der Titel von Claude Code (\(model.session.autoTitle))").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.bottom, 12)
            DialogFoot(hint: String(localized: "Esc abbrechen", bundle: Bundle.app), button: String(localized: "Speichern", bundle: Bundle.app)) { model.save() }
        }
        .dialogFrame()
        .onAppear { focused = true }
    }
}
