# QA 0.3.4 Editor, library and misc (pass 5 leftovers), macOS pass 6 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 6). Final dev bundle of main `0bcac6a3`, isolated under `/tmp/cmq6`.

| Case | Result | Evidence |
| --- | --- | --- |
| Retype busy-background sheet | PASS (seen) | fixture: a synthetic 900 x 500 PNG of random coloured blocks with white text, imported with `capturemann import`. Retype tool, click on "Hello Busy World": sheet "This area has an image or a control in it, so the fill may show / The new text will sit on a patch that does not match what is around it. You can still go on, or cancel and nothing is stored." with a colour swatch, [Cancel] [Use anyway] (`QA-0.3.4-shots/macos-pass6-retype-busy-sheet.png`). **Cancel**: sheet closed, image unchanged, tool stays on, 0 annotations, no click-through. **Use anyway**: the edit box opened with text "Hello Bus-" (the box found only part of the line on a busy background), size 72, background #73347D "Gradient", "Enter to apply · Esc to cancel". Esc cancelled. The first click on the sheet buttons did not place anything |
| Select text (G) | PASS with UX notes | pass 5's "nothing" was a misread: the tool has no on-canvas selection by design (spec ANN-5: a dragged rectangle, text to the clipboard, a toast). With a real mouse drag over C001 to C002 of the 3000 px fixture the clipboard (set to "MARK" before) became "C001 meadow cobalt quartz thistle orchid marble orchia orchid meadow meadow / harbor cobalt harbor canyon / C002 …" (one OCR slip "orchia") and a Success toast window (440 x 72) opened. Notes: the right panel for the tool says "Select text" with "Settings for new annotations." (wrong, nothing to set); no visible selection rectangle after the drag (not checked during it); the first, smaller drag followed by Cmd+C left the clipboard empty (Cmd+C is not the text copy in this tool) |
| Agents & CLI with an unusable socket | PASS (seen) | app started with `CAPTUREMANN_SOCKET=/proc/none/x/s.sock`: log "The CLI and MCP server can't connect to this app: Read-only file system (os error 30)"; page header badge "MCP offline" and "Could not create the socket folder"; sidebar item tooltip "MCP offline · Could not create the socket folder / CaptureMann could not open its agent endpoint, so agents can't reach it. Open Agents & CLI for details."; an amber line repeats it. The client config snippet still shows the unusable path (`"--socket", "/proc/none/x/s.sock"`) and the Tool permissions list stays editable (UX proposal: say the snippet will not work until fixed) |
| Agent confirm text | defect | the Long Shot confirm says "You set this tool to Ask first in Settings → Agents & CLI" while that page shows Take screenshots on Allow (default config) |
| B1 delete with a paused upload | PASS | see the published-recordings report |
| Library selection by hover tick | PASS (seen) | the tick ("Select" / "Chọn", 28 x 28) appears on hover; two ticks open the bar "N selected · Make a board · Export selected… · Delete · ×"; Cmd+A and modifier clicks (my synthetic events) did not select anything and a plain click opened the editor |
| Board sheet then Cancel | note | after Cancel the selection is cleared, so the next click opens a capture (by design "the selection is done") |
| Tall Long Shot thumbnails | low | cards of 1540 x 8330 images show a heavily blurred crop (text unreadable) while 1540 x 3000 ones are sharp |
| Sidebar tile active and disabled states | NOT RUN | |
| Move area, Blueprint, Zoom Lens, canvas text selection | NOT RUN this pass | covered in pass 5 |

## Verdict

PARTIAL.
