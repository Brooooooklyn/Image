# Mobile experience optimization — design

Date: 2026-06-18
Site: image.napi.rs (Void app, React + Tailwind v4, in `website/`)

## Goal

Make the site read and navigate well on phones (360–414px). Scope: fix what is
broken + responsive polish. Not a full mobile redesign.

## Evidence (live dev server, 390px viewport)

```
SITE-WIDE
 🔴 Header nav   logo wraps to 2 lines · links overflow · "GitHub" clipped · no menu

HOME (/)
 🔴 Hero <h1>    "processing" clipped off right edge
                 cause: mobile grid track blows out to ~563px (grid items
                 default min-width:auto + wide code-block child), clipped by
                 the hero section's overflow-hidden
 🟡 Format table overflows viewport by ~50px
 🟡 Code sample  long lines overflow by ~239px
 🟡 Hero stats   grid-cols-3 hardcoded, squeezes <360px
 🟡 Page         slight horizontal scroll (leaked overflow)

DOCS (/docs)
 🟡 Sidebar hidden on mobile -> no in-page doc nav
 ✅ Prose + code read fine

PLAYGROUND (/playground)
 ✅ Stacks cleanly, touch-friendly
 🔴 shares the header issue
```

## Key constraint

The header is server-rendered in `pages/layout.tsx`. `/playground` is a
COEP-isolated island page whose **layout never hydrates** (see GA comment,
`layout.tsx:13-17`). A React-state hamburger would be dead there. Therefore the
drawer must be **CSS-only** so it works on every page without hydration.

## Design

### A. Mobile nav — CSS-only drawer

- Desktop (`md:`+): inline links, unchanged.
- Mobile (`<md`): hamburger button (`md:hidden`) + drawer.
- Mechanism: visually-hidden `<input type="checkbox">` (`peer`) + `<label>`
  hamburger; drawer panel revealed with Tailwind `peer-checked:` variants.
  Zero JS for the toggle → works on `/playground`.
- A tiny inline vanilla script (same pattern as the existing
  `documentElement.classList.add('js')` line) auto-closes the drawer on
  client-side navigation (`@void/react` does SPA nav on hydrating pages).
- Logo gets `whitespace-nowrap` so it never wraps.
- Drawer links sized as ≥40px tap targets.
- a11y: checkbox is focusable + `aria-label` on the control; panel marked up so
  state is conveyed.

### B. Overflow / clipping fixes

| Where | Root cause | Fix |
|---|---|---|
| Hero `<h1>` clipped | grid items `min-width:auto` + wide code child blows mobile track | `min-w-0` on the two hero columns; `min-w-0 overflow-x-auto` on the code block |
| Format table +50px | table wider than viewport | wrap in `overflow-x-auto` scroll container |
| Code sample +239px | long unbroken lines | `min-w-0` + `overflow-x-auto` on the `pre` |
| Page hairline scroll | leaked overflow | `overflow-x: clip` on `body` as backstop, applied **after** the source fixes |

Root-cause first; the `body` guard is a backstop, not the fix (so content is
never silently clipped off-screen).

### C. Responsive polish

- Hero stats: keep `grid-cols-3` but prevent <360px squeeze (tighter gap, and
  step to fewer columns if numbers still collide).
- Docs mobile nav: add a collapsible "Documentation ▾" disclosure at the top of
  docs content on mobile (`md:hidden`), reusing the same sidebar link list.
- Touch targets: nav links + install-switcher tabs to ≥40px tap height.

### D. Verification

Re-run the live browser sweep at **360 / 390 / 414** px on home / docs /
playground / changelog:

1. Assert `document.documentElement.scrollWidth === clientWidth` (zero
   horizontal overflow) on each page/width.
2. Screenshot each page.
3. Confirm drawer opens + closes, including on `/playground`.

Done = evidence (assertions pass + screenshots), not assertion.

## Out of scope

- Full mobile-first redesign of layouts/spacing/type.
- Making the WASM playground tool itself mobile-optimized beyond current
  stacking (already acceptable).
