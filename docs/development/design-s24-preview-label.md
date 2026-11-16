# Design S24: Preview label (text extracted from project/R034Preview.dc.html of the design artifact, Nov 16, 2026)

One line per text node; layout is in the artifact (artboard S24). Extracted for engineers; the board wins where they differ.

0.3.4: Preview label
S24 · Preview label
A Preview label for big new features, reusable for the next ones
Long Shot and Click Guide ship in 0.3.4 as previews: ready to use, still changing. One small neutral component marks them wherever people start them, with a popover for feedback. (Nov 16, 2026: the Settings switch below was dropped; a preview is only a label and always available.)
Component
XS · corner tag
Preview
14 px · 9/600 · on tiles and mode buttons
S · next to a label
Preview
18 px · 11/600 · rows, menus, nav
M · in a title
Preview
22 px · 12/600 · flask icon 12
Hover
Preview
border mist, 6 % fill, tooltip
Keyboard focus
Preview
2 px text ring, 2 px gap
Light theme
Preview
Preview
Preview
border #8A8A90 · text #44444B
Preview
Shape:
pill, 1 px
dashed
border, no fill. Dashed reads as “still being built” and keeps it apart from status pills (solid) and the neutral “New in 0.3.4” pill (solid, filled).
Colour:
neutral only: border
#55555C
, text
text-2 #C9C9CF
. Not blue (agent), not red (rec), not amber (warning), not green (success); no new brand colour.
Text:
“Preview”, sentence case, Geist 600, so Vietnamese diacritics stay readable (“Xem trước”). The flask icon only at size M.
Tokens:
preview.border
,
preview.text
,
preview.border-hover #8B8B93
; light:
#8A8A90
and
#44444B
.
Where it goes
Sidebar tiles · XS corner tag
CaptureMann
0.3.4
Record…
Region
Window
Display
Long Shot
Click Guide
LIBRARY
All captures
214
Recordings
61
Audio
8
Screenshots
139
Click Guides
6
INTELLIGENCE
AI models
Agents & CLI
Settings
Preview
Preview
On the tile’s top-right corner, straddling its border, so it never covers the icon or label.
Capture overlay mode bar · XS
Display
Long Shot
Preview
Click Guide
Preview
Above the button’s top-right corner, so the 40 px button and its label keep their size.
Launcher rows · S
long
Long Shot
Preview
New Click Guide
Preview
Capture region
After the title. Search matches the feature name, not the word Preview.
Page title · M
Click Guides
Change the billing plan
Preview
8 steps
Feature pages (Click Guide editor, Long Shot preview panel). One label per page.
Settings rows · S
Long Shot
Preview
Pick a window or area, then scroll
Not set
Set
Shortcuts and permissions rows for the feature.
Native menus · text suffix
New Recording…
New Click Guide (Preview)
Click Guide of a window… (Preview)
Long Shot (Preview)
macOS and Windows menus can’t draw a pill: add “ (Preview)” to the item text.
Hover and click
Preview
Preview
New and still changing. Your files are safe and stay readable. Click to send feedback.
Popover
Long Shot is in preview
Preview
It works and it is ready to use, but how it looks and behaves may still change in the next releases.
Your Long Shots are normal screenshots and stay readable.
Tell us what works and what doesn’t.
Send feedback
Turn off in Settings › Previews
First use (once per feature)
Click Guide
Preview
Click through a task once; each click becomes a numbered step. Clicks only, never keystrokes.
Send feedback
Got it
Settings › General › Previews
Previews
New features that are ready to use but may still change. Turning one off hides its buttons and menus; anything you made with it stays in your library.
Long Shot
Preview
Capture a scrolling page as one image.
Feedback
Click Guide
Preview
Turn your clicks into a numbered guide.
Feedback
Previews are on by default.
Send general feedback
What it means
Ready to use and supported, but its look or behaviour may change in the next releases. Not “beta” and not “experimental”: the data it makes is always kept readable.
Show it on
Entry points (sidebar tile, overlay mode, launcher row, menus, library header button), the feature’s own page title, its settings rows, and agent tool listings.
Never on
HUDs while capturing (they stay minimal), captured or exported images, library cards of things made with the feature, toasts, and inside the editor canvas.
One per surface
At most one Preview label per page, panel or row. Preview replaces “New in 0.3.4”; never show both.
Interaction
Hover shows the tooltip. Click (or Enter on focus) opens the popover with Send feedback (the link to Settings › Previews was dropped with the switch). Accessible name: “Preview. New and still changing.”
First use
The first time a preview feature opens, a small intro card (non-blocking, top right of its surface) explains it in one line, with Got it and Send feedback. Shown once per feature.
Settings › Previews
Dropped on Nov 16, 2026: there is no Previews section and no switch; the previews are always available and keep only the label.
Agents and CLI
MCP tools of a preview feature add
stage: "preview"
to their listing and “(preview)” to their description;
capturemann --help
marks them
[preview]
. (The `feature_disabled` answer was dropped on Nov 16, 2026.)
Lifecycle
Each feature has a stage in one registry (
preview
or
stable
). Moving to stable removes every label at once, in the release notes’ “Now stable” list. Plan: one or two releases in preview.
Feedback
Send feedback opens a GitHub issue form with the feature name and app version filled in (no logs or screenshots attached unless the user adds them).
Text
English “Preview”; Vietnamese draft “Xem trước”. The label is never truncated; in tight spots the XS tag is used instead of S.
0.3.4
Long Shot and Click Guide are Preview. Touch-up, Blueprint, Boards and Bulk export keep “New in 0.3.4”.
