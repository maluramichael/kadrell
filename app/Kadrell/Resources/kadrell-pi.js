// Kadrell-Integration für Pi: meldet Zustand, sessionId, Titel und Modell der Kachel über `kadrell status`.
// Kadrell lädt die Datei per `pi -e <pfad>` nur in den eigenen Kacheln, in Pis Konfiguration steht nichts.
// Ohne KADRELL_SOCKET (Pi läuft außerhalb von Kadrell) tut sie nichts. Geprüft gegen pi 1.2.0 (docs/extensions.md).
import { execFile } from "node:child_process";

const MAX_MESSAGE = 2000;

export default function (pi) {
  const kadrell = process.env.KADRELL;
  if (!process.env.KADRELL_SOCKET || !kadrell) return;

  let state = "idle";
  let busy = false;
  let lastMessage;

  // Nie blockieren: Antwort und Fehler sind egal, die nächste Meldung trägt den aktuellen Stand.
  const report = (next, flags = {}) => {
    state = next;
    const args = ["status", next];
    for (const [key, value] of Object.entries(flags)) {
      if (value !== undefined && value !== null && value !== "") args.push(`--${key}`, String(value));
    }
    execFile(kadrell, args, { timeout: 3000 }, () => {});
  };

  const modelName = (model) => (model ? `${model.provider}/${model.id}` : undefined);

  const assistantText = (messages) => {
    for (let i = (messages ?? []).length - 1; i >= 0; i--) {
      const m = messages[i];
      if (m?.role !== "assistant") continue;
      const text = typeof m.content === "string"
        ? m.content
        : (m.content ?? []).filter((c) => c?.type === "text").map((c) => c.text).join("\n");
      if (text.trim()) return text.trim().slice(0, MAX_MESSAGE);
    }
    return undefined;
  };

  // Auch bei /new, /resume und /fork: die Kachel folgt der neuen Konversation, wie bei Claudes /clear.
  pi.on("session_start", (_event, ctx) => {
    busy = false;
    report("idle", {
      "session-id": ctx.sessionManager.getSessionId(),
      title: ctx.sessionManager.getSessionName(),
      model: modelName(ctx.model),
    });
  });

  pi.on("session_info_changed", (event) => {
    if (event.name) report(state, { title: event.name });
  });

  pi.on("input", (event) => {
    const text = event.text?.trim();
    if (text && !text.startsWith("/")) report(state, { "first-prompt": text.slice(0, MAX_MESSAGE) });
  });

  pi.on("model_select", (event) => report(state, { model: modelName(event.model) }));

  pi.on("agent_start", () => {
    busy = true;
    report("working");
  });

  pi.on("agent_end", (event) => {
    lastMessage = assistantText(event.messages) ?? lastMessage;
  });

  // `agent_end` reicht nicht: Retries, Compaction oder Folgenachrichten können danach noch laufen.
  pi.on("agent_settled", () => {
    busy = false;
    report("idle", { message: lastMessage });
  });

  pi.on("ui_prompt_start", (event) => report("waiting", { "waiting-for": event.title ?? event.kind }));
  pi.on("ui_prompt_end", () => report(busy ? "working" : "idle"));
}
