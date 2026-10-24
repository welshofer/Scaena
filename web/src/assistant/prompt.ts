// What the assistant is told before the user's first word (PLAN 2.6, SPEC §11): what Scaena
// is, what its tools do here, the deck it works on, what it can read, and the author-deck
// skill, whose prompts come from `skills/` as SPEC §11 says; and what the editor shows as a
// question is asked, which begins it.
import type { Seeing } from "../protocol";

/** A resource the model can read, as `resources/list` names it. */
export interface Listed {
  uri: string;
  name: string;
}

/** The open deck, in a few facts; and its story, where it has one (narrative FUNC-2): the brief
 * it was made from, the arc it was written to, and each beat's claim in spine order, so what the
 * assistant does is aimed at the story the user asked for, not at lint alone. */
export interface Deck {
  title?: string;
  states: string[];
  formats: string[];
  theme?: string;
  brief?: Brief;
  arc?: string;
  claims?: string[];
}

/** The brief a deck was made from, as `meta.narrative.brief` keeps it (ADR-0026), the whole of
 * it (narrative FUNC-4). */
export interface Brief {
  topic?: string;
  audience?: string;
  goal?: string;
  length?: string;
  modality?: string;
  desiredOutcomes?: string;
  voice?: string;
}

/** The deck's story, as the prompt tells it: nothing where the deck has none. */
function story(deck: Deck): string {
  const about: string[] = [];
  if (deck.brief?.topic) about.push(`on ${JSON.stringify(deck.brief.topic)}`);
  if (deck.brief?.audience) about.push(`for ${deck.brief.audience}`);
  if (deck.brief?.goal) about.push(`to ${deck.brief.goal}`);
  if (deck.brief?.length) about.push(`in ${deck.brief.length}`);
  if (deck.arc) about.push(`written to the ${deck.arc} arc`);
  // How it is delivered, what it is to achieve, and its voice, each a sentence of its own.
  const labelled: [string, string | undefined][] = [
    ["Delivered", deck.brief?.modality],
    ["What it is to achieve", deck.brief?.desiredOutcomes],
    ["Its voice", deck.brief?.voice],
  ];
  const said = labelled.flatMap(([label, v]) => (v ? [`${label}: ${v.replace(/\.+$/, "")}.`] : []));
  const claims = deck.claims ?? [];
  if (!about.length && !said.length && !claims.length) return "";
  let told = "\n\nIts story";
  if (about.length) told += `: ${about.join(", ")}`;
  if (said.length) {
    told += `.${said.map((s) => ` ${s}`).join("")}`;
    if (claims.length) told += ` Its beats' claims, in order:${claims.map((c, i) => `\n${i + 1}. ${c}`).join("")}`;
    return told;
  }
  if (!claims.length) told += ".";
  else told += `${about.length ? ". Its beats' claims, in order:" : ", by its beats' claims, in order:"}${claims.map((c, i) => `\n${i + 1}. ${c}`).join("")}`;
  return told;
}

export function system(deck: Deck, resources: Listed[], bundleSkills: string[], authorDeck: string): string {
  const skills = bundleSkills.length
    ? `\nThe bundle carries its own skills too; follow them over the general ones where they differ:\n${bundleSkills.map((s) => `- bundle://skills/${s}`).join("\n")}\n`
    : "";
  return `You are the assistant in Scaena's editor, working with the user on the deck they have open.

Scaena decks are states over one scene graph: nodes exist for the whole deck, each state says what changes, and the theme owns type and layout, so a deck names roles, slots, and presets, never pixels.

Your tools are the Scaena MCP server's, working on the open deck: they take no \`bundle\`, \`out\`, or \`painter\`. Each edit you make (deck_patch, spine_update, state_start, background_set, data_attach, data_edit, theme_edit, deck_lint with \`fix\`) shows in the user's editor as you make it, and they can undo it. Edit with deck_patch (\`dry_run\` first when unsure), check with deck_lint, read the story with deck_health, which scores how the slides pace, balance, and follow their arc and says what to look at on each, and look with deck_render, which returns the frame as an image. A file the user dropped on the editor is in the bundle, data_attach declares a data file the bundle holds, and data_edit reads a data source's rows and edits them in place: a cell set, a row added or taken away. theme_edit edits the deck's own theme, its colors, type roles, and spacing, by JSON Patch operations on it (resource_read of bundle://theme reads it as it is, and scaena://schema/theme says what it takes): a look asked of the whole deck, such as larger headlines or a warmer accent, is an edit of the theme, never a literal written into each node. There is no deck_create, deck_generate, theme_apply, deck_export, or deck_history here: the page opens and downloads bundles itself, and its Versions tab reads and restores the deck's history, so ask the user when the work needs a new bundle, another theme, an export, or an earlier version.

resource_read reads the resources: the schemas, the lint catalog, the specification by section (scaena://spec is its index), the skills, and examples. The patch ops are in scaena://schema/patch. Follow the author-deck skill below, and read the others when the work calls for them.

A question may begin with what the user sees in the editor, in brackets: the state shown, the nodes selected, and any characters selected in a text, counted as replace_text and style_text count them, with the text they make. "This", "it", "these", "here", and words like "shorter" or "bolder" mean what it names: change those, in that state, unless the question says otherwise. What you change is selected in the editor after.

Say briefly what you do and what you found, in the user's language. Lint to zero errors before you say a deck is done, and look at what you changed. Before you say a deck is done, read its health with deck_health and leave each slide saying its beat's claim.

The deck: ${deck.title ? `"${deck.title}", ` : ""}${deck.states.length} state${deck.states.length === 1 ? "" : "s"} (${deck.states.join(", ")})${deck.theme ? `, theme ${deck.theme}` : ""}${deck.formats.length ? `, also in ${deck.formats.join(", ")}` : ""}.${story(deck)}

The resources:
${resources.map((r) => `- ${r.uri}: ${r.name}`).join("\n")}
${skills}
--- scaena://skills/author-deck ---
${authorDeck}`;
}

/** What the editor shows as a question is asked (PLAN 2.52), as the question's first line, in
 * brackets: the system prompt says how to read it. The conversation keeps it with the question,
 * so what was selected then stays said. */
export function seen(seeing: Seeing | undefined): string {
  if (!seeing) return "";
  const format = seeing.format ? ` in ${seeing.format}` : "";
  const nodes = seeing.nodes.length
    ? `selected: ${seeing.nodes.map((n) => (n.type ? `${n.node} (${n.type})` : n.node)).join(", ")}`
    : "nothing selected";
  const c = seeing.characters;
  const characters = c ? `; in ${c.node}, characters ${c.from} to ${c.to} selected: ${JSON.stringify(c.text)}` : "";
  return `[In the editor: state ${seeing.state} shown${format}; ${nodes}${characters}.]\n\n`;
}
