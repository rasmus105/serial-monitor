#!/usr/bin/env python3
"""Validate generated screenshots for obvious terminal rendering artifacts."""

from __future__ import annotations

import argparse
import sys
from collections import deque
from pathlib import Path

try:
    import numpy as np
    from PIL import Image
except ImportError as exc:  # pragma: no cover - exercised only without local tooling.
    print(
        f"Error: {Path(__file__).name} requires Pillow and NumPy ({exc}).",
        file=sys.stderr,
    )
    sys.exit(2)


def accent_mask(image: Image.Image) -> np.ndarray:
    arr = np.asarray(image.convert("RGB"), dtype=np.int16)
    r = arr[:, :, 0]
    g = arr[:, :, 1]
    b = arr[:, :, 2]

    teal = (r < 80) & (g > 70) & (b > 70) & (np.abs(g - b) < 80)
    green = (r < 80) & (g > 70) & (b < 80)
    yellow = (r > 90) & (g > 90) & (b < 80)
    return teal | green | yellow


def series_color_mask(image: Image.Image) -> np.ndarray:
    arr = np.asarray(image.convert("RGB"), dtype=np.int16)
    r = arr[:, :, 0]
    g = arr[:, :, 1]
    b = arr[:, :, 2]

    teal = (r < 60) & (g > 80) & (b > 80) & (g - r > 40) & (b - r > 40)
    green = (r < 80) & (g > 90) & (b < 80) & (g - r > 50) & (g - b > 40)
    yellow = (r > 90) & (g > 90) & (b < 90) & (r - b > 50) & (g - b > 50)
    return teal | green | yellow


def component_boxes(mask: np.ndarray):
    visited = np.zeros(mask.shape, dtype=bool)
    height, width = mask.shape
    points = np.argwhere(mask)

    for start_y, start_x in points:
        if visited[start_y, start_x]:
            continue

        queue: deque[tuple[int, int]] = deque([(int(start_y), int(start_x))])
        visited[start_y, start_x] = True
        xs: list[int] = []
        ys: list[int] = []

        while queue:
            y, x = queue.popleft()
            xs.append(x)
            ys.append(y)

            for ny in (y - 1, y, y + 1):
                if ny < 0 or ny >= height:
                    continue
                for nx in (x - 1, x, x + 1):
                    if nx < 0 or nx >= width or visited[ny, nx] or not mask[ny, nx]:
                        continue
                    visited[ny, nx] = True
                    queue.append((ny, nx))

        yield np.array(xs), np.array(ys)


def diagonal_drift(xs: np.ndarray, ys: np.ndarray) -> tuple[bool, str | None]:
    if xs.size < 200:
        return False, None

    component_height = int(ys.max() - ys.min() + 1)
    component_width = int(xs.max() - xs.min() + 1)

    if component_height < 100 or component_width < 40:
        return False, None

    row_centers: list[float] = []
    for row in np.unique(ys):
        row_xs = xs[ys == row]
        if row_xs.size >= 4:
            row_centers.append(float(np.median(row_xs)))

    if len(row_centers) < 12:
        return False, None

    centers = np.array(row_centers)
    span = float(centers.max() - centers.min())
    jumps = int(np.count_nonzero(np.abs(np.diff(centers)) > 6.0))

    if span > max(60.0, component_width * 0.8) and jumps >= 3:
        return True, f"span={span:.1f}px jumps={jumps} bbox={component_width}x{component_height}"

    return False, None


def foreground_luminance(arr: np.ndarray, box: tuple[int, int, int, int]) -> tuple[float | None, int]:
    x0, y0, x1, y1 = box
    sub = arr[y0:y1, x0:x1].astype(np.float32)
    if sub.size == 0:
        return None, 0

    lum = 0.2126 * sub[:, :, 0] + 0.7152 * sub[:, :, 1] + 0.0722 * sub[:, :, 2]
    foreground = lum > 65.0
    if int(foreground.sum()) < 20:
        return None, int(foreground.sum())

    return float(lum[foreground].mean()), int(foreground.sum())


