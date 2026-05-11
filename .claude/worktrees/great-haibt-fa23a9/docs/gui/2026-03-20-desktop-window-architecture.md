# OpenRig Desktop Window Architecture

This document captures the user-approved GUI architecture for the next desktop phase.

It is the authoritative reference for the shell, window behavior, and reuse strategy across desktop and future touch mode.

## Goal

Turn the current chains UI into a real desktop multi-window application without duplicating forms or business logic for touch mode.

## Approved decisions

### Launcher

- launcher is the only screen that shows branding
- launcher keeps the logo
- launcher remains the project open/create screen

### Non-launcher shell

- non-launcher screens do not show the logo
- the top bar contains only actions
- on the chains screen, the project name replaces the previous logo/header identity area
- the app icon should use the logomark only

### Chains screen

- `Nova chain` moves to the top-right of the chains panel
- each chain uses `In` and `Out` chips at the extremities of the signal line
- hover on `In` and `Out` shows:
  - device
  - sample rate
  - buffer size
  - channels
- click on `In` opens dedicated input config
- click on `Out` opens dedicated output config
- runtime updates should apply immediately when these settings change

### Desktop windows

Desktop opens normal independent windows for:

- project configuration
- chain configuration
- chain input configuration
- chain output configuration
- add/edit stage

Desktop rules:

- windows are not modal blockers
- multiple stage windows may be open at once
- stage type picking stays inline in the main chains screen
- after the type is chosen, the actual stage editor opens in its own window

### Touch behavior

- touch keeps every flow inside the main window
- touch reuses the same forms/components as desktop
- only the container changes; the content and schema handling stay shared

### Chain config scope

- chain configuration edits only chain metadata
- input/output are not edited directly there
- the chain config screen shows summaries and opens dedicated input/output windows

### Dynamic catalog rule

- the GUI must not manually own stage types, model lists, or parameter schemas
- the core is the source of truth
- new stage types, models, and parameter changes must appear automatically in the GUI

## Reuse architecture

The implementation should split each editor into:

1. a shared content component
2. a desktop window host
3. a touch inline host

This keeps:

- one form layout
- one parameter rendering path
- one schema interpretation path
- one validation path

And changes only:

- container
- open/close behavior
- placement in desktop vs touch

## Implementation order

1. remove branding from non-launcher screens
2. move project name and `Nova chain` to the approved shell positions
3. add `In` and `Out` chips with hover data
4. create dedicated input/output forms as shared components
5. wire desktop input/output windows
6. refactor chain config to reuse those forms via summary + launch points
7. extract stage editor form from the inline drawer into shared content
8. keep inline type picker, but open stage editor in external desktop windows
9. keep touch on inline containers using the same shared content

## Constraints

- do not duplicate editor forms for desktop and touch
- do not reintroduce routed-page behavior for desktop config flows
- do not hardcode stage catalogs in the GUI
- do not remove the inline type picker from the desktop chains screen
