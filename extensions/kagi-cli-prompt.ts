import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

/** Provides system-prompt guidance for the local Kagi CLI. */
export default function kagiCliPrompt(pi: ExtensionAPI) {
  pi.on("before_agent_start", (event) => ({
    systemPrompt: `${event.systemPrompt}\n\n${KAGI_CLI_GUIDANCE}`,
  }));
}

/** Guidance injected into every agent turn. */
const KAGI_CLI_GUIDANCE = `Web search is available through the local \`kagi\` CLI via \`bash\`.
Use \`kagi search '<query>' --limit N\` for search and \`kagi extract URL...\` for page extraction; output defaults to markdown.
Use \`--format json\` for raw Kagi API JSON suitable for \`jq\`.
Search flags mirror Kagi API fields, including \`--workflow\`, \`--filters.region\`, \`--filters.after\`, \`--filters.before\`, \`--lens.sites_included\`, \`--lens.sites_excluded\`, \`--extract.count\`, and \`--safe_search\`.
Run \`kagi search --help\` or \`kagi extract --help\` when you need less common options.`;
