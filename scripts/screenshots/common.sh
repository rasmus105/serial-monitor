#!/usr/bin/env bash
set -euo pipefail

SESSION="${SERIAL_MONITOR_TMUX_SESSION:-serial-monitor-test}"
WIDTH="${SERIAL_MONITOR_TMUX_WIDTH:-120}"
HEIGHT="${SERIAL_MONITOR_TMUX_HEIGHT:-40}"
PTY_FILE="/tmp/serial-monitor-screenshot-pty"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
OUTPUT_DIR="${SCREENSHOT_DIR:-${PROJECT_DIR}/docs/screenshots}"
SERIAL_TEST_PID=""
TUI_PANE_PID=""
SERIAL_TEST_LOG="${TMPDIR:-/tmp}/serial-monitor-screenshot-serial-test.log"
FREEZE_CONFIG="${SCREENSHOT_FREEZE_CONFIG:-full}"
FREEZE_THEME="${SCREENSHOT_FREEZE_THEME:-gruvbox}"
FREEZE_FONT_SIZE="${SCREENSHOT_FONT_SIZE:-14}"
FREEZE_LINE_HEIGHT="${SCREENSHOT_LINE_HEIGHT:-1.2}"
FREEZE_BACKGROUND="${SCREENSHOT_BACKGROUND:-#282828}"
CAPTURE_DELAY="${SCREENSHOT_CAPTURE_DELAY:-1}"
SCREENSHOT_RASTER_SCALE="${SCREENSHOT_RASTER_SCALE:-2}"
SCREENSHOT_FONT_CACHE_DIR="${XDG_CACHE_HOME:-${HOME}/.cache}/serial-monitor/screenshots/fonts"
SCREENSHOT_HACK_FONT_FILE="${SCREENSHOT_HACK_FONT_FILE:-${SCREENSHOT_FONT_CACHE_DIR}/HackNerdFontMono-Regular.ttf}"
SCREENSHOT_BRAILLE_FONT_FILE="${SCREENSHOT_BRAILLE_FONT_FILE:-${SCREENSHOT_FONT_CACHE_DIR}/JuliaMono-Regular.ttf}"
SCREENSHOT_FONT_FAMILY="${SCREENSHOT_FONT_FAMILY:-Hack Nerd Font Mono, JuliaMono}"

download_font_file() {
    local output="$1"
    local url="$2"

    if [ -f "${output}" ]; then
        return 0
    fi
    if [ "${SCREENSHOT_DOWNLOAD_FONT:-1}" = "0" ]; then
        return 1
    fi
    if ! command -v curl >/dev/null 2>&1; then
        return 1
    fi

    mkdir -p "$(dirname "${output}")"
    if curl -fsSL "${url}" -o "${output}.tmp"; then
        mv "${output}.tmp" "${output}"
        return 0
    fi

    rm -f "${output}.tmp"
    return 1
}

prepare_screenshot_font() {
    local font_file="${SCREENSHOT_HACK_FONT_FILE}"

    download_font_file \
        "${font_file}" \
        "https://github.com/ryanoasis/nerd-fonts/raw/master/patched-fonts/Hack/Regular/HackNerdFontMono-Regular.ttf" || true

    if [ ! -f "${font_file}" ]; then
        echo "Error: Hack Nerd Font not found: ${font_file}" >&2
        echo "Set SCREENSHOT_HACK_FONT_FILE or allow font download with SCREENSHOT_DOWNLOAD_FONT=1." >&2
        return 1
    fi

    export SCREENSHOT_FONT_FILE="${font_file}"
}

