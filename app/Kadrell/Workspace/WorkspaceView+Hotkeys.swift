import AppKit

/// Kürzel, die nur die Arbeitsfläche betreffen. Vorschau, Baum, Editor und Dialoge behält der AppDelegate.
extension WorkspaceView {
    private static let hotkeyActions: [HotkeyAction: @MainActor (WorkspaceView) -> Void] = [
        .focusLeft: { $0.moveFocus(.left) }, .focusRight: { $0.moveFocus(.right) },
        .focusUp: { $0.moveFocus(.up) }, .focusDown: { $0.moveFocus(.down) },
        .swapLeft: { $0.swapFocused(.left) }, .swapRight: { $0.swapFocused(.right) },
        .swapUp: { $0.swapFocused(.up) }, .swapDown: { $0.swapFocused(.down) },
        .resizeLeft: { $0.resizeFocused(.left) }, .resizeRight: { $0.resizeFocused(.right) },
        .resizeUp: { $0.resizeFocused(.up) }, .resizeDown: { $0.resizeFocused(.down) },
        .splitRight: { $0.setSplit("r") }, .splitDown: { $0.setSplit("d") },
        .nextSession: { $0.cycleFocus(1) }, .prevSession: { $0.cycleFocus(-1) }, .lastSession: { $0.focusLast() },
        .zoom: { $0.toggleZen() }, .nextLayout: { $0.setMode($0.mode.next) }, .syncInput: { $0.toggleSync() },
        .closeFocused: { $0.removeFocused() },
    ]

    /// false: keine Aktion der Arbeitsfläche, der Aufrufer ist dran.
    func perform(_ action: HotkeyAction) -> Bool {
        if let i = action.tileIndex { focusTile(i); return true }
        guard let run = Self.hotkeyActions[action] else { return false }
        run(self)
        return true
    }
}
