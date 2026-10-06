// The languages' hyphenation patterns (ADR-0015), which the editor's module leaves out: the
// build copies each file beside the pages, and the worker fetches one the first time a text
// hyphenates in its language. A single-file export's worker is built with `no-hyphenation.ts`:
// its module, the player's, has every language compiled in.
import ca from "../../crates/scaena-engine/hyphenation/ca.bin?url";
import cs from "../../crates/scaena-engine/hyphenation/cs.bin?url";
import da from "../../crates/scaena-engine/hyphenation/da.bin?url";
import de from "../../crates/scaena-engine/hyphenation/de.bin?url";
import el from "../../crates/scaena-engine/hyphenation/el.bin?url";
import en from "../../crates/scaena-engine/hyphenation/en.bin?url";
import es from "../../crates/scaena-engine/hyphenation/es.bin?url";
import fi from "../../crates/scaena-engine/hyphenation/fi.bin?url";
import fr from "../../crates/scaena-engine/hyphenation/fr.bin?url";
import it from "../../crates/scaena-engine/hyphenation/it.bin?url";
import nl from "../../crates/scaena-engine/hyphenation/nl.bin?url";
import pl from "../../crates/scaena-engine/hyphenation/pl.bin?url";
import pt from "../../crates/scaena-engine/hyphenation/pt.bin?url";
import ru from "../../crates/scaena-engine/hyphenation/ru.bin?url";
import sv from "../../crates/scaena-engine/hyphenation/sv.bin?url";
import tr from "../../crates/scaena-engine/hyphenation/tr.bin?url";
import uk from "../../crates/scaena-engine/hyphenation/uk.bin?url";

/** Each language's patterns by its code (`de`), and where the page has them. */
export const hyphenation: Record<string, string> = { en, de, fr, es, it, pt, nl, sv, da, fi, pl, cs, ru, uk, tr, el, ca };
