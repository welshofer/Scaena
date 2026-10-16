// The word tables the code scanners read (PLAN 2.132), which the editor's module leaves out, as it
// leaves out the hyphenation patterns (ADR-0015): the build copies each file beside the pages, and
// the worker fetches one the first time a code text in a language that reads it lays out. A
// single-file export's worker is built with `no-code-tables.ts`: its module, the player's, has
// every table compiled in.
import clike from "../../crates/scaena-core/code/clike.txt?url";
import css from "../../crates/scaena-core/code/css.txt?url";
import dockerfile from "../../crates/scaena-core/code/dockerfile.txt?url";
import graphql from "../../crates/scaena-core/code/graphql.txt?url";
import hcl from "../../crates/scaena-core/code/hcl.txt?url";
import makefile from "../../crates/scaena-core/code/makefile.txt?url";
import powershell from "../../crates/scaena-core/code/powershell.txt?url";
import python from "../../crates/scaena-core/code/python.txt?url";
import ruby from "../../crates/scaena-core/code/ruby.txt?url";
import shell from "../../crates/scaena-core/code/shell.txt?url";
import sql from "../../crates/scaena-core/code/sql.txt?url";

/** Each file of tables by its name (`clike`), and where the page has it. */
export const codeTables: Record<string, string> = {
  clike, css, dockerfile, graphql, hcl, makefile, powershell, python, ruby, shell, sql,
};
