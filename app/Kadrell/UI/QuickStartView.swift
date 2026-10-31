import SwiftUI

/// Zustand des Schnellstarts (⌘⇧I): erst die Idee, dann das erkannte Projekt zur Bestätigung. Esc und ⏎ kommen vom
/// OverlayPanel, deshalb liegen `submit()` und `cancel()` am Modell.
@Observable
@MainActor
final class QuickStartModel {
    enum Phase { case idea, thinking, pick }

    private(set) var candidates: [QuickStartCandidate]
    var idea = "" { didSet { if idea != oldValue { ideaChanged() } } }
    /// Namenssuche während des Tippens, für den Hinweis unter dem Feld.
    private(set) var live = QuickStartMatch(ranked: [])
    var filter = "" { didSet { if filter != oldValue { updateList() } } }
    private(set) var phase = Phase.idea
    private(set) var list: [QuickStartCandidate] = []
    var selected = 0
    /// Woher die Vorauswahl kommt, eine Zeile über der Liste.
    private(set) var origin = ""
    /// Vom Modell bereinigter Prompt (Projektverweis entfernt), nur wenn `QuickStartMatcher.acceptCleaned` ihn durchlässt.
    private(set) var cleaned: String?
    var useCleaned = true
    private(set) var error: String?
    /// Anzeigename des Modells der Funktion, nil = Schnellstart ohne Modell (nur Namenssuche).
    let modelName: String?

    @ObservationIgnored private var ranked: [QuickStartCandidate] = []
    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private let ask: ((String, String) async throws -> String)?
    @ObservationIgnored var onStart: ((QuickStartCandidate, String) -> Void)?
    @ObservationIgnored var onClose: (() -> Void)?

    /// `ask` schickt System- und Nutzertext an das Modell der Funktion; nil, wenn sie aus ist oder kein Modell hat.
    init(candidates: [QuickStartCandidate], modelName: String?, ask: ((String, String) async throws -> String)?) {
        self.candidates = candidates
        self.modelName = modelName
        self.ask = ask
    }

    var prompt: String { (useCleaned ? cleaned : nil) ?? idea.trimmingCharacters(in: .whitespacesAndNewlines) }

    /// Neuer Stand aus dem Index (Repo-Scan fertig). Eine schon gezeigte Auswahl bleibt, wie sie ist.
    func updateCandidates(_ c: [QuickStartCandidate]) {
        candidates = c
        if phase == .idea { live = QuickStartMatcher.match(idea, in: candidates) }
    }

    func submit() {
        switch phase {
        case .idea: analyze()
        case .thinking: skipModel()
        case .pick: start()
        }
    }

    /// Esc: aus der Projektwahl zurück zur Idee, beim Warten aufs Modell ebenso, sonst zu.
    func cancel() {
        switch phase {
        case .idea: onClose?()
        case .thinking, .pick: backToIdea()
        }
    }

    private func backToIdea() {
        task?.cancel()
        task = nil
        phase = .idea
        filter = ""
        error = nil
    }

    private func ideaChanged() {
        if phase == .thinking { backToIdea() }
        live = QuickStartMatcher.match(idea, in: candidates)
    }

    /// Erst die Namenssuche. Das Modell fragt Kadrell nur, wenn es eingeschaltet ist und entweder der Name nicht
    /// eindeutig ist oder es den Projektverweis aus dem Prompt nehmen soll.
    func analyze() {
        let text = idea.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { NSSound.beep(); return }
        let m = QuickStartMatcher.match(text, in: candidates)
        ranked = m.ranked
        cleaned = nil
        useCleaned = true
        error = nil
        guard let ask, !candidates.isEmpty, !m.confident || LocalModelSettings.quickStartStripReference else { finish(m, answer: nil, failure: nil); return }
        phase = .thinking
        let shortlist = Array(m.ranked.prefix(QuickStartPrompt.limit))
        let user = QuickStartPrompt.user(idea: text, candidates: shortlist, nameMatch: m.topStrength > 0 ? 0 : nil)
        task = Task { [weak self] in
            do {
                let reply = try await ask(QuickStartPrompt.system, user)
                guard !Task.isCancelled else { return }
                let answer = QuickStartPrompt.parse(reply)
                self?.finish(m, answer: answer, failure: answer == nil ? String(localized: "Antwort nicht lesbar", bundle: Bundle.app) : nil)
            } catch {
                guard !Task.isCancelled else { return }
                self?.finish(m, answer: nil, failure: LocalModelError.describe(error))
            }
        }
    }