prepare_screenshot_fonts() {
    # Freeze's direct PNG converter can drift terminal columns with custom fonts.
    # Render SVG with Freeze, then rasterize through librsvg/fontconfig.
    if ! command -v rsvg-convert >/dev/null 2>&1; then
        echo "Error: screenshots need rsvg-convert for accurate monospaced rendering." >&2
        echo "Install librsvg, for example: brew install librsvg" >&2
        return 1
    fi

    prepare_screenshot_font

    local text_font="${SCREENSHOT_FONT_FILE}"
    # Hack Nerd Font is the primary terminal font, but it does not include
    # Braille cells. Use a monospaced fallback so graph rows keep terminal-cell
    # alignment when Freeze emits SVG text runs.
    local braille_font="${SCREENSHOT_GRAPH_BRAILLE_FONT_FILE:-${SCREENSHOT_BRAILLE_FONT_FILE}}"

    download_font_file \
        "${braille_font}" \
        "https://raw.githubusercontent.com/cormullion/juliamono/master/JuliaMono-Regular.ttf" || true

    if [ ! -f "${braille_font}" ]; then
        echo "Error: graph Braille font not found: ${braille_font}" >&2
        echo "Set SCREENSHOT_GRAPH_BRAILLE_FONT_FILE or allow font download with SCREENSHOT_DOWNLOAD_FONT=1." >&2
        return 1
    fi

    local fontconfig_dir="${TMPDIR:-/tmp}/serial-monitor-screenshot-fontconfig"
    mkdir -p "${fontconfig_dir}/fonts" "${fontconfig_dir}/cache"
    cp "${text_font}" "${fontconfig_dir}/fonts/"
    cp "${braille_font}" "${fontconfig_dir}/fonts/"

    SCREENSHOT_FONTCONFIG_FILE="${fontconfig_dir}/fonts.conf"
    cat >"${SCREENSHOT_FONTCONFIG_FILE}" <<EOF
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig>
  <dir>${fontconfig_dir}/fonts</dir>
  <cachedir>${fontconfig_dir}/cache</cachedir>
</fontconfig>
EOF

    if command -v fc-cache >/dev/null 2>&1; then
        FONTCONFIG_FILE="${SCREENSHOT_FONTCONFIG_FILE}" fc-cache -f >/dev/null 2>&1 || true
    fi

    unset SCREENSHOT_FONT_FILE
    export SCREENSHOT_FONTCONFIG_FILE
    export SCREENSHOT_FONT_FAMILY
}

prepare_graph_fonts() {
    prepare_screenshot_fonts
}

prepare_screenshot_font

patch_freeze_svg_grid() {
    local svg="$1"
    local python="${SCREENSHOT_PYTHON:-python3}"

    if ! command -v "${python}" >/dev/null 2>&1; then
        echo "Error: screenshots need python3 to patch Freeze SVG terminal-cell alignment." >&2
        return 1
    fi

    "${python}" "${SCRIPT_DIR}/patch-freeze-svg.py" "${svg}"
}

ensure_svg_clip_path() {
    local svg="$1"

    if ! command -v perl >/dev/null 2>&1; then
        echo "Error: screenshots need perl to patch Freeze SVG clipping before rasterization." >&2
        return 1
    fi

    perl -0pi -e '
        if (!/<clipPath\s+id="terminalMask"/ && /<rect width="([^"]+)" height="([^"]+)".*? x="([^"]+)" y="([^"]+)"/s) {
            my ($width, $height, $x, $y) = ($1, $2, $3, $4);
            my $clip = qq{<clipPath id="terminalMask"><rect x="$x" y="$y" width="$width" height="$height"/></clipPath>};
            s{</defs></svg>}{$clip</defs></svg>};
        }
    ' "${svg}"
}

# Use a temp HOME so the TUI starts with default settings
export SERIAL_MONITOR_TMUX_HOME="${TMPDIR:-/tmp}/serial-monitor-screenshot-home"

screenshot_tui_command() {
    if [ "${SCREENSHOT_BUILD_RELEASE:-1}" != "0" ] || [ ! -x "${PROJECT_DIR}/target/release/serial-tui" ]; then
        cargo build --release -p serial-tui >&2
    fi

    printf "%q" "${PROJECT_DIR}/target/release/serial-tui"
}

wait_for_tmux_session() {
    local waited=0
    while [ "${waited}" -lt 20 ]; do
        if tmux has-session -t "${SESSION}" 2>/dev/null; then
            return 0
        fi
        sleep 0.1
        waited=$((waited + 1))
    done

    echo "Error: tmux session '${SESSION}' exited before it could be driven." >&2
    echo "Try running: target/release/serial-tui" >&2
    return 1
}

start_tui() {
    "${SCRIPT_DIR}/../tmux/start" "$(screenshot_tui_command)"
    wait_for_tmux_session
    TUI_PANE_PID=$(tmux display-message -p -t "${SESSION}" '#{pane_pid}' 2>/dev/null || true)
}

