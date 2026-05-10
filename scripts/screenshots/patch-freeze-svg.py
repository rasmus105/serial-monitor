#!/usr/bin/env python3
"""Patch Freeze SVG output for terminal-cell accurate rasterization."""

from __future__ import annotations

import argparse
import os
import re
from pathlib import Path
from xml.sax.saxutils import escape, unescape


TEXT_RE = re.compile(r"<text\s+([^>]*)>(.*?)</text>", re.DOTALL)
TSPAN_RE = re.compile(r"<tspan\s+([^>]*)>(.*?)</tspan>", re.DOTALL)
RECT_RE = re.compile(r"<rect\s+([^>]*)/>")
ATTR_RE = re.compile(r'([:\w-]+)="([^"]*)"')
BRAILLE_RE = re.compile(r"[\u2800-\u28ff]")
DARK_GRAY_FILL = "#808080"


def xml_unescape(text: str) -> str:
    return unescape(text, {"&quot;": '"', "&apos;": "'"})


def text_content(body: str) -> str:
    return "".join(xml_unescape(match.group(2)) for match in TSPAN_RE.finditer(body))


def parse_number(value: str) -> float | None:
    match = re.match(r"([0-9.]+)", value)
    if not match:
        return None
    return float(match.group(1))


def svg_cell_width(svg: str) -> float | None:
    font_size_match = re.search(r'font-size="([0-9.]+)px"', svg)
    if not font_size_match:
        return None

    cell_width_ratio = float(os.environ.get("SCREENSHOT_CELL_WIDTH_RATIO", "0.602142857"))
    return float(font_size_match.group(1)) * cell_width_ratio


def svg_text_rows(svg: str) -> list[tuple[float, float, str]]:
    rows: list[tuple[float, float, str]] = []
    for text in TEXT_RE.finditer(svg):
        attrs = dict(ATTR_RE.findall(text.group(1)))
        x = parse_number(attrs.get("x", ""))
        y = parse_number(attrs.get("y", ""))
        if x is None or y is None:
            continue
        rows.append((x, y, text_content(text.group(2))))
    return rows


def legend_span_near(content: str, column: int) -> tuple[int, int] | None:
    if column < 0 or column >= len(content):
        return None

    pairs = {"┌": "┐", "│": "│", "└": "┘"}
    start = next(
        (index for index in range(column, min(len(content), column + 5)) if content[index] in pairs),
        None,
    )
    if start is None:
        return None

    end_char = pairs.get(content[start])
    if end_char is None:
        return None

    end = content.find(end_char, start + 1)
    if end < 0:
        return None

    width = end - start + 1
    if 3 <= width <= 30:
        return start, width

    return None


def patch_legend_background_rects(svg: str) -> str:
    cell_width = svg_cell_width(svg)
    if cell_width is None:
        return svg

    rows = svg_text_rows(svg)
    if not rows:
        return svg

    def patch_rect(match: re.Match[str]) -> str:
        attrs = dict(ATTR_RE.findall(match.group(1)))
        x = parse_number(attrs.get("x", ""))
        y = parse_number(attrs.get("y", ""))
        height = parse_number(attrs.get("height", ""))
        width = parse_number(attrs.get("width", ""))
        if x is None or y is None or height is None or width is None:
            return match.group(0)

        row = next((row for row in rows if y <= row[1] <= y + height), None)
        if row is None:
            return match.group(0)

        text_x, _, content = row
        column = round((x - text_x) / cell_width)
        legend_span = legend_span_near(content, column)
        if legend_span is None:
            return match.group(0)

        legend_start, legend_width = legend_span
        expected_width = (legend_start - column + legend_width) * cell_width
        if width <= expected_width * 1.5:
            return match.group(0)

        patched_attrs = re.sub(
            r'width="[^"]+"',
            f'width="{expected_width:.2f}"',
            match.group(1),
            count=1,
        )
        return f"<rect {patched_attrs}/>"

    return RECT_RE.sub(patch_rect, svg)