    /// ⏎ beim Warten: nicht länger aufs Modell warten, die Namenssuche entscheidet.
    private func skipModel() {
        task?.cancel()
        task = nil
        finish(QuickStartMatcher.match(idea, in: candidates), answer: nil, failure: String(localized: "übersprungen", bundle: Bundle.app))
    }

    private func finish(_ m: QuickStartMatch, answer: QuickStartPrompt.Answer?, failure: String?) {
        ranked = m.ranked
        var pick = m.top
        origin = Self.nameOrigin(m)
        if let a = answer, a.index >= 0, a.index < min(ranked.count, QuickStartPrompt.limit), !m.confident {
            pick = ranked[a.index]
            origin = a.sure ? String(localized: "erkannt von \(modelName ?? "")", bundle: Bundle.app)
                            : String(localized: "\(modelName ?? "") vermutet, bitte prüfen", bundle: Bundle.app)
        } else if let a = answer, a.index < 0, !m.confident {
            origin = String(localized: "\(modelName ?? "") findet kein passendes Projekt · zuletzt benutzte zuerst", bundle: Bundle.app)
        } else if let failure, !m.confident {
            origin += " · " + String(localized: "Modell: \(failure)", bundle: Bundle.app)
        }
        if let a = answer, a.sure, LocalModelSettings.quickStartStripReference, let pick,
           QuickStartMatcher.acceptCleaned(a.prompt, original: idea, candidate: pick) {
            cleaned = a.prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        if let pick, let i = ranked.firstIndex(of: pick) { ranked.insert(ranked.remove(at: i), at: 0) }
        task = nil
        phase = .pick
        filter = ""
        updateList()
    }

    private static func nameOrigin(_ m: QuickStartMatch) -> String {
        switch m.topStrength {
        case 2 where m.confident: String(localized: "erkannt am Namen im Text", bundle: Bundle.app)
        case 2: String(localized: "mehrere Projekte passen zum Namen", bundle: Bundle.app)
        case 1: String(localized: "ähnlicher Name im Text, bitte prüfen", bundle: Bundle.app)
        default: String(localized: "kein Projekt im Text erkannt · zuletzt benutzte zuerst", bundle: Bundle.app)
        }
    }

    /// Tippen in der Projektwahl filtert wie ⌘N über Ordner- und Gruppennamen.
    private func updateList() {
        let q = filter.trimmingCharacters(in: .whitespaces)
        if q.isEmpty {
            list = ranked
        } else {
            var hits: [(candidate: QuickStartCandidate, rank: Int, order: Int)] = []
            for (i, c) in ranked.enumerated() {
                // Nur Treffer im Namen (Rang 0 bis 2): die Buchstabenfolge irgendwo im langen Pfad trifft sonst fast alles.
                if let r = FolderIndex.rank(q, path: c.path + "/" + c.name), r < 3 { hits.append((c, r, i)) }
            }
            hits.sort { $0.rank != $1.rank ? $0.rank < $1.rank : $0.order < $1.order }
            list = hits.map(\.candidate)
        }
        selected = 0
    }

    func move(_ d: Int) { selected = max(0, min(max(list.count - 1, 0), selected + d)) }

    /// Tab in der Projektwahl: zwischen bereinigtem Prompt und Originaltext wechseln.
    func toggleCleaned() {
        guard cleaned != nil else { NSSound.beep(); return }
        useCleaned.toggle()
    }

    func pick(_ i: Int) {
        guard list.indices.contains(i) else { return }
        selected = i
        start()
    }

    func start() {
        guard list.indices.contains(selected) else { NSSound.beep(); return }
        let c = list[selected]
        guard FolderIndex.isDirectory(c.path) else {
            error = String(localized: "\(Theme.shortPath(c.path)) existiert nicht mehr", bundle: Bundle.app)
            NSSound.beep()
            return
        }
        onStart?(c, prompt)
    }
}

/// ⌘⇧I: Idee tippen, ⏎ zeigt das erkannte Projekt, ⏎ startet dort eine Session mit der Idee als erstem Prompt.
struct QuickStartView: View {
    @Bindable var model: QuickStartModel

    var body: some View {
        VStack(spacing: 0) {
            Text("SCHNELLSTART").dialogTitle()
            if model.phase == .pick { pickPhase } else { ideaPhase }
            DialogFoot(hint: hint, button: model.phase == .pick ? String(localized: "Starten", bundle: Bundle.app) : String(localized: "Weiter", bundle: Bundle.app)) { model.submit() }
        }
        .dialogFrame()
    }

    private var hint: String {
        switch model.phase {
        case .idea: String(localized: "⏎ Projekt erkennen · Esc schließen", bundle: Bundle.app)
        case .thinking: String(localized: "⏎ ohne Modell weiter · Esc zurück", bundle: Bundle.app)
        case .pick: String(localized: "⏎ starten · ↑↓ oder tippen wählt ein anderes Projekt · Esc zurück", bundle: Bundle.app)
        }
    }

