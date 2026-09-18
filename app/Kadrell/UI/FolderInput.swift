import SwiftUI
import AppKit

/// Nativer Ordnerdialog. Liefert den gewählten Pfad oder nil. `images: true` wählt stattdessen eine Bilddatei.
@MainActor
func chooseFolder(start: String, images: Bool = false, completion: @escaping (String?) -> Void) {
    guard !AppBinary.replaced else { warnReplaced(); completion(nil); return }
    let panel = NSOpenPanel()
    panel.canChooseDirectories = !images
    panel.canChooseFiles = images
    if images { panel.allowedContentTypes = [.image] }
    panel.canCreateDirectories = true
    panel.allowsMultipleSelection = false
    panel.directoryURL = URL(fileURLWithPath: FolderIndex.normalize(start))
    panel.prompt = String(localized: "Wählen", bundle: Bundle.app)
    panel.level = NSWindow.Level(rawValue: NSWindow.Level.floating.rawValue + 1)   // über dem Dialog-Overlay
    panel.begin { resp in
        completion(resp == .OK ? panel.url.map { $0.path } : nil)
    }
}

/// Wurde die App ersetzt, während sie läuft (Update, Neubau), verwirft macOS den Dateidialog wegen der geänderten Signatur,
/// und AppKit hängt beim Wiederholen alle 10 s den Main-Thread auf. Stempel der Programmdatei beim Start gegen jetzt.
@MainActor
enum AppBinary {
    static let atLaunch = stamp()
    static var replaced: Bool { stamp() != atLaunch }
    private static func stamp() -> String? {
        guard let path = Bundle.main.executablePath, let a = try? FileManager.default.attributesOfItem(atPath: path) else { return nil }
        return "\(a[.systemFileNumber] ?? "")/\((a[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0)"
    }
}

@MainActor
private func warnReplaced() {
    let alert = NSAlert()
    alert.messageText = String(localized: "Kadrell wurde seit dem Start ersetzt", bundle: Bundle.app)
    alert.informativeText = String(localized: "Der Dateiauswahl-Dialog öffnet sich erst nach einem Neustart von Kadrell. Pfade lassen sich weiter eintippen oder hineinziehen.", bundle: Bundle.app)
    alert.runModal()
}

/// Fußleiste der Dialoge: Hinweis links, Aktion rechts als Button (⏎ löst dieselbe Aktion aus).
/// Zeichnet sich nicht an Ort und Stelle, sondern meldet sich an `OverlayScroll`: dort steht sie fest unter
/// dem scrollenden Inhalt, der Button bleibt also auch in langen Dialogen sichtbar.
struct DialogFoot: View {
    let hint: String
    let button: String
    let action: () -> Void

    var body: some View {
        Color.clear.frame(height: 0).preference(key: DialogFootKey.self, value: DialogFootData(hint: hint, button: button, action: action))
    }
}

struct DialogFootData: Equatable {
    let hint: String
    let button: String
    let action: () -> Void
    static func == (a: Self, b: Self) -> Bool { a.hint == b.hint && a.button == b.button }
}

struct DialogFootKey: PreferenceKey {
    static var defaultValue: DialogFootData? { nil }
    static func reduce(value: inout DialogFootData?, nextValue: () -> DialogFootData?) { value = nextValue() ?? value }
}

struct DialogFootBar: View {
    let foot: DialogFootData

    var body: some View {
        VStack(spacing: 0) {
            Divider().overlay(Theme.lineColor)
            HStack(spacing: 12) {
                Text(foot.hint).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                Spacer()
                Button(action: foot.action) {
                    Text("\(foot.button)  ⏎").font(Theme.ui(12, bold: true)).foregroundStyle(Theme.bgColor)
                        .padding(.horizontal, 12).padding(.vertical, 6).background(Theme.runningColor)
                }.buttonStyle(.plain).kbdFocusRing()
            }
            .padding(.horizontal, 16).padding(.vertical, 10)
        }
        .background(Theme.panelColor)
    }
}

/// Setzt den Text senkrecht in die Mitte. Ein randloses NSTextField zeichnet seine Zeile sonst oben im Rahmen:
/// in unseren Feldern ist der Rahmen höher als die Zeile, der Text klebt dadurch an der Oberkante.
final class CenteredTextFieldCell: NSTextFieldCell {
    /// Zeilenhöhe aus der Font-Metrik. Nicht `cellSize`, das bei scrollbaren Feldern die volle Rahmenhöhe liefert.
    private var lineHeight: CGFloat {
        let f = font ?? NSFont.systemFont(ofSize: NSFont.systemFontSize)
        return ceil(f.ascender - f.descender)
    }

    private func centered(_ rect: NSRect) -> NSRect {
        guard rect.height > lineHeight else { return rect }
        return NSRect(x: rect.minX, y: rect.minY + (rect.height - lineHeight) / 2, width: rect.width, height: lineHeight)
    }

    /// Feldeditor (beim Tippen) senkrecht mittig platzieren.
    override func drawingRect(forBounds rect: NSRect) -> NSRect { centered(super.drawingRect(forBounds: rect)) }
    override func edit(withFrame rect: NSRect, in view: NSView, editor: NSText, delegate: Any?, event: NSEvent?) {
        super.edit(withFrame: centered(rect), in: view, editor: editor, delegate: delegate, event: event)
    }
    override func select(withFrame rect: NSRect, in view: NSView, editor: NSText, delegate: Any?, start: Int, length: Int) {
        super.select(withFrame: centered(rect), in: view, editor: editor, delegate: delegate, start: start, length: length)
    }

    /// Text und Platzhalter zeichnet NSTextFieldCell fest an der Oberkante und ignoriert dabei jeden Rahmen-Offset,
    /// den man ihr gibt. Darum hier selbst zeichnen, senkrecht mittig. Gilt nur, solange nicht editiert wird
    /// (beim Tippen übernimmt der Feldeditor, siehe oben).
    override func drawInterior(withFrame cellFrame: NSRect, in controlView: NSView) {
        let text = attributedStringValue.length > 0 ? attributedStringValue : (placeholderAttributedString ?? NSAttributedString())
        guard text.length > 0 else { return }
        text.draw(at: CGPoint(x: cellFrame.minX + 2, y: cellFrame.midY - text.size().height / 2))
    }
}