# --- Cleanup ----------------------------------------------------------------

descendant_pids() {
    local parent="$1"
    local child

    if ! command -v pgrep >/dev/null 2>&1; then
        return 0
    fi

    while IFS= read -r child; do
        if [ -z "${child}" ]; then
            continue
        fi
        printf '%s\n' "${child}"
        descendant_pids "${child}"
    done < <(pgrep -P "${parent}" 2>/dev/null || true)
}

wait_for_pids_to_exit() {
    local waited=0
    local pid
    local alive

    while [ "${waited}" -lt 20 ]; do
        alive=0
        for pid in "$@"; do
            if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
                alive=1
                break
            fi
        done
        if [ "${alive}" -eq 0 ]; then
            return 0
        fi
        sleep 0.1
        waited=$((waited + 1))
    done

    return 1
}

terminate_pids() {
    if [ "$#" -eq 0 ]; then
        return 0
    fi

    local pid
    local pids=("$@")

    if [ "${#pids[@]}" -eq 0 ]; then
        return 0
    fi

    for pid in "${pids[@]}"; do
        if [ -n "${pid}" ]; then
            kill "${pid}" 2>/dev/null || true
        fi
    done

    if wait_for_pids_to_exit "${pids[@]}"; then
        return 0
    fi

    for pid in "${pids[@]}"; do
        if [ -n "${pid}" ]; then
            kill -KILL "${pid}" 2>/dev/null || true
        fi
    done
}

stop_tui() {
    local pane_pid="${TUI_PANE_PID}"
    local pid
    local pids=()
    local pid_count=0

    if tmux has-session -t "${SESSION}" 2>/dev/null; then
        if [ -z "${pane_pid}" ]; then
            pane_pid=$(tmux display-message -p -t "${SESSION}" '#{pane_pid}' 2>/dev/null || true)
        fi
        if [ -n "${pane_pid}" ]; then
            pids+=("${pane_pid}")
            pid_count=$((pid_count + 1))
            while IFS= read -r pid; do
                if [ -n "${pid}" ]; then
                    pids+=("${pid}")
                    pid_count=$((pid_count + 1))
                fi
            done < <(descendant_pids "${pane_pid}")
        fi

        tmux send-keys -t "${SESSION}" ':' quit Enter 2>/dev/null || true
        if [ "${pid_count}" -gt 0 ]; then
            wait_for_pids_to_exit "${pids[@]}" || true
        fi

        if tmux has-session -t "${SESSION}" 2>/dev/null; then
            tmux kill-session -t "${SESSION}" 2>/dev/null || true
        fi
    elif [ -n "${pane_pid}" ]; then
        pids+=("${pane_pid}")
        pid_count=$((pid_count + 1))
        while IFS= read -r pid; do
            if [ -n "${pid}" ]; then
                pids+=("${pid}")
                pid_count=$((pid_count + 1))
            fi
        done < <(descendant_pids "${pane_pid}")
    fi

    if [ "${pid_count}" -gt 0 ]; then
        terminate_pids "${pids[@]}"
    fi
    TUI_PANE_PID=""
}

cleanup() {
    stop_tui
    stop_serial_test
    rm -f "${PTY_FILE}"
    rm -f "${SERIAL_TEST_LOG}"
    rm -rf "${SERIAL_MONITOR_TMUX_HOME}"
    rm -rf "${TMPDIR:-/tmp}/serial-monitor-screenshot-fontconfig"
}

trap cleanup EXIT

# --- Helper: start serial-test ---------------------------------------------

stop_serial_test() {
    local pid
    local pids=()
    local pid_count=0

    if [ -n "${SERIAL_TEST_PID}" ] && kill -0 "${SERIAL_TEST_PID}" 2>/dev/null; then
        pids+=("${SERIAL_TEST_PID}")
        pid_count=$((pid_count + 1))
        while IFS= read -r pid; do
            if [ -n "${pid}" ]; then
                pids+=("${pid}")
                pid_count=$((pid_count + 1))
            fi
        done < <(descendant_pids "${SERIAL_TEST_PID}")

        if [ "${pid_count}" -gt 0 ]; then
            terminate_pids "${pids[@]}"
        fi
        wait "${SERIAL_TEST_PID}" 2>/dev/null || true
        SERIAL_TEST_PID=""
    fi
}

