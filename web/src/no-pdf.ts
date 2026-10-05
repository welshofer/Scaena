// The PDF painter a single-file export's worker is built with (PLAN 2.5): none. Its page plays a
// bundle and never exports one, which is what the PDF's module is for (PLAN 2.54).
const none = (): never => {
  throw new Error("a single-file export does not draw PDFs");
};
export default none;
export const pdf = none;
