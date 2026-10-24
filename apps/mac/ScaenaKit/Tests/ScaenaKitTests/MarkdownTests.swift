import ScaenaKit
import Testing

/// The assistant's answer split into its blocks (narrative ATTR-4), as the browser's
/// `markdown.ts` splits it: the cases are `web/markdown.mjs`'s, so the two read alike.
@Test func anAnswerSplitsIntoItsBlocksAsTheBrowsersDoes() {
    let listed = "Three things stand out:\n\n- The **cover** is clean.\n- `revenue` runs long.\n  - its figure wraps\n- The close claims nothing.\n\nFix the second first."
    #expect(
        MarkdownBlocks.blocks(of: listed) == [
            .paragraph("Three things stand out:"),
            .list(
                ordered: false,
                items: [
                    .init(level: 0, text: "The **cover** is clean."), .init(level: 0, text: "`revenue` runs long."),
                    .init(level: 1, text: "its figure wraps"), .init(level: 0, text: "The close claims nothing."),
                ]),
            .paragraph("Fix the second first."),
        ])
    let changed = "## What I changed\n1. Tightened the headline\n2. Moved the note\n\n```json\n[{ \"op\": \"set_text\" }]\n```\nDone."
    #expect(
        MarkdownBlocks.blocks(of: changed) == [
            .heading(level: 2, "What I changed"),
            .list(ordered: true, items: [.init(level: 0, text: "Tightened the headline"), .init(level: 0, text: "Moved the note")]),
            .code("[{ \"op\": \"set_text\" }]", language: "json"),
            .paragraph("Done."),
        ])
    #expect(
        MarkdownBlocks.blocks(of: "A line\nand the next.\r\n\r\nAnother.") == [
            .paragraph("A line\nand the next."), .paragraph("Another."),
        ])
    #expect(MarkdownBlocks.blocks(of: "```\nunclosed") == [.code("unclosed", language: nil)])
    #expect(MarkdownBlocks.blocks(of: "") == [])
}