start_serial_test() {
    local mode="$1"
    shift
    rm -f "${PTY_FILE}"
    cargo build -p serial-test >&2
    screenshot_tui_command >/dev/null
    "${PROJECT_DIR}/target/debug/serial-test" "${mode}" --ready-file "${PTY_FILE}" "$@" >"${SERIAL_TEST_LOG}" 2>&1 &
    SERIAL_TEST_PID=$!
    local waited=0
    while [ ! -f "${PTY_FILE}" ] && [ "${waited}" -lt 50 ]; do
        sleep 0.1
        waited=$((waited + 1))
    done
    if [ ! -f "${PTY_FILE}" ]; then
        echo "Error: PTY file not created within timeout" >&2
        exit 1
    fi
    PTY=$(cat "${PTY_FILE}")
    echo "PTY: ${PTY}"
}

# --- Helper: capture pane and freeze ---------------------------------------

capture_and_freeze() {
    local filename="$1"
    local output="${OUTPUT_DIR}/${filename}"
    local freeze_output="${output}"
    local temp_dir=""
    local temp_svg=""
    local freeze_args=(
        -c "${FREEZE_CONFIG}"
        --font.size "${FREEZE_FONT_SIZE}"
        --line-height "${FREEZE_LINE_HEIGHT}"
        --background "${FREEZE_BACKGROUND}"
    )

    if [ -z "${SCREENSHOT_FONTCONFIG_FILE:-}" ] || [ ! -f "${SCREENSHOT_FONTCONFIG_FILE}" ]; then
        prepare_screenshot_fonts
    fi

    if [ -n "${SCREENSHOT_FONT_FAMILY:-}" ]; then
        freeze_args+=(--font.family "${SCREENSHOT_FONT_FAMILY}")
    elif [ -n "${SCREENSHOT_FONT_FILE:-}" ]; then
        if [ ! -f "${SCREENSHOT_FONT_FILE}" ]; then
            echo "Error: SCREENSHOT_FONT_FILE does not exist: ${SCREENSHOT_FONT_FILE}" >&2
            return 1
        fi
        freeze_args+=(--font.file "${SCREENSHOT_FONT_FILE}")
    fi

    freeze_args+=(--theme "${FREEZE_THEME}")

    if [ -n "${SCREENSHOT_FREEZE_ARGS:-}" ]; then
        # Intentionally split extra flags so callers can pass normal CLI-style options.
        # shellcheck disable=SC2206
        local extra_args=(${SCREENSHOT_FREEZE_ARGS})
        freeze_args+=("${extra_args[@]}")
    fi

    mkdir -p "${OUTPUT_DIR}"
    if [ "${output##*.}" = "png" ]; then
        temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/serial-monitor-screenshot.XXXXXX")"
        temp_svg="${temp_dir}/${filename}.svg"
        freeze_output="${temp_svg}"
    fi

    # Short pause so the TUI renders the final state. Individual scripts can
    # override this when capturing a short-lived intermediate state.
    sleep "${CAPTURE_DELAY}"
    if [ -n "${SCREENSHOT_FONTCONFIG_FILE:-}" ]; then
        tmux capture-pane -t "${SESSION}" -p -e -J | env FONTCONFIG_FILE="${SCREENSHOT_FONTCONFIG_FILE}" freeze "${freeze_args[@]}" -o "${freeze_output}"
    else
        tmux capture-pane -t "${SESSION}" -p -e -J | freeze "${freeze_args[@]}" -o "${freeze_output}"
    fi

    if [ -n "${temp_svg}" ]; then
        patch_freeze_svg_grid "${temp_svg}"
        ensure_svg_clip_path "${temp_svg}"
        env FONTCONFIG_FILE="${SCREENSHOT_FONTCONFIG_FILE}" rsvg-convert --zoom "${SCREENSHOT_RASTER_SCALE}" "${temp_svg}" -o "${output}"
        rm -rf "${temp_dir}"
    fi
    echo "Screenshot saved: ${output}"
}
