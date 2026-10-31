import AppKit
import QuartzCore

/// Arbeits-Anzeige: ein Punkt in der Arbeitsfarbe des Schemas, der langsam atmet (Größe, Deckkraft und ein weicher
/// Schein, Zyklus 2,8 s, sanft ein- und ausschwingend). Er läuft als Core-Animation im Render-Server: kein Takt, kein
/// Neuzeichnen der View. Alle Punkte atmen im selben Takt (die Phase hängt an der Uhr, nicht am Erscheinen), damit
/// viele arbeitende Sessions als ruhige Fläche wirken statt als Flackern.
///
/// Abgrenzung zu den Vorgängern: Der Punkt bis 1.70 blinkte per Timer-Neuzeichnen alle 1,2 s zwischen 30 und 100 %
/// Deckkraft (hektisch, und ohne Bewegung nur über die Farbe vom fertigen Punkt zu unterscheiden), die Strich-Anzeige
/// danach lief in 160-ms-Schritten. Hier bleibt die Deckkraft über 70 %, und der Schein ist auch bei „Bewegung
/// reduzieren“ da: dann steht der Punkt still mit festem Schein, die Form unterscheidet ihn weiter vom Statuspunkt.
@MainActor
final class BreathingDots: NSObject {
    /// Ein Atemzug: einatmen und ausatmen, je die Hälfte.
    static let period: CFTimeInterval = 2.8

    /// Zustand eines Punkts: Größe (1 = wie der Statuspunkt), Deckkraft und Stärke des Scheins.
    struct Pose: Equatable {
        var scale: CGFloat, opacity: Float, glow: Float
        /// Ausgeatmet, der ruhigste Punkt des Zyklus.
        static let exhaled = Pose(scale: 0.86, opacity: 0.75, glow: 0.1)
        /// Eingeatmet: volle Größe wie der Statuspunkt, Schein am stärksten.
        static let inhaled = Pose(scale: 1, opacity: 1, glow: 0.75)
        /// „Bewegung reduzieren“: steht still, voll, mit festem Schein.
        static let still = Pose(scale: 1, opacity: 1, glow: 0.55)
    }

    /// Weicher Schein um den Punkt, in logischen Punkten (vor `Theme.scale`).
    static let glowRadius: CGFloat = 3

    /// Ebene unter allen Unter-Views: über dem, was die View selbst zeichnet, unter Terminals und Knöpfen.
    private let container = CALayer()
    private var dots: [String: CAShapeLayer] = [:]
    private var animated = false

