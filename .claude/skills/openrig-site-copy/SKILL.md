---
name: openrig-site-copy
description: Use when writing or rewriting any user-facing line on the OpenRig site (site/index.html, site/sections/*.html, site/i18n/**) — headline, lede, section title, card title, button label or note — in any of the three languages, including when the owner says a line "sounds bad" and wants alternatives.
---

# OpenRig site copy

## Overview

The site's copy kept getting rejected because it was written as poetry instead
of as a claim. This is the contract a line has to satisfy before it is shown to
anyone.

**Core rule: a line only earns its place if it would be FALSE on a competitor's
site.** If "OpenRig" can be swapped for "Neural DSP" or "Helix" and the sentence
stays true, the sentence says nothing.

## The recipe

Every block on the page is two pieces, in this order:

1. **Headline** — states what the thing *does* or what the player *gets*. Not a
   mood. **There is no word limit.** A headline that has to explain the product
   is allowed to be a full sentence; a four-word slogan that explains nothing
   reads as empty and gets rejected. Length follows the content, never the other
   way around.
2. **Lede** — ONE sentence, anchored to something concrete: a number, a real gear
   name, a file format, a platform, or an action the player performs.

No third supporting sentence, no wind-up clause before the point.

**Owner's correction (recorded):** short does not mean good. Asked for a
headline, the failure mode here was compressing the product down to a slogan —
"Um rig pra cada instrumento" — which passes every check and still says nothing
a reader can act on. Say the whole thing.

## The four checks — run every one before proposing a line

| Check | How to run it | Fail → |
|---|---|---|
| **Swap** | Replace OpenRig with Neural DSP / ToneX / Helix. Still true? | Generic. Rewrite around what only this app does. |
| **Concrete** | Does the line contain a number, proper name, format (`.nam`, IR, VST3, Mac) or an action verb? | Zero anchors = rewrite. |
| **Breath** | Read it out loud. Did you need a breath mid-sentence? | Too long. Cut. |
| **Hedge** | Search for "sem X que você sinta", "não é só", "muito mais que", "de verdade", "simplesmente" | Turn the hedge into a positive claim. |

## Measured anti-patterns

These are real lines from this site that the owner rejected, and why:

| Rejected line | What broke |
|---|---|
| `Só falta / você plugar / a guitarra.` | Three-line split of one weak thought; says nothing that isn't true of every amp sim. Fails Swap + Concrete. |
| `... De graça, sem latência que você sinta.` | Hedge. States the absence of a defect instead of the benefit. |
| `O amp responde / como o de verdade.` | "de verdade" hedge; fails Swap — every modeler claims this. |
| `Ataque de palheta, sustain, o jeito que a nota comprime quando você força. O OpenRig tem amp nativo escrito em Rust e também carrega captura neural de amp real — você escolhe. Nos dois casos, sem latência que você sinta...` | Three sentences where one belongs; implementation jargon in a top-of-page lede. Fails Breath. |
| `Isso começou porque três / pedaleiras custam caro.` | Vague subject ("Isso"), a number that explains nothing to a first-time reader. |

**Splitting a line over `<br>` does not make it stronger.** The `<br>` is
typography for a line that already works. If the line is weak, breaking it in
three just spreads the weakness over three lines.

## Implementation jargon stays out of the top

Rust, isolated runtime, native DSP, neural capture, real-time platform: none of
these belong in a hero or an H2. Translate them into the consequence for the
player, and only in body copy:

- ❌ "cada entrada é um runtime isolado"
- ✅ "duas guitarras na mesma interface, uma não atrapalha a outra"
- ❌ "amp nativo escrito em Rust e captura neural"
- ✅ "amp do app ou captura de um amp real — você escolhe"

## How this space actually writes (verbatim references)

Checked against the sites players already read:

- Neural DSP — product name as headline, one concrete promise under it:
  "No compromises. Just smaller." / "An infinite number of amps and pedals on
  your pedalboard" / "The most powerful floorboard amp modeler on the planet"
- TONE3000 — names real gear and states the price:
  "Wish you could play iconic amps like the Fender Twin Reverb, Marshall JCM800,
  and Vox AC30, without spending zillions?" / "It's free!"
- Obsidian — imperative headline, then one sentence saying plainly what it is:
  "Sharpen your thinking." / "The free and flexible app for your private
  thoughts." / "Free without limits."

Common shape: **short claim, then one plain sentence with a concrete anchor.**
None of them open with a mood.

## Three languages, written natively

`pt-BR` and `es-ES` are not translations of the English line. Write each one in
its own language and run the four checks again in that language — a line can
pass in English and read like a translation in Portuguese.

Pick ideas that survive all three languages. A pun that only lands in one is a
dead end: it forces two weak lines to keep one good one.

## Where the lines live

| Block | HTML (English fallback, inline) | Translations |
|---|---|---|
| Hero, nav, footer | `site/index.html` | `site/i18n/{en,pt-BR,es-ES}/shell.json` |
| Sections | `site/sections/<name>.html` | `site/i18n/{en,pt-BR,es-ES}/{gear,features,end}.json` |

Every line exists in **four** places: the inline English in the HTML plus the
three JSON files. Changing one line means changing all four, in the same commit.
Grep the `data-i18n` key to find them all.

## Presenting options to the owner

He picks the line; you generate the candidates. Never show one option, and never
show three rewordings of the same thought — that reads as no choice at all.

Show **three candidates attacking different angles**:

1. What it does — the capability ("Teu rig inteiro num computador")
2. What it saves — money, weight, hassle ("Chega de carregar pedaleira")
3. What the player feels/does — the action at the guitar ("Plugou, tocou")

If all three come back rejected, the angle is wrong, not the wording: ask what
the block has to say, in one short question, before writing another round.

## Common mistakes

- Rewording a rejected line instead of changing its angle.
- Trimming a line to hit a length target. Empty and short is worse than long and clear.
- Writing the English first and translating — the other two languages end up stiff.
- Editing the HTML and forgetting the three JSONs (or vice-versa), leaving the
  page mixing old and new copy per language.
- Adding a third sentence to "explain" the lede. Cut it; the lede is the explanation.

## Checking the page before proposing it

Serve the working copy and read the rendered line, don't eyeball the HTML:

```bash
cd .solvers/issue-N/site && python3 -m http.server 8898
```

Then drive it with the Playwright MCP. **Screenshots land in the session's
working directory — which is the user's main folder, and LEI ZERO forbids
writing there.** Pass an absolute `filename` under the scratchpad, and if one
still lands in the repo root, move it out immediately.

What the render has to show: the headline still fits the three `<br>` lines at
desktop width, `document.documentElement.scrollWidth === window.innerWidth`
(no horizontal overflow), and the same in each of the three languages — a line
that fits in English can wrap to a fourth line in Portuguese or Spanish.
