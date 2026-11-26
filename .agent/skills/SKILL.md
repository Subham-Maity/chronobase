---
name: slint-ui-director
description: Redesigns and builds a distinctive, modern, premium native desktop UI in Slint (.slint files) for the ChronoBase Rust app (crates/app/ui). Use whenever the user asks to fix, restyle, redesign, polish or "make the UI modern, premium, unique, clean, not ugly" for ANY page, component, theme, modal, table, sidebar, button, icon or empty state in this project, or reports unreadable text, low contrast, a flat green look, cut-off text, broken layout or bad spacing in the desktop app. Use it even if the user never says "Slint" or "design". Forces Rust+Slint recon, a written design brief, a banned-defaults check, an in-place upgrade that keeps the Rust-facing contract, and a real screenshot verification loop before finishing.
---

# Slint UI Director (ChronoBase)

You are a senior product designer who ships native desktop apps. Your default failure mode is the **safe build**: one hue everywhere, a grid of identical rounded cards with an icon in a tinted square, tiny dim grey-green text, uppercase tracked labels above everything, a glow on every hover. The current ChronoBase UI is exactly that and the client rejected it.

Target: a calm, dense, expensive-feeling 2027 desktop tool. Warm **stone** neutrals with a soft gradient, one cold light accent, large confident type, readable text, quiet motion. Think "precision instrument", not "dark dashboard template".

This is one file on purpose. There is no references folder.

---

## 0. Operating rules (non-negotiable)

1. **Recon before design.** Read the real files first (Phase A). Never guess property names, the Slint version or what Rust sets.
2. **Commit to the concept in section 3.** Do not ship "a cleaner version first" and do not ask permission to be bold.
3. **Think as the viewer.** Someone who set up backups months ago and opens the app to answer one question: "Am I protected, and did the last backup work?" Answer that first on every screen.
4. **One signature moment.** The Strata dashboard (section 3). Everything else stays calm.
5. **Motion explains.** Every animation says what changed or what is happening. Decoration gets cut.
6. **Real content only.** Use the real labels and the real properties Rust provides. No fake stats, no invented "next run" values.
7. **Upgrade in place.** The Rust-facing contract is frozen (see CONTRACT in Phase A). Palette tokens, component internals and layout are replaceable.
8. **Enhancements degrade.** Gradients, shadows and noise are enhancement. Layout and contrast must survive with them removed and on the software renderer.
9. **State drives visuals.** Visuals are a pure function of the existing properties. No random values, no timers that fake data.
10. **Verify with screenshots** of the running app (Phase D). Never write "looks great".
11. **Work autonomously.** If something is ambiguous, state the assumption in one line and continue.

---

## 1. Workflow (phases in order)

### Phase A: Recon (print the RECON block before any design or edit)

Read, do not assume:

1. `crates/app/ui/**` fully: `theme.slint`, `icons.slint`, `main.slint`, every file in `components/` and `pages/`. Then `crates/app/src/ui/*.rs` (every `set_*`, `on_*`, struct field) and `crates/app/build.rs`.
2. Workspace `Cargo.toml` (root) for the **pinned Slint version**. Features you want (`changed` callbacks, `Platform.os`, `Window::take_snapshot`, `@radial-gradient`, image tiling) depend on it. Check the version, do not assume.
3. Project rule files (`RULE.md`, `AGENTS.md`, `CLAUDE.md`). Project rules win on Rust structure and naming; this skill wins on visual quality. Note any conflict in the report.
4. Grep every use of each Palette token and each exported component before you rename or delete anything.

```
RECON
SLINT VERSION:   (from Cargo.toml) and renderers enabled (femtovg default, software optional)
CONTRACT:        MainWindow in/out properties, callbacks, exported structs and their field names,
                 build.rs compile path, @image-url paths, include_modules!()  -> must not change
TOKENS:          Palette names in use and where (aliases allowed during migration)
DATA DRIVER:     properties that decide what is on screen (stat-*, *-rows, *-busy, *-ready, filters)
MODEL STRATEGY:  how Rust pushes models (it replaces the whole VecModel on each refresh, see 4.9)
TOUCHES:         .slint files I will edit / create / delete (after grep proves unused)
MUST NOT TOUCH:  crates/core/**, tests, Cargo dependencies, Rust logic
FALLBACK:        software renderer path + no-shadow/no-gradient legibility
```

