import SwiftUI

/// Rückfrage im App-Design (statt NSAlert). ⏎ bestätigt, Esc bricht ab.
struct ConfirmView: View {
    let title: String
    let info: String
    let button: String
    let destructive: Bool
    let onConfirm: () -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title).font(Theme.ui(14, bold: true)).foregroundStyle(Theme.fgColor)
                .padding(.horizontal, 16).padding(.top, 16)
            if !info.isEmpty {
                Text(info).font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
                    .padding(.horizontal, 16).padding(.top, 8)
            }
            HStack(spacing: 8) {
                Spacer()
                Button(action: onCancel) {
                    Text("Abbrechen").font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
                        .padding(.horizontal, 12).padding(.vertical, 6).overlay(Rectangle().stroke(Theme.lineColor, lineWidth: 1))
                }.buttonStyle(.plain)
                Button(action: onConfirm) {
                    Text(button).font(Theme.ui(12, bold: true)).foregroundStyle(Theme.bgColor)
                        .padding(.horizontal, 12).padding(.vertical, 6).background(destructive ? Theme.errorColor : Theme.runningColor)
                }.buttonStyle(.plain)
            }
            .padding(16)
        }
        .frame(width: 520 * Theme.scale, alignment: .leading)
        .background(Theme.panelColor)
    }
}

extension Theme {
    static let errorColor = Color(nsColor: error)
}
