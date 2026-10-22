import CoreGraphics

/// Was die rechte Seite der Leiste zeigt. Passt sie nicht neben die linken Tasten, fällt in fester Reihenfolge etwas
/// weg: erst die Einträge der Extensions (der linke zuerst), dann fable, 7d, der volle Kontoname, die Update-Taste,
/// die Versionswarnung, der Tipp und 5h. Dann nimmt das Display die Kurzform und wird notfalls mit „…“ gekürzt, Lampe
/// und Uhr bleiben. Reicht selbst das nicht (schmales Fenster bei großer UI), fällt als Letztes die Konto-Taste weg,
/// das Konto bleibt in den Einstellungen wählbar. Linke Tasten, „N warten“ und „N trennen“ bleiben immer.
/// Reine Logik, gemessen und gezeichnet wird in der Leiste.
struct BarPlan: Equatable {
    /// So viele Einträge der Extensions bleiben, von rechts gezählt.
    var extensions: Int
    /// Sichtbare Nutzungswerte nach Kürzel („5h“, „7d“, „fable“).
    var usage: Set<String> = ["5h", "7d", "fable"]
    var shortAccount = false, account = true, update = true, version = true, tip = true
    var compact = false
    /// Um so viel wird der Text im Display gekürzt, wenn selbst die Kurzform nicht passt.
    var shrink: CGFloat = 0

    /// Schritte nach den Einträgen der Extensions, in der Reihenfolge, in der sie Platz schaffen.
    private static let steps: [@Sendable (inout BarPlan) -> Void] = [
        { $0.usage.remove("fable") }, { $0.usage.remove("7d") }, { $0.shortAccount = true }, { $0.update = false },
        { $0.version = false }, { $0.tip = false }, { $0.usage.remove("5h") }, { $0.compact = true },
    ]

    /// Der vollständigste Plan, dessen rechte Seite (`width`) in `room` passt. Reicht auch die Kurzform nicht, kürzt
    /// `shrink` den Displaytext um den Rest.
    static func fit(extensions: Int, room: CGFloat, width: (BarPlan) -> CGFloat) -> BarPlan {
        var plan = BarPlan(extensions: extensions)
        while plan.extensions > 0, width(plan) > room { plan.extensions -= 1 }
        var rest = steps[...]
        while width(plan) > room, let step = rest.popFirst() { step(&plan) }
        plan.shrink = max(0, width(plan) - room)
        guard width(plan) > room else { return plan }
        plan.account = false
        plan.shrink = 0
        plan.shrink = max(0, width(plan) - room)
        return plan
    }
}