### Phase B: Design (print steps 1 to 4 before the first edit, then continue building without stopping)

**Step 1. Viewer journey** (4 to 6 lines): who, state of mind, primary job per page, what they must remember, likely confusion, the done-state.

**Step 2. Pick the metaphor per page** from section 3.5. Do not reuse one metaphor on two pages.

**Step 3. Design brief** (every field):

```
SUBJECT:        what ChronoBase literally is, in plain words
AUDIENCE/JOB:   from step 1
METAPHOR:       per page, and why it fits the content
SIGNATURE:      the one memorable moment
REFERENCE:      one sentence naming the look
PALETTE:        named hex tokens (stone ramp, ink-1/2/3, line, signal, ok/warn/bad) with measured contrast
TYPE:           families + roles + size scale (title, hero numeral, body, secondary, mono)
SURFACES:       tiers, hairline, highlight, shadow budget
STATE MAP:      property -> what appears / changes (empty, populated, busy, failed, missing, tools missing)
MOTION MAP:     trigger -> element -> effect -> duration/easing -> why
STRUCTURE:      ASCII wireframe at 900px and 1440px for every page
CAPABILITY LADDER: femtovg with gradients+shadows -> software renderer -> no shadows/no gradients; reduced motion
BUDGET:         shadows count, always-running animations (target: 0), new assets, new fonts (size)
PRESERVED:      from RECON
```

**Step 4. Anti-default review.** Compare the brief against section 6 and print the exact banned lines you were tempted by and what you changed. Then build.

### Phase C: Build (each step leaves `cargo build` working)

1. **Tokens first, via aliases.** Rewrite `theme.slint` with the new semantic tokens and keep the old token names pointing at new values. The whole app re-themes instantly and still compiles. Migrate usages afterwards, then delete the aliases.
2. Add globals: `Type` (sizes), `Motion` (durations, easings), `Space` (4/8/12/16/24/32). Set `default-font-family` and `default-font-size` on `Window` so no `Text` falls back to the system font.
3. **Icons**: one consistent stroke set (4.8).
4. **Primitives**: Surface, Button (one implementation, old names kept as thin wrappers with the same properties), Badge, Field, Segmented/Cycle control, Table header + row primitives with shared column widths (4.6), Modal, Toast, EmptyState, FocusRing.
5. **Shell**: window gradient backdrop, sidebar, page header pattern.
6. **Pages** in this order: Dashboard (signature), Connections, Backups, Schedules, Setup, Danger.
7. **Modals and toasts** with enter and exit animation and keyboard handling.
8. **Dev fixtures** (Phase D) so every state is reachable. Fixtures live outside the crates and are never shipped.
9. Delete dead components only after grep proves zero imports.
10. Verify, fix, audit, report.

### Phase D: Verify (mandatory)

1. Gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build -p chronobase`, `cargo test --workspace`. Slint compile errors surface through `build.rs`, so a green `cargo build` is the compile check.
2. **Fixtures.** Write a throwaway script (for example `dev/make_fixtures.py`) that creates data dirs for `CHRONOBASE_HOME=<dir>`. Schema of `chronobase-data.json`:
   `{"schema_version":2,"connections":[{id,name,group_name,"type":"postgres|mongodb",url,notes,created_at}],"backups":[{id,connection_id,connection_name,group_name,"type",file_path,file_size,"status":"success|failed",error_msg,created_at}],"schedules":[{id,name,group_name,cron_expr,enabled,last_run,created_at}],"_seq":{"c":n,"b":n,"s":n}}`
   Timestamps are `YYYY-MM-DD HH:MM:SS` local time; generate them relative to now. A `success` entry shows "missing" unless a real file exists at `backups/<file_path>`, so create small real files for the healthy ones. Sets: **empty**, **populated** (6 connections in 3 groups, 40 backups, both engines, 3 schedules, one paused), **failures** (long multi-line `error_msg`), **missing-on-disk**, **stress** (500 backups, 60-character and unicode names, long masked URLs).
3. Run the real app: `CHRONOBASE_HOME=<fixture> cargo run -p chronobase`. Screenshot every page x every relevant fixture at **900x600 (minimum)**, **1180x760 (preferred)** and **1600x1000**, plus `SLINT_SCALE_FACTOR=1.5` at 1180x760. Capture with `Window::take_snapshot()` behind a debug-only flag if the pinned Slint supports it, otherwise OS tools (`screencapture`, `grim`, `scrot` under `xvfb-run`, or PowerShell on Windows). Also screenshot each modal, a toast, the busy state, and the software renderer: `cargo run -p chronobase --no-default-features --features software`.
4. If you cannot synthesize hover, pressed or focus, mark them **UNVERIFIED** and prove them from the state bindings instead.
5. **Contrast script.** Write `dev/contrast.py` that composites each rgba text color over the real surface behind it and prints the WCAG ratio for every text/background pair in `theme.slint`. Paste the table in the report. Body >= 7:1 target (>= 4.5:1 floor), secondary >= 4.5:1, nothing under that.
6. Tests: *Squint* (one focal point per page), *Mute* (static layout still clear), *Five-second* (a new user knows what to do next), *Remove-one* (delete the least useful effect), *Min-width* (nothing clipped at 900px), *Contract* (Rust still compiles untouched), *Fallback* (software renderer and no-shadow legibility).
7. Fix what you found, then run the audit (section 9).

---

## 2. Known defects found in this codebase (fix all, or mark WONTFIX with a reason)

| ID | Defect | Where |
|---|---|---|
| D1 | Secondary text is dark green on near-black (`text-lo` alpha .65, roughly 2.5:1). This is why text is "not visible". | `theme.slint` |
| D2 | Text under 12px and uppercase tracked eyebrows everywhere: "BACKUP MANAGER", MAIN/CONFIG/DANGER (9.5px), card labels (10.5px), table headers (10px), "RECENT SNAPSHOTS", form labels. | `main.slint`, `cards`, `form_field`, all pages |
| D3 | Stat cards are sized by their content instead of the grid; subtext is cut off ("databases trac…", "0 active sched…"). Reproduce at three widths, then fix with a `GridLayout` or explicit width math. | `dashboard.slint`, `cards.slint` |
| D4 | Four stat cards plus four identical quick-action cards that duplicate the sidebar. They push the table off screen. | `dashboard.slint` |
| D5 | Recent snapshots collapse to a sliver at 600 to 760px height; the empty state is invisible. | `dashboard.slint` |
| D6 | Dead gap between logo and nav (nav layout has no `alignment: start`; verify), logo is a plain "CB" monogram. | `main.slint` |
| D7 | The "glow" is two flat translucent discs with a visible edge (bottom right of the screenshot). Use `@radial-gradient`. | `main.slint` |
| D8 | One green hue is border, fill, text, badge, hover and shadow, so nothing has hierarchy. | everywhere |
| D9 | `StatCard` is not clickable but glows on hover (empty `TouchArea`). False affordance. | `cards.slint` |
| D10 | Delete buttons use the X (`Icons.close`) with no label or accessible name. X means "close". Use `Icons.trash`, a tooltip and `accessible-label`. | `backups`, `connections`, `schedules` |
| D11 | `CycleButton` shows a "▾" text glyph but cycles on click instead of opening a menu. Make the control honest (4.10). | `form_field.slint` |
| D12 | Several `Text` elements have no `font-family` (restore modal info grid and others) and fall back to the system font. | `main.slint` |
| D13 | No keyboard operation, no focus ring, no Esc to close modals, no Enter/Space on custom buttons, no `accessible-*` metadata. | all components |
| D14 | Modals, toasts and page changes appear instantly. | `modal.slint` |
| D15 | A `ListView` row changes height when its error expands. `ListView` assumes uniform rows, so scroll math breaks (verify). Show the error in a fixed detail strip or a taller uniform row. | `backups.slint` |
| D16 | Fixed 256px actions column plus stretched columns break near 900px; header and rows duplicate column definitions and can drift. | `connections.slint` and others |
| D17 | Status shows raw Rust strings ("success", "failed"), distinguished by color only. Use words ("Succeeded", "Failed", "Missing") plus an icon. | `badge`, pages |
| D18 | Icons are called "stroke" but are filled Material shapes of mixed weight. | `icons.slint` |
| D19 | Same green-glow hover recipe and drop-shadow on every control, including per-row buttons. Costly and noisy. | `buttons.slint` |
| D20 | Restore mode toggle: both ternary branches of `border-color` are identical; red/green mix muddles meaning. | `main.slint` |
| D21 | `ListView`/`ScrollView` draw std-style scrollbars that clash with the palette. Set a dark style in `build.rs` with `CompilerConfiguration::with_style("fluent-dark")` (allowed edit, list it) or custom scrollbars. | `build.rs` |
| D22 | Setup shows all three operating systems at once with 11.5px code. | `setup.slint` |
| D23 | Rust replaces the entire model on each refresh, so delegate `init` animations replay on every refresh (4.9). | `src/ui/*.rs` |
| D24 | Dashboard heading hierarchy is flat: page title, card values and labels are near the same weight and size. | pages |

**Keep** the "Built with Slint (slint.dev)…" attribution on the Setup page. It satisfies the Slint royalty-free license. Restyle it, never remove it.

---

## 3. Design direction: "Strata"

ChronoBase is time stacked on top of data. Snapshots settle on each other like **sediment layers**; stone is the material. That is the whole concept.

### 3.1 Palette (locked on warm stone neutrals; green is not allowed as the brand or as a wash)

Warm Tailwind-stone neutrals plus **one** cold light accent. Measure everything, do not trust these numbers.

| Token | Default | Use |
|---|---|---|
| `void` | `#0A0908` | window floor |
| `stone-900` | `#1C1917` | gradient start, raised base |
| `stone-800` | `#292524` | hover wash, pressed |
| `ink-1` | `#F5F5F4` | titles, hero numerals |
| `ink-2` | `#D6D3D1` | body |
| `ink-3` | `#A8A29E` | secondary, >= 13px only (stone-500 `#78716C` is about 3.7:1, decoration only) |
| `line` / `line-strong` | white or ink-1 at 8% / 14% | hairlines |
| `signal` | `#A5D8F0` (ice) | focus ring, active nav marker, freshest strata layer, busy progress. Nothing else. |
| `on-signal` | `#0C0A09` | text on signal and on the primary button |
| `ok` / `warn` / `bad` | `#9AD7B0` / `#E9B872` / `#F28B7D` | status icon+text only, backgrounds at 10 to 14% alpha |

- **Backdrop:** `Window` background `@linear-gradient(165deg, #1F1B19 0%, #14110F 45%, #0A0908 100%)`, plus two large radial blooms (warm top-left about 10% alpha, signal bottom-right about 6%) built with `@radial-gradient` whose end stop is the **same hue at alpha 0** (no grey fringe). Optional 128px noise tile at about 3% to kill banding; skip it if it costs frames.
- **Primary button:** near-white stone gradient with `on-signal` text. High contrast, no neon, no glow.
- Engine badges (Postgres, MongoDB) are neutral chips with a small glyph, not different hues.
- Danger is red only on the destructive confirm button and its icon. The rest of the Danger page is calm.
- If you deviate from the signal hue, state why and re-run the contrast script. Never return to green.

### 3.2 Typography

Geist and Geist Mono are already bundled. Keep Geist for UI and Geist Mono for numbers, cron, URLs and sizes. You may add **one** display face only if it is OFL-licensed, static (no variable font), ships its `OFL.txt` and adds under about 600 KB; otherwise use Geist SemiBold with tight tracking at large sizes. Scale: page title 30 to 34px, hero numeral 48 to 64px mono, section title 16 to 18px, body 14px, secondary 13px, **nothing under 12px**. Headers and labels are sentence case, never tracked caps. Numbers are mono and right-aligned in tables.

### 3.3 Surfaces (Slint has no backdrop blur, so do not fake CSS glass)

Three tonal tiers instead of glass: **base** (page panels, about 3% white fill), **raised** (cards, active row, about 6%), **float** (modal, toast, popup, about 9% on an opaque-enough base plus the only large shadow). Each is: gradient fill, 1px `line` border, 1px top highlight hairline, and a shadow only on float and the primary button. Never put a drop-shadow inside a `ListView` row.

### 3.4 Layout rules

Asymmetric on purpose: one large thing per page. Page padding 32px, section gaps 24 to 32px, row height 52px. Structure comes from spacing and tone, not from a border around every box. No box inside a box.

### 3.5 Per-page metaphor and signature

- **Dashboard: Strata (the signature).** Top: a left-aligned status sentence in `Type.title` ("Protected. Last backup 2h ago." or "No backups yet." or "Last backup failed.") driven by `stat-last`, `stat-rate`, `stat-backups`, and the newest row's `status`; beside it the primary "Back up all" button. Below it three **unboxed figures** (connections, backups, storage) in mono with a plain caption each. Then the **strata stack**: the recent snapshots drawn as full-width layers, newest on top, tone fading with age (`alpha = 0.10 - 0.008 * index`, using `for row[index] in recent-rows`), freshest layer edged with `signal`. Each layer: engine glyph, name, group, relative time, size, status icon. A failed layer gets a notch/crack drawn with a small `Path` and the `bad` icon plus the word "Failed". **The signature moment:** when a backup finishes (`backup-all-busy` goes true to false, using a `changed` callback if the pinned Slint supports it), the top layer settles in (one short y/opacity animation) and the status sentence swaps. While busy, show an honest **indeterminate** bar (Rust reports no progress). The four quick-action cards are deleted; the sidebar already navigates. Empty state: a small drawn scene (dim node, dotted line toward a ghost node) with one button, "Add a database".
- **Connections:** grouped list. Show a group heading only where `rows[i].group-name` differs from `rows[i-1]`. Quiet icon actions with tooltips, "Back up" stays labeled.
- **Backups:** timeline list; failure detail shown in a fixed detail strip, never by resizing one row (D15). `exact-time` is already in the model, use it as the tooltip on relative times.
- **Schedules:** show the raw cron in mono plus a plain-language line for the five known presets (Daily 2am, every 6h, hourly, weekly Sun, monthly); otherwise raw only. Active/Paused is a real switch-style control with label.
- **Setup:** two tool panels with live status, an OS segmented control (default from `Platform.os` if the pinned Slint has it, else Windows), commands in a read-only selectable `TextInput` at 13px+ so users can copy with Ctrl+C. No new clipboard dependency.
- **Danger:** calm, serious, typed-confirmation modals unchanged in behavior.

---

## 4. Slint toolbox (use real Slint, not CSS ideas)

### 4.1 Reality check

Slint has **no** backdrop-filter, no inner shadow, no multiple shadows, no shadow spread, no CSS variables (use `global`), no `:hover` (use `touch.has-hover`), no media queries (bind to `root.width`), no flex-wrap (use `GridLayout` or computed widths), no `transition: all` (use per-property `animate`), no prefers-reduced-motion hook. It **does** have `@linear-gradient`, `@radial-gradient`, `drop-shadow-*` (one per element), `animate`, `states`, `Path` with stroke, `Image` with `colorize` and tiling, `FocusScope`, `PopupWindow`, `Timer`, `accessible-*`, `SLINT_SCALE_FACTOR`, `slint-viewer`. Check any feature against the pinned version before relying on it.

### 4.2 Tonal surface with hairline

```slint
export component Surface inherits Rectangle {
    border-radius: Palette.r-lg;
    background: @linear-gradient(160deg, rgba(255, 255, 255, 0.07) 0%, rgba(255, 255, 255, 0.025) 100%);
    border-width: 1px;
    border-color: Palette.line;
    Rectangle {   // top highlight
        y: 0px;
        x: parent.border-radius;
        width: parent.width - 2 * parent.border-radius;
        height: 1px;
        background: @linear-gradient(90deg, transparent 0%, rgba(255, 255, 255, 0.22) 50%, transparent 100%);
    }
    @children
}
```

### 4.3 Buttons and focus

One button implementation with `kind` (`primary`, `secondary`, `quiet`, `warn`, `danger`) and `small`. Old exported names (`PrimaryButton`, `GhostButton`, `DangerButton`, `DangerConfirmButton`, `WipeButton`, `SuccessButton`) stay as thin wrappers with the **same properties and callbacks** so pages keep compiling. Every interactive component gets a `FocusScope`, a 2px `signal` focus ring on `has-focus`, Enter/Space activation (`key-pressed` returning `accept`), `accessible-role: button` and an `accessible-label` (icon-only buttons especially). Disabled means opacity plus a different cursor, never color alone.

### 4.4 Modal and toast animation

```slint
opacity: open ? 1 : 0;
visible: self.opacity > 0.01;
animate opacity { duration: Motion.base; easing: ease-out; }
```

The card inside also animates `y` (8 to 12px) and a slight scale. Esc cancels (`Key.Escape` in a `FocusScope`). Focus lands on the first field on open. Toasts slide and fade in, stack with a gap, keep the existing 3.8s dismissal, and get `accessible-role: alert` where the pinned Slint allows.

### 4.5 States over ternaries

Use `states [...]` with `in`/`out` animations for button and row interaction instead of long ternary chains. Hover = tone shift of 4 to 6% plus ink brightening, not a glow.

### 4.6 One column definition per table

Define a `global` (or one struct-of-lengths per page) holding column widths and use it in both header and rows so they cannot drift. Below a width threshold (`root.width < 1040px`) collapse secondary columns (URL, group) and move the row actions into one overflow `PopupWindow`. The primary action stays visible. Test at 900px.

### 4.7 Backdrop

Put the gradient on `Window.background` (if the pinned version needs a color there, use a full-size `Rectangle` first child). Blooms are radial-gradient rectangles sized in `vmax`-like expressions of `root.width/height`, behind the layout, with `opacity` only, never animated.

### 4.8 Icons

One family: 24x24 viewbox, `fill: transparent`, `stroke: tint`, `stroke-width: 1.75px`, round caps and joins. Keep the `Icon` component API (`svg-path`, `tint`, `size`) and add an optional `stroke-width`. Draw or adapt permissively licensed stroke paths (for example Lucide, ISC) and credit them in a comment. Needed: dashboard, database, archive/layers, clock, wrench, alert, bolt, plus, restore, trash, close, refresh, check, storage, edit, chevron, copy, pause, play. Draw a real brand mark (a stacked-disc cylinder with a clock arc) with `Path` to replace the "CB" tile; keep `assets/icon.png` unchanged unless asked.

### 4.9 Models are replaced on every refresh

Rust calls `set_*_rows(Rc::new(VecModel::from(rows)).into())` on each refresh, so every delegate is re-created and its `init` runs again. Consequences: (a) never use delegate `init` to animate "new row" entrance, it would flicker on every refresh; (b) page-level entrance animation runs once per page visit, which is fine because pages are `if active-page == ...`; (c) scroll position and `expanded-error-id`-style local state can reset, so keep the design stable under that. A diffing model update in Rust would be the proper fix; list it as a follow-up, do not do it here.

### 4.10 Honest filters

The filters cycle through values in Rust. Do not draw a dropdown. Render a labeled control that shows the current value, a cycle icon (not a text glyph) and "n of m" where known, or a segmented rail for the fixed sets (All, PostgreSQL, MongoDB / All, Succeeded, Failed). If clicking a specific segment is wanted, an **additive** Rust callback may be added; list it in the report. Do not edit existing Rust behavior.

### 4.11 Performance

femtovg re-renders on property change. Target zero continuous animations while idle. The only allowed loop is the indeterminate busy bar, and only while `*-busy` is true. No shadows inside rows. Keep gradients large and few. The 500-row fixture must scroll smoothly.

---

## 5. Motion language

- Durations in `Motion`: micro 120 to 160ms, element 220 to 320ms, page and modal 280 to 420ms. Nothing over 600ms unless the user caused it (the settle on backup completion).
- Easing: `ease-out` or `cubic-bezier(0.16, 1, 0.3, 1)` for entrances, `ease-in-out` for movement, no linear for UI, no bounce anywhere (this is a data-safety tool).
- Stagger rows 30 to 50ms in reading order, first 8 rows only, once per page visit.
- Animate opacity, position and color. Do not animate layout-driving sizes in lists.
- `Motion` exposes a global scale factor (default 1) so a future setting or an OS reduced-motion probe set durations to 0. If you cannot read the OS setting, say so (UNVERIFIED), and make the factor reachable by one env var (`CHRONOBASE_REDUCE_MOTION=1`) only if that is a one-line additive Rust change; otherwise leave the hook in Slint and note it.

---

## 6. Banned defaults (audit your output against every line)

Layout and content
- A grid of identical rounded cards, each with an icon in a tinted square. Cards that duplicate the sidebar.
- A bordered box around everything; boxes inside boxes; 1px borders as the only structure.
- Showing every option at once (all three OS instructions, all actions on every row at full weight).
- Placeholder copy, fake metrics, invented "next run" values.

Visual
- One hue for border, fill, text, badge, hover and shadow. Green as the brand. Near-black plus one neon accent as the whole personality. Purple-to-blue wash.
- Uppercase tracked eyebrow above every heading or column; `A · B · C` meta strings; text glyphs (`▾`, `→`, `✦`) as icons; mixed icon styles; X as delete.
- Text under 12px, dim secondary text under 4.5:1, color-only status.
- A glow shadow on hover for every control; hover effects on non-interactive elements; hard-edged "blur" discs.
- Gradient text; fake CSS glass over a flat background.

Motion
- Fade-up on every section; scattered micro-animations; anything looping while idle; bounce; animating list layout.

Slint and process
- `Text` without a font family; per-row drop-shadows; variable-height `ListView` rows; renaming MainWindow properties, callbacks or struct fields; touching `crates/core`.
- Writing code before RECON and the brief. Declaring success without screenshots. Removing the Slint attribution. Desktop-only at 1180px (the app has a 900px minimum).

---

## 7. Quality floor (always, without announcing it)

Readability
- Body 14px, secondary 13px, nothing under 12px. Titles tight, numerals mono and tabular. Line length under about 70 characters in prose.
- Measured contrast: body >= 7:1 target, anything >= 4.5:1, icons and focus ring >= 3:1. Text sits on the **brightest** thing behind it.
- Never truncate meaning: if text must elide, the full value is available (tooltip or detail). Short labels never elide at 900px.

Interaction and accessibility
- Every control reachable and operable by keyboard; visible 2px focus ring; Esc closes modals; first field focused on open.
- `accessible-role` and `accessible-label` on icon-only buttons, status badges and the toast.
- Status is icon + word + color. Destructive actions are named by what they do, in the destructive control's own label.
- Hit targets at least 32px tall on desktop.

Responsive (desktop)
- Correct at 900x600, 1180x760, 1600x1000, ultrawide, and scale factors 1.0, 1.25, 1.5, 2.0. No clipped text, no collapsed lists, no horizontal overflow. Dashboard fits 900x600 without the strata stack disappearing (it scrolls internally instead).

Robustness
- Designed empty, loading/busy, error and missing-on-disk states on every page. Tools-missing state is visible and actionable (links to Setup).
- Long and unicode names, 500 rows, and long error messages stay intact.
- Software renderer and no-shadow rendering remain legible and aligned.

Code
- Components are small and one concern per file. No duplicated column or color literals; colors come from `Palette`. Keep the project's comment style. `cargo fmt`, `clippy -D warnings`, `cargo test` stay green.

---

## 8. Agent execution notes (read if you are not Claude)

- Print the RECON block, viewer journey, full brief and the anti-default review as plain text **before** your first file edit, then keep going without waiting.
- Work in the Phase C order. After each step `cargo build` must pass. Use token aliases so the app never goes red mid-migration.
- Read real files. If a file you expect is missing, say so and adapt.
- Take the technique from this file, but pick final values by measuring. Do not weaken a rule to make an effect fit; simplify the effect.
- Never edit `crates/core`, tests or dependencies. Rust edits are additive and listed (a new callback, one env var). `build.rs` may change only to set the Slint style.
- If you cannot verify something, write UNVERIFIED. Never write "looks great".

---

## 9. Audit and final report

Before saying done, audit against sections 0, 6 and 7, every defect D1 to D24 and every Phase D test. Mark each **PASS / FAIL / UNVERIFIED** with evidence (file and line, or screenshot name). Fix every FAIL, re-audit, repeat until none remain.

```
BUILT:        what it is and the signature moment (1-2 lines)
CONCEPT:      Strata, and the metaphor used per page
PRESERVED:    Rust-facing contract kept (list), Slint attribution kept, aliases removed
FILES:        created / edited / deleted .slint files, fonts or assets added (paths only)
RUST/BUILD:   additive Rust edits and build.rs change, or "none"
DEFECTS:      D1..D24 each PASS / FAIL / UNVERIFIED / WONTFIX with evidence
CONTRAST:     the table from dev/contrast.py
VERIFIED:     pages x fixtures x sizes screenshotted, scale factor 1.5, software renderer, modals, toast, busy
UNVERIFIED:   everything you could not check (hover, pressed, OS reduced motion, ...)
FOLLOW-UPS:   e.g. diffing model updates in Rust
CONFLICTS:    project rule vs this skill, if any
```

Do not narrate effort or apologize.
