#!/usr/bin/env python3
"""Writes a TrueType font, family "ETV Box", whose only glyph ("A") fills the em square.

Any fallback font draws "A" with an empty top-left corner, so a white pixel there shows
that libass loaded this font from the fonts folder.

usage: make_font.py <out.ttf>   (needs fontTools)
"""
import sys

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

EM = 1000


def box():
    pen = TTGlyphPen(None)
    pen.moveTo((0, 0))
    pen.lineTo((0, EM))
    pen.lineTo((EM, EM))
    pen.lineTo((EM, 0))
    pen.closePath()
    return pen.glyph()


fb = FontBuilder(EM, isTTF=True)
fb.setupGlyphOrder([".notdef", "A"])
fb.setupCharacterMap({ord("A"): "A"})
fb.setupGlyf({".notdef": TTGlyphPen(None).glyph(), "A": box()})
fb.setupHorizontalMetrics({".notdef": (EM, 0), "A": (EM, 0)})
fb.setupHorizontalHeader(ascent=EM, descent=0)
fb.setupNameTable({"familyName": "ETV Box", "styleName": "Regular"})
fb.setupOS2(sTypoAscender=EM, sTypoDescender=0, usWinAscent=EM, usWinDescent=0)
fb.setupPost()
# fixed timestamps keep the output byte-identical
fb.setupHead(unitsPerEm=EM, created=0, modified=0)
fb.save(sys.argv[1])