def traffic_timestamp_issues(image: Image.Image) -> list[str]:
    arr = np.asarray(image.convert("RGB"))
    height, width, _ = arr.shape
    r = arr[:, :, 0]
    g = arr[:, :, 1]
    b = arr[:, :, 2]
    x_grid = np.indices((height, width))[1]
    y_grid = np.indices((height, width))[0]

    rx_green = (r < 50) & (g > 80) & (b < 50) & (x_grid < width * 0.25) & (y_grid < height * 0.4)
    rows = np.where(rx_green.sum(axis=1) > 5)[0]
    if rows.size == 0:
        return ["traffic screenshot: could not locate first RX row for timestamp color check"]

    row_top = int(rows.min())
    scale_x = width / 1189.0
    timestamp_box = (
        int(120 * scale_x),
        max(0, row_top - 3),
        int(180 * scale_x),
        min(height, row_top + 18),
    )
    payload_box = (
        int(185 * scale_x),
        max(0, row_top - 3),
        int(520 * scale_x),
        min(height, row_top + 18),
    )

    timestamp_lum, timestamp_pixels = foreground_luminance(arr, timestamp_box)
    payload_lum, payload_pixels = foreground_luminance(arr, payload_box)
    if timestamp_lum is None or payload_lum is None:
        return [
            "traffic screenshot: insufficient timestamp/payload pixels "
            f"for color check (timestamp={timestamp_pixels}, payload={payload_pixels})"
        ]

    if payload_lum - timestamp_lum < 35.0:
        return [
            "traffic screenshot: payload is not visibly brighter than muted timestamp "
            f"(timestamp={timestamp_lum:.1f}, payload={payload_lum:.1f})"
        ]

    if timestamp_lum > 150.0:
        return [f"traffic screenshot: timestamp is not muted gray enough (timestamp={timestamp_lum:.1f})"]

    default_text_boxes = {
        "config label": (
            int(795 * scale_x),
            int(275 * (height / 858.0)),
            int(900 * scale_x),
            int(300 * (height / 858.0)),
        ),
        "config value": (
            int(1015 * scale_x),
            int(275 * (height / 858.0)),
            int(1085 * scale_x),
            int(300 * (height / 858.0)),
        ),
    }
    for label, box in default_text_boxes.items():
        default_lum, default_pixels = foreground_luminance(arr, box)
        if default_lum is None:
            return [f"traffic screenshot: insufficient {label} pixels for color check ({default_pixels})"]
        if default_lum - timestamp_lum < 35.0:
            return [
                f"traffic screenshot: {label} is not visibly brighter than muted timestamp "
                f"(timestamp={timestamp_lum:.1f}, {label}={default_lum:.1f})"
            ]

    return []


def graph_panel_intrusion_issues(image: Image.Image) -> list[str]:
    width, height = image.size
    mask = series_color_mask(image)

    # The connection panel should contain only neutral UI/text colors. A row of
    # graph-series color here means the SVG renderer lost terminal-cell alignment.
    x0 = int(width * 0.66)
    x1 = int(width * 0.92)
    y0 = int(height * 0.12)
    y1 = int(height * 0.27)
    region = mask[y0:y1, x0:x1]
    if region.size == 0:
        return ["graph screenshot: could not sample connection panel for graph intrusion check"]

    row_counts = region.sum(axis=1)
    row_threshold = max(6, int(region.shape[1] * 0.015))
    rows_over_threshold = int((row_counts > row_threshold).sum())
    max_row = int(row_counts.max())

    if rows_over_threshold > 0:
        return [
            "graph screenshot: graph series colors intrude into connection panel "
            f"(rows={rows_over_threshold}, max_row_pixels={max_row}, threshold={row_threshold})"
        ]

    return []


def first_text_group_start(mask: np.ndarray) -> int | None:
    column_counts = mask.sum(axis=0)
    threshold = max(2, int(mask.shape[0] * 0.12))
    columns = np.where(column_counts > threshold)[0]
    if columns.size == 0:
        return None

    groups: list[list[int]] = []
    for column in columns:
        x = int(column)
        if not groups or x - groups[-1][-1] > 3:
            groups.append([x])
        else:
            groups[-1].append(x)

    for group in groups:
        width = group[-1] - group[0] + 1
        if width >= 6:
            return group[0]

    return None


def graph_connection_alignment_issues(image: Image.Image) -> list[str]:
    arr = np.asarray(image.convert("RGB"), dtype=np.int16)
    height, width, _ = arr.shape
    r = arr[:, :, 0]
    g = arr[:, :, 1]
    b = arr[:, :, 2]

    neutral_text = (np.abs(r - g) < 5) & (np.abs(g - b) < 5) & (r > 80) & (r < 170)
    scale_y = height / 858.0
    scale_x = width / 1189.0
    x0 = int(790 * scale_x)
    x1 = int(900 * scale_x)
    starts: list[int] = []

    for baseline in (118.6, 135.4, 152.2, 169.0, 185.8, 202.6):
        y0 = max(0, int(baseline * scale_y - 11 * scale_y))
        y1 = min(height, int(baseline * scale_y + 2 * scale_y))
        start = first_text_group_start(neutral_text[y0:y1, x0:x1])
        if start is None:
            return ["graph screenshot: could not locate connection labels for alignment check"]
        starts.append(x0 + start)

    drift = max(starts) - min(starts)
    if drift > max(3, int(2 * scale_x)):
        return [f"graph screenshot: connection rows are horizontally misaligned (drift={drift}px)"]

    return []


def graph_legend_background_issues(image: Image.Image) -> list[str]:
    arr = np.asarray(image.convert("RGB"), dtype=np.int16)
    height, width, _ = arr.shape

    # Freeze can over-extend background rects for the legend row. The connection
    # panel should mostly retain the normal terminal background in this band.
    region = arr[int(height * 0.12) : int(height * 0.24), int(width * 0.66) : int(width * 0.92)]
    r = region[:, :, 0]
    g = region[:, :, 1]
    b = region[:, :, 2]
    legend_bg = (r >= 44) & (r <= 54) & (np.abs(r - g) < 3) & (np.abs(g - b) < 3)
    legend_bg_pixels = int(legend_bg.sum())
    if legend_bg_pixels > 10_000:
        return [
            "graph screenshot: legend background bleeds into connection panel "
            f"({legend_bg_pixels} pixels)"
        ]

    return []


