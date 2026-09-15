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

/// Stift am Gruppen-Header: Name und Farbe (nativer Picker plus Palette). Der Ordner bleibt fest.
struct EditGroupView: View {
    @Bindable var model: EditGroupModel
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            Text("GRUPPE BEARBEITEN").font(Theme.ui(11)).kerning(0.6).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.top, 12)
            HStack(spacing: 12) {
                TextField("Name", text: $model.name).textFieldStyle(.plain).font(Theme.ui(15))
                    .focused($focused)
                    .onSubmit { model.save() }
                ColorPicker("", selection: $model.color, supportsOpacity: false).labelsHidden()
                HStack(spacing: 4) {
                    ForEach(Theme.palette, id: \.self) { hex in
                        Rectangle().fill(Color(nsColor: NSColor(hexString: hex))).frame(width: 14, height: 14)
                            .onTapGesture { model.color = Color(nsColor: NSColor(hexString: hex)) }
                    }
                }
            }
            .padding(14)
            Text(Theme.shortPath(model.group.cwd)).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.bottom, 12)
            DialogFoot(hint: "Esc abbrechen", button: "Speichern") { model.save() }
        }
        .font(Theme.ui(12))
        .foregroundStyle(Theme.fgColor)
        .frame(width: 640 * Theme.scale)
        .background(Theme.panelColor)
        .onAppear { focused = true }
    }
}
