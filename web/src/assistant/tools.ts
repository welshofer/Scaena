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

/** `resource_read`: the MCP server's resources, the skills a bundle carries, and its theme. */
export const resourceRead: Tool = {
  name: "resource_read",
  description:
    "Read a resource, as the MCP server serves it: the schemas (scaena://schema/deck, scaena://schema/patch, …), " +
    "the lint catalog (scaena://lint/catalog), the specification by section (scaena://spec is its index), " +
    "the skills (scaena://skills/author-deck, …), and examples; a skill the bundle carries (bundle://skills/NAME); " +
    "and the theme the deck is drawn in, as theme_edit edits it (bundle://theme).",
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

/** Whether a call did what the conversation heads it as having done (`done`): not where it
 * stopped, nor where an edit was refused or only tried, which it heads as not done.
 * `scaena_chat::tools::made` says the same for the Mac. */
export function made(name: string, json: string, error: boolean): boolean {
  if (error) return false;
  let r: any;
  try {
    r = JSON.parse(json);
  } catch {
    return true;
  }
  switch (name) {
    case "deck_patch":
    case "spine_update":
    case "theme_apply":
      return Boolean(r?.applied);
    case "theme_edit":
      return Boolean(r?.applied) && !r?.refused;
    case "data_attach":
      return Boolean(r?.attached);
    case "data_edit":
      return Boolean(r?.edited);
    default:
      return true;
  }
}

/** A line saying what a tool's result came to, for the page to show beside the call, in a
 * person's words (PLAN 2.99): what lint finds counted, never its codes. `scaena_chat::tools::summary`
 * says the same for the Mac. */
export function summary(name: string, json: string, error: boolean): string {
  // A resource is its text, not JSON.
  if (name === "resource_read" && !error) return `${json.length} characters`;
  let r: any;
  try {
    r = JSON.parse(json);
  } catch {
    return json.slice(0, 200);
  }
  if (error) return `Stopped: ${r.message ?? json}`;
  const many = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
  const delta = (p = r) => {
    const [added, removed] = [p.added?.length ?? 0, p.removed?.length ?? 0];
    const parts = [];
    if (added) parts.push(`${added} new ${added === 1 ? "issue" : "issues"}`);
    if (removed) parts.push(added ? `${removed} gone` : `${many(removed, "issue")} gone`);
    return parts.length ? ` (${parts.join(", ")})` : "";
  };
  switch (name) {
    case "deck_patch":
    case "spine_update":
      return (r.applied ? "Done" : r.added?.length ? "Refused" : "Tried, not made") + delta();
    case "deck_find": {
      const texts = r.found?.length ?? 0;
      const found = `${r.matches ?? 0} match${r.matches === 1 ? "" : "es"} in ${texts} text${texts === 1 ? "" : "s"}`;
      const p = r.replaced;
      if (!p) return found;
      return `${found}; ${p.applied ? "replaced" : p.added?.length ? "refused" : "tried, not replaced"}${delta(p)}`;
    }
    case "data_attach":
      return (r.attached ? `Added ${r.source}: ${r.rows} rows` : "Refused") + delta();
    case "data_edit": {
      const rows = r.sheet?.rows?.length ?? 0;
      const sheet = `${r.source}, ${rows} row${rows === 1 ? "" : "s"}`;
      if (r.edited) return `Changed ${sheet}${delta()}`;
      if (r.refused) return `Refused${delta()}`;
      return r.added?.length || r.removed?.length ? `Tried, not written${delta()}` : `Read ${sheet}`;
    }
    case "theme_edit":
      if (r.refused) return `Refused${delta()}`;
      return `${r.applied ? "Changed" : "Tried changing"} ${r.theme}, ${many(r.paths?.length ?? 0, "value")}${delta()}`;
    case "deck_lint": {
      const fixed = r.fixed?.length ? `Fixed ${many(r.fixed.length, "issue")}; ` : "";
      const said: Record<string, string> = { error: "problem", warning: "warning", info: "note" };
      const counts = ["error", "warning", "info"].map((s) => [s, (r.findings ?? []).filter((f: { severity: string }) => f.severity === s).length] as const);
      const found = counts.filter(([, n]) => n).map(([s, n]) => `${n} ${said[s]}${n > 1 ? "s" : ""}`).join(", ");
      return fixed + (found || (fixed ? "no issues" : "No issues"));
    }
    case "deck_render":
      return `${r.size?.join("×")} px`;
    case "deck_read":
      return r.scn !== undefined ? `The deck, ${r.scn.split("\n").length} lines` : "The deck";
    case "deck_inspect": {
      const n = r.states?.length ?? 0;
      return n === 1 ? "1 slide" : `${n} slides and steps`;
    }
    case "deck_diff": {
      const n = Object.keys(r.changes ?? {}).length;
      return n === 1 ? "1 object changes" : `${n} objects change`;
    }
    default:
      return "Done";
  }
}