def file_sender_progress_background_issues(image: Image.Image) -> list[str]:
    arr = np.asarray(image.convert("RGB"), dtype=np.int16)
    height, width, _ = arr.shape
    scale_y = height / 858.0
    scale_x = width / 1189.0
    issues: list[str] = []

    progress_region = arr[
        int(700 * scale_y) : int(755 * scale_y), int(80 * scale_x) : int(785 * scale_x)
    ]
    if progress_region.size == 0:
        issues.append("file sender screenshot: could not sample progress panel for border color check")
    else:
        pr = progress_region[:, :, 0]
        pg = progress_region[:, :, 1]
        pb = progress_region[:, :, 2]
        default_fg = (pr > 210) & (pg > 190) & (pb > 140)
        default_fg_pixels = int(default_fg.sum())
        if default_fg_pixels > 1_000:
            issues.append(
                "file sender screenshot: progress border/stats use default foreground "
                f"instead of muted gray ({default_fg_pixels} pixels)"
            )

    progress_bar = arr[
        int(713 * scale_y) : int(730 * scale_y), int(100 * scale_x) : int(775 * scale_x)
    ]
    if progress_bar.size == 0:
        issues.append("file sender screenshot: could not sample progress bar for unfilled visibility check")
    else:
        br = progress_bar[:, :, 0]
        bg = progress_bar[:, :, 1]
        bb = progress_bar[:, :, 2]
        visible_gray = (
            (br >= 90)
            & (br <= 155)
            & (np.abs(br - bg) < 6)
            & (np.abs(bg - bb) < 6)
        )
        visible_gray_pixels = int(visible_gray.sum())
        if visible_gray_pixels < 500:
            issues.append(
                "file sender screenshot: unfilled progress bar is not visibly distinct "
                f"({visible_gray_pixels} gray pixels)"
            )

    # Freeze can over-extend the progress gauge background past the left pane.
    # The control panel's send row should not inherit the gauge's #444 background.
    region = arr[int(708 * scale_y) : int(730 * scale_y), int(790 * scale_x) : int(1085 * scale_x)]
    if region.size == 0:
        issues.append("file sender screenshot: could not sample control panel for progress background check")
        return issues

    r = region[:, :, 0]
    g = region[:, :, 1]
    b = region[:, :, 2]
    gauge_bg = (r >= 62) & (r <= 76) & (np.abs(r - g) < 3) & (np.abs(g - b) < 3)
    gauge_bg_pixels = int(gauge_bg.sum())
    if gauge_bg_pixels > 1_000:
        issues.append(
            "file sender screenshot: progress background bleeds into control panel "
            f"({gauge_bg_pixels} pixels)"
        )

    label_bg = (r >= 110) & (r <= 145) & (np.abs(r - g) < 4) & (np.abs(g - b) < 4)
    label_bg_pixels = int(label_bg.sum())
    if label_bg_pixels > 5_000:
        issues.append(
            "file sender screenshot: percentage background bleeds into control panel "
            f"({label_bg_pixels} pixels)"
        )

    progress_fill = (r < 20) & (g > 90) & (b > 90)
    progress_fill_pixels = int(progress_fill.sum())
    if progress_fill_pixels > 1_000:
        issues.append(
            "file sender screenshot: progress fill bleeds into control panel "
            f"({progress_fill_pixels} pixels)"
        )

    return issues


def validate(path: Path) -> list[str]:
    try:
        image = Image.open(path)
        image.load()
    except Exception as exc:  # noqa: BLE001 - report image decode failures directly.
        return [f"cannot read image: {exc}"]

    arr = np.asarray(image.convert("RGB"))
    if arr.std() < 1.0:
        return ["image appears blank"]

    issues: list[str] = []
    for xs, ys in component_boxes(accent_mask(image)):
        failed, detail = diagonal_drift(xs, ys)
        if failed:
            issues.append(f"possible diagonal terminal-column drift: {detail}")

    if path.name == "traffic-config.png":
        issues.extend(traffic_timestamp_issues(image))
    if path.name == "graph-sensor.png":
        issues.extend(graph_panel_intrusion_issues(image))
        issues.extend(graph_connection_alignment_issues(image))
        issues.extend(graph_legend_background_issues(image))
    if path.name == "file-sender.png":
        issues.extend(file_sender_progress_background_issues(image))

    return issues


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("images", nargs="+", type=Path)
    args = parser.parse_args()

    failed = False
    for image in args.images:
        issues = validate(image)
        if issues:
            failed = True
            for issue in issues:
                print(f"{image}: {issue}")
        else:
            print(f"{image}: ok")

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
