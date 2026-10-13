import SwiftUI

/// ⌘, Einstellungen: links die Seiten, rechts deren Inhalt. Änderungen gelten sofort, Esc schließt.
/// ↑↓ in der Leiste oder ⌘↑ / ⌘↓ von überall wechseln die Seite, Tab geht in den Inhalt.
struct SettingsView: View {
    @Bindable var model: SettingsModel
    @FocusState private var sidebarFocused: Bool

    /// Feste Höhe, damit der Dialog beim Seitenwechsel nicht springt; der Inhalt scrollt.
    private var height: CGFloat { 640 * Theme.scale }
    private var sidebarWidth: CGFloat { 170 * Theme.scale }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(String(localized: "EINSTELLUNGEN", bundle: Bundle.app)).kerning(0.6)
                Spacer()
                Text(verbatim: "Kadrell v\(Settings.version)")
            }
            .font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
            .padding(.horizontal, 16).padding(.top, 12)
            Divider().overlay(Theme.lineColor).padding(.top, 12)
            HStack(alignment: .top, spacing: 0) {
                sidebar
                Divider().overlay(Theme.lineColor)
                ScrollView(.vertical) {
                    VStack(alignment: .leading, spacing: 0) {
                        content(model.page)
                        Color.clear.frame(height: 16)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .id(model.page)
            }
            .frame(height: height)
            DialogFoot(hint: String(localized: "↑↓ oder ⌘↑↓ Seite · Tab in den Inhalt · Änderungen gelten sofort · Esc schließt", bundle: Bundle.app),
                       button: String(localized: "Fertig", bundle: Bundle.app)) { model.stopRecording(); model.onClose?() }
        }
        .frame(width: 1000 * Theme.scale, alignment: .leading)
        .background(Theme.panelColor)
        .background {
            Button("") { model.movePage(-1) }.keyboardShortcut(.upArrow, modifiers: .command).opacity(0).accessibilityHidden(true)
            Button("") { model.movePage(1) }.keyboardShortcut(.downArrow, modifiers: .command).opacity(0).accessibilityHidden(true)
        }
        .environment(\.locale, model.locale)
        .onAppear { sidebarFocused = true }
        .onDisappear { model.stopRecording() }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(SettingsPage.allCases, id: \.self) { p in
                if p.startsHarnesses {
                    Text(String(localized: "HARNESSES", bundle: Bundle.app)).kerning(0.6).font(Theme.ui(10, bold: true)).foregroundStyle(Theme.mutedColor)
                        .padding(.horizontal, 16).padding(.top, 14).padding(.bottom, 4)
                }
                sidebarRow(p)
            }
            Spacer()
        }
        .padding(.top, 8)
        .frame(width: sidebarWidth, alignment: .leading)
        .frame(maxHeight: .infinity, alignment: .top)
        .focusable()
        .focusEffectDisabled()
        .focused($sidebarFocused)
        .onKeyPress(.upArrow) { model.movePage(-1); return .handled }
        .onKeyPress(.downArrow) { model.movePage(1); return .handled }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(String(localized: "Seiten", bundle: Bundle.app))
    }

    private func sidebarRow(_ p: SettingsPage) -> some View {
        let on = p == model.page
        return Text(p.title)
            .font(Theme.ui(12, bold: on))
            .foregroundStyle(on ? Theme.fgColor : Theme.mutedColor)
            .padding(.leading, p.indented ? 28 : 16).padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(on ? Theme.surfaceColor : .clear)
            .overlay(alignment: .leading) { if on { (sidebarFocused ? Theme.runningColor : Theme.lineColor).frame(width: 2) } }
            .contentShape(Rectangle())
            .onTapGesture { model.page = p; sidebarFocused = true }
            .accessibilityAddTraits(on ? [.isButton, .isSelected] : .isButton)
            .accessibilityAction { model.page = p }
    }
}
