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

## New approved architecture

The next implementation phase is now approved and should override older assumptions where they conflict.

### Window strategy

- the logo appears only on the launcher
- outside the launcher, the top bar should contain only actions
- in desktop mode, the app should feel like a native multi-window desktop tool, not a routed web app
- in touch mode, the same forms and components should stay inside the main window

### Desktop vs touch behavior

- desktop mode opens independent normal windows for:
  - project configuration
  - chain configuration
  - chain input configuration
  - chain output configuration
  - add/edit stage
- desktop mode allows multiple stage windows open at the same time
- touch mode keeps these flows inside the main window
- the implementation must reuse the same form components in both modes and only swap the container strategy

### Chain screen shell

- the project name replaces the old logo position on the chains screen
- `Nova chain` lives in the top-right corner of the chains panel
- the launcher keeps the branding; the chains screen does not
- the app window/icon should use the logomark only

### Chain endpoints

- each chain should show `In` and `Out` as chips at the start and end of the signal line
- hover on `In` and `Out` must show:
  - device name
  - sample rate
  - buffer size
  - channels
- click on `In` opens a dedicated input window
- click on `Out` opens a dedicated output window
- input/output changes apply immediately to the runtime

### Chain configuration scope

- the chain configuration window should contain only chain metadata
- input and output are not edited directly inside the chain config form
- the chain config form should surface the current input/output summary and open the dedicated input/output windows

### Stage flow

- desktop mode keeps the stage type picker inline inside the chains screen
- after choosing the type, desktop opens a dedicated stage window
- each stage can open in its own separate desktop window
- touch mode keeps the same stage flow inside the main window
- add and edit should use the same stage form component

### Reuse rule

- no behavior or schema for stage types, models, or parameters may be hardcoded in the GUI
- if the core adds a new stage type, model, or parameter, the GUI must reflect it automatically
- visual mapping may still provide presentation metadata, but catalog membership and parameter schema come from the core

## Scope

- current focus is desktop mode
- desktop is mouse-first
- the same visual language and component model should be reusable in a future touch mode
- touch-specific layout changes are expected later, but the desktop redesign should not paint the system into a corner

## Product framing

OpenRig is not a single physical pedalboard clone.

The GUI should communicate that a project can contain multiple chains, and that each chain can behave like its own pedalboard/rig chain.

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
- the user must still be able to switch to other chains without losing the sense of a multi-chain project

### Desktop information architecture

- there is a project-level view centered on the list of chains
- each track shows its stage chain in the project view
- entering a track opens a pedalboard-oriented editor for that track
- track switching inside the open-track experience uses a collapsible drawer, not fixed tabs or a permanent side rail

### Chain list behavior

- each track card should show the real SVG icons for its stages
- stages are interactive, not decorative
- clicking a stage in the project/track list opens a floating editor panel over the project view
- stages have explicit enabled/disabled state
- enabled/disabled must be visible in the chain itself, not hidden in a secondary panel

### Block insertion flow

- the chain must allow insertion of a new stage between existing stages
- insertion affordance appears only on hover/focus in the space between stages
- adding a stage is a two-step flow:
  1. choose the stage type from a floating icon-only chooser
  2. configure the stage in the right-side drawer by selecting the model and adjusting parameters
- after selection, the new stage is inserted into the chain position the user targeted

## Implemented now

The current desktop implementation already includes:

- refreshed desktop visual tokens with darker graphite surfaces and blue digital accenting
- redesigned launcher with a stronger brand panel and recent-project focus
- redesigned project overview with denser track cards
- stage chips with SVG icons and visible enabled/bypass state
- floating right-side stage editor drawer in the project view
- add-stage starts with a separate floating type chooser, then enters the drawer
- edit-stage opens the drawer directly and does not allow changing the type
- stage insertion flow in two steps:
  1. choose type
  2. choose model
- inserting the new stage directly into the selected chain position
- stage enable/bypass and delete actions inside the stage drawer
- redesigned project settings and track routing editor screens
- project settings and track routing editor as separate desktop windows

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

The project view should behave like the overview and editing hub for multiple chains.

It should prioritize:

- clear scan of all chains
- visible stage chains on each track
- quick stage editing through a floating drawer without leaving the project view
- fast insertion and reordering intent in the chain
- obvious entry into the deeper pedalboard view for a track

### Open track view

The open track view should make the selected track feel like a pedalboard/rig workspace.

It should prioritize:

- the currently selected track as the main canvas
- deeper stage editing than the project view
- easy access to other chains through a collapsible drawer
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
- whether the block drawer should remain a right-side inspector or evolve into a wider floating editor for more complex block families

## Working rule for future agents

When changing the GUI direction or implementing material behavior based on this redesign:

- read this file first
- treat it as the starting context for the GUI work, not as optional background reading
- update this file when a design decision becomes concrete or changes
- record new user-approved decisions here as soon as possible so later agents inherit the same context
- do not silently diverge from the decisions above
- if implementation forces a tradeoff, record the tradeoff here
