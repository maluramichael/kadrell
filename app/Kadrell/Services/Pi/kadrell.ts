/**
 * Kadrell-Anbindung für Pi. Kadrell legt diese Datei an, bevor es eine Pi-Session startet, und hält sie aktuell.
 *
 * - Stellt das lokale Modell bereit, das Kadrell der Session mitgibt (Provider „kadrell“, OpenAI-kompatibel).
 * - Meldet den Status der Session an ihre Kachel (`kadrell status …`), wie die Hooks bei Claude Code.
 *
 * Außerhalb einer Pi-Kachel von Kadrell (ohne `KADRELL_PI`) tut sie nichts. Änderungen hier gehen beim nächsten Start
 * einer Pi-Session verloren, die Quelle liegt im Kadrell-Repo unter `app/Kadrell/Services/Pi/kadrell.ts`.
 */
import { execFile } from "node:child_process";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const env = process.env;

/** Eine Zeile, höchstens 200 Zeichen: so zeigt Kadrell die erste Eingabe als Titel der Kachel. */
function flat(text: string): string {
	return text.replace(/\s+/g, " ").trim().slice(0, 200);
}

export default function (pi: ExtensionAPI) {
	if (!env.KADRELL_PI || !env.KADRELL || !env.KADRELL_SOCKET) return;

	if (env.KADRELL_PI_BASE_URL && env.KADRELL_PI_MODEL) {
		pi.registerProvider("kadrell", {
			name: "Kadrell",
			baseUrl: env.KADRELL_PI_BASE_URL,
			apiKey: "kadrell",
			api: "openai-completions",
			models: [
				{
					id: env.KADRELL_PI_MODEL,
					name: env.KADRELL_PI_MODEL,
					reasoning: true,
					input: ["text"],
					cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
					contextWindow: 131072,
					maxTokens: 32768,
				},
			],
		});
	}

	// Ohne Warten und ohne Fehler: die Meldung darf Pi weder bremsen noch stören. Die Aufrufe laufen nacheinander,
	// sonst überholt ein spätes „idle“ das folgende „working“.
	let state = "idle";
	let queue = Promise.resolve();
	const status = (next: string, ...args: string[]) => {
		state = next;
		queue = queue.then(() => new Promise<void>((done) => execFile(env.KADRELL!, ["status", next, ...args], () => done())));
	};
	let firstPrompt = true;

	pi.on("session_start", () => status("idle"));
	pi.on("session_info_changed", (event) => {
		if (event.name) status(state, "--title", flat(event.name));
	});
	pi.on("input", (event) => {
		if (firstPrompt && event.text.trim()) {
			firstPrompt = false;
			status("working", "--first-prompt", flat(event.text));
		}
		return { action: "continue" };
	});
	pi.on("agent_start", () => status("working"));
	pi.on("ui_prompt_start", (event) => status("waiting", ...(event.title ? ["--waiting-for", flat(event.title)] : [])));
	pi.on("ui_prompt_end", () => status("working"));
	pi.on("agent_settled", () => status("idle"));
}
