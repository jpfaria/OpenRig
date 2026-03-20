# GUI Redesign

This document is the working reference for the OpenRig GUI redesign.

It is intended to be the shared source of truth for future agents working on the desktop UI and, later, the touch UI.

## Primary purpose

This file exists to preserve the decisions already made with the user across sessions and across different agents.

Agents should treat it as the canonical handoff document for GUI direction unless the user explicitly replaces a decision later.

If code, screenshots, mockups, or prior assumptions conflict with this file, this file should win until it is updated.

## Status

Working draft

Desktop-first redesign approved at the direction level.

Desktop implementation started.

## Scope

- current focus is desktop mode
- desktop is mouse-first
- the same visual language and component model should be reusable in a future touch mode
- touch-specific layout changes are expected later, but the desktop redesign should not paint the system into a corner

## Product framing

OpenRig is not a single physical pedalboard clone.

The GUI should communicate that a project can contain multiple tracks, and that each track can behave like its own pedalboard/rig chain.

The target feeling is:

- professional guitar processing environment
- modern pedalboard identity
- more technical and product-grade than a skeuomorphic boutique pedal app

## Confirmed design decisions

### Overall direction

- redesign should be substantial, not a cosmetic cleanup
- chosen direction is a focused hybrid
- the app is track-centric
- when a track is opened, it can become the main pedalboard view
- the user must still be able to switch to other tracks without losing the sense of a multi-track project

### Desktop information architecture

- there is a project-level view centered on the list of tracks
- each track shows its stage chain in the project view
- entering a track opens a pedalboard-oriented editor for that track
- track switching inside the open-track experience uses a collapsible drawer, not fixed tabs or a permanent side rail

### Track list behavior

- each track card should show the real SVG icons for its stages
- stages are interactive, not decorative
- clicking a stage in the project/track list opens a quick inline editor without leaving the project view
- stages have explicit enabled/disabled state
- enabled/disabled must be visible in the chain itself, not hidden in a secondary panel

### Stage insertion flow

- the chain must allow insertion of a new stage between existing stages
- insertion affordance appears only on hover/focus in the space between stages
- adding a stage is a two-step flow:
  1. choose the stage type
  2. choose the specific item from that type
- after selection, the new stage is inserted into the chain position the user targeted

## Implemented now

The current desktop implementation already includes:

- refreshed desktop visual tokens with darker graphite surfaces and blue digital accenting
- redesigned launcher with a stronger brand panel and recent-project focus
- redesigned project overview with denser track cards
- stage chips with SVG icons and visible enabled/bypass state
- quick inline stage editor in the project view
- stage insertion flow in two steps:
  1. choose type
  2. choose model
- inserting the new stage directly into the selected chain position
- stage enable/bypass toggle from the quick editor
- redesigned project settings and track routing editor screens

## Correction note

An early implementation pass drifted toward a generic dark dashboard / SaaS product layout.

That drift is not aligned with the approved direction and should not be treated as the target style.

Future agents should explicitly correct away from:

- marketing-style split hero panels
- generic dashboard cards with passive chips
- stage chains that read like badges instead of hardware modules
- overly flat layouts that hide the pedalboard identity

The approved target is still:

- technical pedalboard workspace
- multi-track first
- stage modules with stronger hardware character
- project view that feels like a rig control surface, not a SaaS admin screen

## Not implemented yet

The following parts are still pending relative to the full redesign direction:

- dedicated pedalboard view for the opened track
- deeper per-parameter stage editor inside the project view
- desktop drag-and-drop stage reordering
- touch-specific layout adaptation

### Visual language

- visual direction is "pedaleira pro moderna"
- the UI should feel like professional guitar gear translated into software
- avoid a generic DAW look
- avoid a fake single-board metaphor that hides the multi-track model
- avoid excessive empty space and oversized neutral panels
- hierarchy, density, and state clarity are more important than ornamental effects

### Controls and component language

- controls may borrow cues from hardware pedals
- component styling should remain clean and technical, not toy-like
- knobs should look intentional and premium
- current preferred knob direction is digital glow
- knob values should live inside the knob to reduce visual sprawl
- control treatment should vary by stage type when appropriate, so different effect families have more personality

## Layout intent by screen

### Launcher

The launcher is a project selector, not a landing page.

It must stay minimal and operational:

- logo present but contained
- recent projects list is the main focus
- each recent project opens with a single click on the row
- each recent row keeps only one secondary action: remove from recents
- only two primary actions outside the list:
  - `Abrir projeto`
  - `Novo projeto`

Approved visual direction for the launcher:

- modern digital pedalboard mood
- more scenic background, but restrained
- straight, technical geometry
- avoid soft rounded cards and pill-heavy UI
- avoid explanatory marketing copy, status pills, counters, or decorative dashboard language

### Project view

The project view should behave like the overview and editing hub for multiple tracks.

It should prioritize:

- clear scan of all tracks
- visible stage chains on each track
- quick stage editing inline
- fast insertion and reordering intent in the chain
- obvious entry into the deeper pedalboard view for a track

### Open track view

The open track view should make the selected track feel like a pedalboard/rig workspace.

It should prioritize:

- the currently selected track as the main canvas
- deeper stage editing than the project view
- easy access to other tracks through a collapsible drawer
- preserving the mental model that the user is still inside a larger multi-track project

### Project setup and routing views

Setup/routing screens should keep the new visual language, but they should remain more utilitarian than the pedalboard editor.

They should prioritize:

- denser rows
- clear selected vs unselected states
- faster scanning of input/output assignments
- less dead space than the current implementation

## Reuse constraints for future touch mode

The desktop redesign should prepare for touch reuse by keeping:

- consistent tokens for color, spacing, elevation, and state
- reusable stage card, stage chip, knob, switch, and button primitives
- layouts that can collapse into larger tap targets later

The desktop version may still use hover and precise pointer affordances where helpful.

## Explicit non-goals

- do not redesign desktop around a single fixed pedalboard metaphor
- do not make project-level editing depend on always entering the full pedalboard view
- do not reduce stage icons to passive badges
- do not hide chain editing behind obscure menus

## Open questions

These items are not fully locked yet and should be resolved before final implementation details solidify:

- in the open track view, whether primary editing lives entirely on the pedalboard blocks or uses a pedalboard plus complementary side panel
- the exact visual treatment for enabled vs bypassed vs selected stages
- whether stage reordering is drag-and-drop, command-driven, or both in desktop mode
- how much of the stage quick editor is editable inline before the user should transition into the deeper track view

## Working rule for future agents

When changing the GUI direction or implementing material behavior based on this redesign:

- read this file first
- treat it as the starting context for the GUI work, not as optional background reading
- update this file when a design decision becomes concrete or changes
- record new user-approved decisions here as soon as possible so later agents inherit the same context
- do not silently diverge from the decisions above
- if implementation forces a tradeoff, record the tradeoff here
