import AppKit

/// Zeichnet ein einzelnes Bauteil des Instrument-Looks (Fläche, Gruppe, Feld, Zeile, Taste, Fuge). AppKit setzt sie
/// direkt ein, SwiftUI über einen Wrapper. Sie nimmt keine Klicks an, die gehen an die Views darüber.
final class InstrumentSurfaceView: NSView {
    enum Part: Equatable {
        case raised
        case group
        case well(focused: Bool)
        case row(outline: Bool, ring: Bool)
        case key(pressed: Bool, role: KeyRole)
        case hSeam
    }

    var part: Part { didSet { needsDisplay = true } }

    init(part: Part, frame: NSRect = .zero) {
        self.part = part
        super.init(frame: frame)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { false }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func draw(_ dirtyRect: NSRect) {
        Theme.scaled(bounds) { r in draw(part, in: r) }
    }

    private func draw(_ part: Part, in r: CGRect) {
        switch part {
        case .raised:
            Instrument.raisedPanel(CGRect(x: 0, y: 0, width: r.width, height: max(0, r.height - Instrument.shadowDepth)))
        case .group:
            Instrument.group(r)
        case .well(let focused):
            Instrument.well(r, focused: focused)
        case .row(let outline, let ring):
            Instrument.rowHighlight(r, outline: outline, ring: ring)
        case .key(let pressed, let role):
            Instrument.key(r, pressed: pressed, fill: role.fill)
        case .hSeam:
            Instrument.hSeam(x: 0, y: 0, width: r.width)
        }
    }
}
