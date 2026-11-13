# Desktop setup reference

Generated November 13, 2026 for the compact server/wake/permissions redesign.
`desktop-setup-reference.png` is the visual target; its example statuses are not
measurements or acceptance evidence.

Prompt direction: polished native dark glass desktop setup panel, subtle blur,
24 px outer radius, blue action accent, clear sans-serif hierarchy. Laptop-led
header, computer summary strip, four rounded accordion rows (Computer & server,
Network access, Firewall, Wake this computer), only Network expanded with detected
address/router facts and concise next actions. Advanced details collapsed. Fixed
footer with connection-scope hint and Done. Preserve legibility, real icons,
keyboard use, high contrast and reduced motion. No walls of technical copy.

Implementation: shared SetupSection disclosure and SetupGlassDialog, actual
runtime facts substituted for illustrative network/readiness text. Native
screenshots and release acceptance are recorded outside Git in turn221 evidence.