    override init() {
        super.init()
        container.actions = Self.noActions
        // Selektor statt Block: die Abmeldung erledigt NotificationCenter, wenn die View (und damit das hier) geht.
        NotificationCenter.default.addObserver(self, selector: #selector(reduceMotionChanged), name: Feedback.reduceMotionChanged, object: nil)
    }

    @objc private func reduceMotionChanged() { applyMotion() }

    /// Ohne implizite Animationen: Lage, Größe und Farbe folgen der View sofort, nur das Atmen bewegt sich.
    private static let noActions: [String: any CAAction] = ["position": NSNull(), "bounds": NSNull(), "frame": NSNull(),
                                                            "path": NSNull(), "fillColor": NSNull(), "shadowColor": NSNull(), "shadowPath": NSNull(),
                                                            "shadowRadius": NSNull(), "opacity": NSNull(), "shadowOpacity": NSNull(),
                                                            "transform": NSNull(), "sublayers": NSNull()]

    /// Punkte je Schlüssel, `rect` in Bounds-Koordinaten der View (schon mit `Theme.scale`). Fehlende verschwinden.
    func sync(_ wanted: [String: (rect: CGRect, color: NSColor)], in view: NSView) {
        guard let host = view.layer else { return }
        if container.superlayer !== host { host.insertSublayer(container, at: 0) }
        container.frame = host.bounds
        for key in dots.keys where wanted[key] == nil { dots.removeValue(forKey: key)?.removeFromSuperlayer() }
        for (key, dot) in wanted {
            let layer = dots[key] ?? makeDot(key)
            place(layer, rect: view.convertToLayer(dot.rect), color: dot.color.cgColor)
        }
        if animated != !Feedback.reduceMotion { applyMotion() }
    }

    /// Für Tests: Lage (ohne Atem-Skalierung) und Farbe der Punkte in Layer-Koordinaten.
    var frames: [String: CGRect] {
        dots.mapValues { l in CGRect(x: l.position.x - l.bounds.width / 2, y: l.position.y - l.bounds.height / 2, width: l.bounds.width, height: l.bounds.height) }
    }
    func color(_ key: String) -> CGColor? { dots[key]?.fillColor }
    func isBreathing(_ key: String) -> Bool { dots[key]?.animation(forKey: "breath") != nil }

    private func makeDot(_ key: String) -> CAShapeLayer {
        let layer = CAShapeLayer()
        layer.actions = Self.noActions
        layer.shadowOffset = .zero
        dots[key] = layer
        container.addSublayer(layer)
        Self.apply(Feedback.reduceMotion ? .still : .inhaled, to: layer)
        if animated { Self.breathe(layer) }
        return layer
    }

    /// Über `bounds` und `position` statt `frame`: der Atem skaliert die Ebene, `frame` wäre dann nicht mehr eindeutig.
    private func place(_ layer: CAShapeLayer, rect: CGRect, color: CGColor) {
        let bounds = CGRect(origin: .zero, size: rect.size), center = CGPoint(x: rect.midX, y: rect.midY)
        guard layer.bounds != bounds || layer.position != center || layer.fillColor != color else { return }
        layer.bounds = bounds
        layer.position = center
        layer.path = CGPath(ellipseIn: bounds, transform: nil)
        layer.shadowPath = layer.path
        layer.fillColor = color
        layer.shadowColor = color
        layer.shadowRadius = Self.glowRadius * Theme.scale
    }

    /// „Bewegung reduzieren“ an: alle Punkte stehen still; aus: alle atmen, im gemeinsamen Takt.
    private func applyMotion() {
        animated = !Feedback.reduceMotion
        for layer in dots.values {
            layer.removeAnimation(forKey: "breath")
            Self.apply(animated ? .inhaled : .still, to: layer)
            if animated { Self.breathe(layer) }
        }
    }

    static func apply(_ pose: Pose, to layer: CALayer) {
        layer.transform = CATransform3DMakeScale(pose.scale, pose.scale, 1)
        layer.opacity = pose.opacity
        layer.shadowOpacity = pose.glow
    }

    /// Aus- und wieder einatmen, sanft an beiden Enden. Der Beginn liegt auf einem Vielfachen von `period` der
    /// Medienzeit, so sind alle Punkte in Phase, auch die später erschienenen.
    private static func breathe(_ layer: CALayer) {
        func part(_ key: String, _ from: Any, _ to: Any) -> CABasicAnimation {
            let anim = CABasicAnimation(keyPath: key)
            anim.fromValue = from
            anim.toValue = to
            return anim
        }
        let a = Pose.inhaled, b = Pose.exhaled
        let group = CAAnimationGroup()
        group.animations = [part("transform.scale", a.scale, b.scale), part("opacity", a.opacity, b.opacity), part("shadowOpacity", a.glow, b.glow)]
        group.duration = period / 2
        group.autoreverses = true
        group.repeatCount = .infinity
        group.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        let now = CACurrentMediaTime()
        group.beginTime = layer.convertTime(now, from: nil) - now.truncatingRemainder(dividingBy: period)
        group.isRemovedOnCompletion = false
        layer.add(group, forKey: "breath")
    }

    /// Zustand zum Zeitpunkt `t` (Sekunden Medienzeit), wie die Animation ihn zeigt (Kurve angenähert), für die
    /// Sichtprüfung einzelner Bilder des Zyklus.
    static func pose(at t: CFTimeInterval) -> Pose {
        let phase = t.truncatingRemainder(dividingBy: period) / period
        let half = phase < 0.5 ? phase * 2 : 2 - phase * 2
        let p = Float(half * half * (3 - 2 * half))
        let a = Pose.inhaled, b = Pose.exhaled
        return Pose(scale: a.scale + (b.scale - a.scale) * CGFloat(p), opacity: a.opacity + (b.opacity - a.opacity) * p,
                    glow: a.glow + (b.glow - a.glow) * p)
    }
}
