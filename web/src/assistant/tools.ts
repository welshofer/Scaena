// The assistant's tools (PLAN 2.6): the MCP server's, as `docs/schema/mcp/` says each one is
// called (ADR-0009), less what names a place on disk (`bundle`, `out`) and the painter, since
// they work on the bundle the page holds; and `resource_read`, which reads what the server
// serves as resources.
import type { Tool } from "./providers";

interface McpTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown> & { properties?: Record<string, unknown>; required?: string[] };
}

const mcp = import.meta.glob("../../../docs/schema/mcp/*.json", { eager: true, import: "default" }) as Record<string, McpTool>;

/** What a page's tool does not take: the session is the bundle, and the CPU painter draws. */
const DROPPED = ["bundle", "out", "painter"];

/** `resource_read`: the MCP server's resources, and the skills a bundle carries. */
export const resourceRead: Tool = {
  name: "resource_read",
  description:
    "Read a resource, as the MCP server serves it: the schemas (scaena://schema/deck, scaena://schema/patch, …), " +
    "the lint catalog (scaena://lint/catalog), the specification by section (scaena://spec is its index), " +
    "the skills (scaena://skills/author-deck, …), and examples; and a skill the bundle carries (bundle://skills/NAME).",
  schema: {
    type: "object",
    properties: { uri: { type: "string", description: "The resource's uri." } },
    required: ["uri"],
    additionalProperties: false,
  },
};

/** The tools named `names`, as their MCP tools take them, less what the page does not take;
 * then `resource_read`. */
export function tools(names: string[]): Tool[] {
  const byName = new Map(Object.values(mcp).map((t) => [t.name, t]));
  return [
    ...names.map((name) => {
      const tool = byName.get(name);
      if (!tool) throw new Error(`docs/schema/mcp has no ${name}`);
      const schema = structuredClone(tool.inputSchema);
      for (const drop of DROPPED) delete schema.properties?.[drop];
      if (schema.required) schema.required = schema.required.filter((r) => !DROPPED.includes(r));
      return { name, description: tool.description, schema };
    }),
    resourceRead,
  ];
}

/** A line saying what a tool's result came to, for the page to show beside the call. */
export function summary(name: string, json: string, error: boolean): string {
  // A resource is its text, not JSON.
  if (name === "resource_read" && !error) return `${json.length} characters`;
  let r: any;
  try {
    r = JSON.parse(json);
  } catch {
    return json.slice(0, 200);
  }
  if (error) return `stopped: ${r.message ?? json}`;
  const codes = (findings: { code: string }[] | undefined) => [...new Set((findings ?? []).map((f) => f.code))].join(", ");
  const delta = (p = r) => {
    const parts = [];
    if (p.added?.length) parts.push(`+${codes(p.added)}`);
    if (p.removed?.length) parts.push(`−${codes(p.removed)}`);
    return parts.length ? ` (${parts.join(" ")})` : "";
  };
  switch (name) {
    case "deck_patch":
    case "spine_update":
      return (r.applied ? "applied" : r.added?.length ? "refused" : "not applied (a dry run)") + delta();
    case "deck_find": {
      const texts = r.found?.length ?? 0;
      const found = `${r.matches ?? 0} match${r.matches === 1 ? "" : "es"} in ${texts} text${texts === 1 ? "" : "s"}`;
      const p = r.replaced;
      if (!p) return found;
      return `${found}; ${p.applied ? "replaced" : p.added?.length ? "refused" : "not replaced (a dry run)"}${delta(p)}`;
    }
    case "data_attach":
      return (r.attached ? `attached ${r.source}: ${r.rows} rows` : "refused") + delta();
    case "deck_lint": {
      const fixed = r.fixed?.length ? `fixed ${codes(r.fixed)}; ` : "";
      const counts = ["error", "warning", "info"].map((s) => [s, (r.findings ?? []).filter((f: { severity: string }) => f.severity === s).length] as const);
      const found = counts.filter(([, n]) => n).map(([s, n]) => `${n} ${s}${n > 1 ? "s" : ""}`).join(", ");
      return fixed + (found || "nothing found");
    }
    case "deck_render":
      return `${r.state}, ${r.size?.join("×")} px`;
    case "deck_read":
      return r.scn !== undefined ? `${r.scn.split("\n").length} lines of .scn` : "deck.json";
    case "deck_inspect":
      return `${r.states?.length ?? 0} state${r.states?.length === 1 ? "" : "s"}`;
    case "deck_diff":
      return `${Object.keys(r.changes ?? {}).length} nodes change`;
    default:
      return "done";
  }
}
