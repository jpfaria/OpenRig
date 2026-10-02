# Drum machine assets

Content for the built-in drum machine (`docs/drum-machine.md`).

## Grooves (`grooves/`)

Converted from the [Groove MIDI Dataset](https://magenta.tensorflow.org/datasets/groove)
(Jon Gillick, Adam Roberts, Jesse Engel, Mikhail Kulikov, Mike Tyka and
Douglas Eck, Google Magenta), licensed under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). The conversion
(loop and fill extraction, General MIDI to drum roles) is
`tools/drums/gmd_to_grooves.py`; each groove's `source` names the performance
it came from.

## Kits (`kits/`)

Hydrogen kit folders. The AVL Drumkits are by Glen MacArthur
([bandshed.net](https://www.bandshed.net/avldrumkits/)), licensed under
[CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/); their files
are kept as published.

| Folder | Kit |
|---|---|
| `black-pearl/` | AVLDrumkits Black Pearl 2023 (Pearl 4-piece, Sabian cymbals) |
| `red-zeppelin/` | AVLDrumkits Red Zeppelin 2023 (Ludwig, Zildjian cymbals) |
