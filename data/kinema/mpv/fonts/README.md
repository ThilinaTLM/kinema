<!--
SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
SPDX-License-Identifier: Apache-2.0
-->

# Kinema icon font

`KinemaIcons.ttf` is a **static**, subsetted build of Google's
**Material Symbols Rounded** (Apache-2.0), instantiated at axis
values `FILL=1, wght=400, GRAD=0, opsz=24`. It provides the 22
glyphs rendered by the in-mpv chrome (`kinema-overlays.lua`).

The family is renamed to `Kinema Icons` so it never collides with
a user-installed Material Symbols build on the same system. The
Lua script reaches for it via `{\fnKinema Icons}`.

## Why subset?

Upstream ships either a 15 MB variable font or a multi-MB static
TTF per axis combination. Shipping all that over the wire, just
to render ~20 icons, makes no sense. The subset weighs ~8 KB.

## Why static?

libass (0.17.x) has had intermittent bugs with variable-font
resolution; pre-instancing the font at a fixed axis combination
sidesteps that code path entirely.

## Regenerating the font

Required: Python 3.10+, `fonttools`, `brotli`.

    python3 -m venv .venv
    .venv/bin/pip install fonttools brotli

Fetch the upstream variable font. Pin a commit if you care about
byte-for-byte reproducibility:

    curl -sL -o MaterialSymbolsRounded.ttf \
        'https://github.com/google/material-design-icons/raw/master/variablefont/MaterialSymbolsRounded%5BFILL%2CGRAD%2Copsz%2Cwght%5D.ttf'

Run the builder (`build_font.py` below). It instantiates, subsets,
and renames the family.

```python
# build_font.py — paste and run from the same directory
import io
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools.subset import Subsetter, Options

SRC, DST = 'MaterialSymbolsRounded.ttf', 'KinemaIcons.ttf'

GLYPHS = {
    'play_arrow':      0xE037, 'pause':           0xE034,
    'skip_previous':   0xE045, 'skip_next':       0xE044,
    'replay_10':       0xE059, 'forward_10':      0xE056,
    'volume_up':       0xE050, 'volume_off':      0xE04F,
    'volume_down':     0xE04D, 'closed_caption':  0xE01C,
    'music_note':      0xE405, 'speed':           0xE9E4,
    'settings':        0xE8B8, 'fullscreen':      0xE5D0,
    'fullscreen_exit': 0xE5D1, 'close':           0xE5CD,
    'check':           0xE5CA, 'arrow_drop_down': 0xE5C5,
    'arrow_back':      0xE5C4, 'info':            0xE88E,
    'keyboard':        0xE312, 'more_vert':       0xE5D4,
}

src  = TTFont(SRC)
inst = instancer.instantiateVariableFont(
    src, {'FILL': 1, 'wght': 400, 'GRAD': 0, 'opsz': 24})

opts = Options()
opts.layout_features = ['*']
opts.notdef_outline  = True
opts.glyph_names     = True
opts.name_IDs        = ['*']
opts.name_languages  = ['*']
opts.name_legacy     = True
opts.drop_tables     = ['DSIG', 'STAT', 'fvar', 'avar',
                        'MVAR', 'HVAR', 'VVAR', 'gvar']
sub = Subsetter(options=opts)
sub.populate(unicodes=sorted(GLYPHS.values()))
sub.subset(inst)

# Rename family so it can't collide with installed MS builds.
NEW = {
    1:  'Kinema Icons',
    2:  'Regular',
    3:  'Kinema Icons Regular; from Material Symbols Rounded',
    4:  'Kinema Icons Regular',
    6:  'KinemaIcons-Regular',
    16: 'Kinema Icons',
    17: 'Regular',
    21: 'Kinema Icons',
    22: 'Regular',
}
name = inst['name']
for nid in NEW:
    name.names = [n for n in name.names if n.nameID != nid]
for nid, val in NEW.items():
    name.setName(val, nid, 3, 1, 0x409)  # Windows, Unicode, en-US
    name.setName(val, nid, 1, 0, 0)      # Macintosh, Roman, Eng.

inst.save(DST)
```

## Codepoint table

The Lua script hard-codes these codepoints in its theme table.
Any change here must be mirrored there.

| slot            | codepoint | upstream glyph     |
|-----------------|-----------|--------------------|
| play_arrow      | U+E037    | play_arrow         |
| pause           | U+E034    | pause              |
| skip_previous   | U+E045    | skip_previous      |
| skip_next       | U+E044    | skip_next          |
| replay_10       | U+E059    | replay_10          |
| forward_10      | U+E056    | forward_10         |
| volume_up       | U+E050    | volume_up          |
| volume_off      | U+E04F    | volume_off         |
| volume_down     | U+E04D    | volume_down        |
| closed_caption  | U+E01C    | closed_caption     |
| music_note      | U+E405    | music_note         |
| speed           | U+E9E4    | speed              |
| settings        | U+E8B8    | settings           |
| fullscreen      | U+E5D0    | fullscreen         |
| fullscreen_exit | U+E5D1    | fullscreen_exit    |
| close           | U+E5CD    | close              |
| check           | U+E5CA    | check              |
| arrow_drop_down | U+E5C5    | arrow_drop_down    |
| arrow_back      | U+E5C4    | arrow_back         |
| info            | U+E88E    | info               |
| keyboard        | U+E312    | keyboard           |
| more_vert       | U+E5D4    | more_vert          |

## License

Material Symbols is distributed under the Apache License, Version
2.0. The unmodified upstream license text is preserved alongside
this README in `LICENSE-MaterialSymbols`. Our subset is a
derivative work and is likewise distributed under Apache-2.0,
consistent with Kinema's own license.