def patch_progress_background_rects(svg: str) -> str:
    cell_width = svg_cell_width(svg)
    if cell_width is None:
        return svg

    rows = svg_text_rows(svg)
    if not rows:
        return svg

    def patch_rect(match: re.Match[str]) -> str:
        attrs = dict(ATTR_RE.findall(match.group(1)))
        fill = attrs.get("fill")
        if fill not in {"#444444", "#008080", "#808080"}:
            return match.group(0)

        x = parse_number(attrs.get("x", ""))
        y = parse_number(attrs.get("y", ""))
        height = parse_number(attrs.get("height", ""))
        width = parse_number(attrs.get("width", ""))
        if x is None or y is None or height is None or width is None:
            return match.group(0)

        row = next((row for row in rows if y <= row[1] <= y + height), None)
        if row is None:
            return match.group(0)

        text_x, _, content = row
        if "█" not in content or "%" not in content:
            return match.group(0)

        column = round((x - text_x) / cell_width)

        label = re.search(r"\d+%", content)
        if label is not None and label.start() <= column < label.end():
            expected_width = (label.end() - column) * cell_width
            if width <= expected_width * 1.5:
                return match.group(0)

            patched_attrs = re.sub(
                r'width="[^"]+"',
                f'width="{expected_width:.2f}"',
                match.group(1),
                count=1,
            )
            return f"<rect {patched_attrs}/>"

        if fill != "#444444":
            return match.group(0)

        progress_end = content.find("││")
        if progress_end < 0:
            return match.group(0)

        if column >= progress_end:
            return match.group(0)

        expected_width = (progress_end - column) * cell_width
        if width <= expected_width * 1.1:
            return match.group(0)

        patched_attrs = re.sub(
            r'width="[^"]+"',
            f'width="{expected_width:.2f}"',
            match.group(1),
            count=1,
        )
        return f"<rect {patched_attrs}/>"

    return RECT_RE.sub(patch_rect, svg)


def add_fill_to_unfilled_tspan_attrs(attrs: str, fill: str) -> str:
    if re.search(r'\sfill="', attrs):
        return attrs
    return f'{attrs} fill="{fill}"'


def patch_progress_row_fills(svg: str) -> str:
    def patch_text(match: re.Match[str]) -> str:
        attrs = match.group(1)
        body = match.group(2)
        content = text_content(body)
        patch_all_unfilled = (
            content.startswith("┌ Progress ")
            or content.startswith("│Bytes:")
            or (content.startswith("└") and "┘└" in content)
        )
        patch_first_unfilled = content.startswith("│") and "█" in content and "%" in content

        if not patch_all_unfilled and not patch_first_unfilled:
            return match.group(0)

        patched_parts: list[str] = []
        patched_first = False
        cursor = 0
        for tspan in TSPAN_RE.finditer(body):
            patched_parts.append(body[cursor : tspan.start()])
            tspan_attrs = tspan.group(1)
            text = tspan.group(2)
            if patch_all_unfilled or (patch_first_unfilled and not patched_first):
                if not re.search(r'\sfill="', tspan_attrs):
                    tspan_attrs = add_fill_to_unfilled_tspan_attrs(tspan_attrs, DARK_GRAY_FILL)
                    patched_first = True
            patched_parts.append(f"<tspan {tspan_attrs}>{text}</tspan>")
            cursor = tspan.end()

        patched_parts.append(body[cursor:])
        return f"<text {attrs}>" + "".join(patched_parts) + "</text>"

    return TEXT_RE.sub(patch_text, svg)


def patch_braille_rows(svg: str) -> str:
    cell_width = svg_cell_width(svg)
    if cell_width is None:
        return svg

    # Freeze lays out terminal columns at the Hack Nerd Font advance. Braille
    # falls back to a different font, so each Braille-containing row is rewritten
    # with explicit per-cell x positions before librsvg rasterization.
    def patch_text(match: re.Match[str]) -> str:
        attrs = match.group(1)
        body = match.group(2)
        if not BRAILLE_RE.search(text_content(body)):
            return match.group(0)

        x_match = re.search(r'x="([0-9.]+)px"', attrs)
        if not x_match:
            return match.group(0)

        x_origin = float(x_match.group(1))
        column = 0
        patched_parts: list[str] = []

        for tspan in TSPAN_RE.finditer(body):
            tspan_attrs = tspan.group(1)
            fill_match = re.search(r'fill="([^"]+)"', tspan_attrs)
            fill_attr = f' fill="{fill_match.group(1)}"' if fill_match else ""

            for char in xml_unescape(tspan.group(2)):
                if char != " ":
                    x = x_origin + column * cell_width
                    patched_parts.append(
                        f'<tspan xml:space="preserve" x="{x:.2f}px"{fill_attr}>{escape(char)}</tspan>'
                    )
                column += 1

        return f'<text {attrs}>' + "".join(patched_parts) + "</text>"

    return TEXT_RE.sub(patch_text, svg)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("svg", type=Path)
    args = parser.parse_args()

    svg = args.svg.read_text(encoding="utf-8")
    svg = patch_legend_background_rects(svg)
    svg = patch_progress_background_rects(svg)
    svg = patch_progress_row_fills(svg)
    svg = patch_braille_rows(svg)
    args.svg.write_text(svg, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