    private var ideaPhase: some View {
        VStack(alignment: .leading, spacing: 8) {
            PathField(text: $model.idea, placeholder: String(localized: "Idee tippen, z. B. „In Kadrell habe ich die Idee, dass …“", bundle: Bundle.app),
                      size: 14, onTab: {}, onSubmit: model.submit, onMove: { _ in })
            HStack(spacing: 6) {
                if model.phase == .thinking {
                    ProgressView().controlSize(.small)
                    Text(String(localized: "frage \(model.modelName ?? "") …", bundle: Bundle.app)).foregroundStyle(Theme.mutedColor)
                } else {
                    Text(liveText).foregroundStyle(Theme.mutedColor).lineLimit(1).truncationMode(.middle)
                }
                Spacer()
            }
            .font(Theme.ui(11))
        }
        .padding(14)
    }

    private var liveText: String {
        let m = model.live
        if let top = m.top, m.topStrength > 0 { return String(localized: "erkannt: \(top.name) · \(Theme.shortPath(top.path))", bundle: Bundle.app) }
        if let name = model.modelName { return String(localized: "kein Name im Text · \(name) hilft beim Zuordnen", bundle: Bundle.app) }
        return String(localized: "kein Name im Text · ohne lokales Modell zuletzt benutzte zuerst", bundle: Bundle.app)
    }

    private var pickPhase: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text("ERSTER PROMPT").capsLabel()
                    if model.cleaned != nil {
                        Text(model.useCleaned ? String(localized: "Projektverweis entfernt · Tab: Originaltext", bundle: Bundle.app)
                                              : String(localized: "Originaltext · Tab: ohne Projektverweis", bundle: Bundle.app))
                            .font(Theme.ui(10.5)).foregroundStyle(Theme.accentTextColor)
                    }
                }
                Text(model.prompt).foregroundStyle(Theme.fgColor).lineLimit(3).frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.horizontal, 16).padding(.top, 12)
            PathField(text: $model.filter, placeholder: String(localized: "Anderes Projekt suchen", bundle: Bundle.app),
                      onTab: model.toggleCleaned, onSubmit: model.submit, onMove: model.move)
                .padding(14)
            HStack {
                Text(model.error ?? model.origin).font(Theme.ui(10.5))
                    .foregroundStyle(model.error != nil ? Theme.errorTextColor : Theme.mutedColor).lineLimit(1)
                Spacer()
            }
            .padding(.horizontal, 16).padding(.bottom, 6)
            SeamLine()
            list
        }
    }

    private static let rowHeight: CGFloat = 46
    private static let visibleRows: CGFloat = 6

    private var list: some View {
        let items = model.list
        return ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 0) {
                    if items.isEmpty {
                        Text("Kein Projekt gefunden.").foregroundStyle(Theme.mutedColor)
                            .frame(maxWidth: .infinity, alignment: .leading).padding(16)
                    }
                    ForEach(Array(items.enumerated()), id: \.element.id) { i, c in
                        row(c, on: i == model.selected).onTapGesture { model.pick(i) }
                    }
                }
            }
            .frame(height: Self.visibleRows * Self.rowHeight * Theme.scale)
            .onChange(of: model.selected) { _, s in if items.indices.contains(s) { proxy.scrollTo(items[s].id) } }
        }
    }

    private func row(_ c: QuickStartCandidate, on: Bool) -> some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(c.name).foregroundStyle(Theme.fgColor).lineLimit(1)
                Text(Theme.shortPath(c.path)).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1).truncationMode(.head)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                Text(Self.sourceText(c)).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                if c.lastUsed > 0 {
                    Text(Date(timeIntervalSince1970: c.lastUsed), format: .relative(presentation: .named)).font(Theme.ui(10.5)).foregroundStyle(Theme.mutedColor)
                }
            }
        }
        .padding(.vertical, 8).padding(.horizontal, 16)
        .frame(height: Self.rowHeight * Theme.scale)
        .instrumentRow(selected: on)
        .id(c.id)
    }

    private static func sourceText(_ c: QuickStartCandidate) -> String {
        if c.sessions > 0 { return String(localized: "\(c.sessions) Sessions", bundle: Bundle.app) }
        switch c.source {
        case .group: return String(localized: "Gruppe", bundle: Bundle.app)
        case .session, .folder: return String(localized: "benutzter Ordner", bundle: Bundle.app)
        case .repo: return "Git"
        }
    }
}
