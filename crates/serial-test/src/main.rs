//! Serial Test Utility
//!
//! Creates fake serial ports for testing the serial monitor TUI.
//!
//! Uses `socat` to create a PTY pair, then writes test data to one end
//! while you connect to the other end with the TUI.

use std::process::Stdio;
use std::time::Duration;
use std::{env, path::PathBuf};

use rand::{Rng, SeedableRng, rngs::StdRng};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    /// Random hex bytes
    Hex,
    /// Readable ASCII text
    Ascii,
    /// Simulated sensor data (key: value format)
    Sensor,
    /// Echo back received data
    Echo,
    /// Random UTF-8 strings with emojis and special characters
    Utf8,
    /// High-speed ASCII flood for stress testing
    Flood,
}

struct Args {
    mode: Mode,
    ready_file: Option<PathBuf>,
    seed: Option<u64>,
    interval_ms: Option<u64>,
    lines: Option<u64>,
}

fn print_usage() {
    eprintln!(
        r#"Serial Test Utility - Create fake serial ports for testing

USAGE:
    serial-test [MODE] [OPTIONS]

MODES:
    hex      Random hex bytes (default)
    ascii    Readable ASCII text lines  
    sensor   Simulated sensor data (temp, humidity, pressure)
    echo     Echo back any received data
    utf8     Random UTF-8 strings with emojis and special characters
    flood    High-speed ASCII flood for stress testing

OPTIONS:
    --ready-file <PATH>  Write the TUI-connectable PTY path to PATH
    --seed <N>           Use deterministic random data
    --interval-ms <N>    Override delay between writes where supported
    --lines <N>          Stop after N generated chunks/lines where supported

EXAMPLES:
    serial-test           # Start with random hex data
    serial-test sensor    # Start with sensor simulation
    serial-test echo      # Echo mode for testing TX
    serial-test utf8      # UTF-8 mode with emojis and special chars
    serial-test flood     # Stress test with high-speed data
    serial-test ascii --ready-file /tmp/serial-pty --seed 1 --interval-ms 50

The program will print the PTY path to connect to.
Press Ctrl+C to stop.
"#
    );
}

fn parse_mode(value: &str) -> Option<Mode> {
    match value {
        "hex" => Some(Mode::Hex),
        "ascii" => Some(Mode::Ascii),
        "sensor" => Some(Mode::Sensor),
        "echo" => Some(Mode::Echo),
        "utf8" => Some(Mode::Utf8),
        "flood" => Some(Mode::Flood),
        _ => None,
    }
}

fn parse_value<T: std::str::FromStr>(flag: &str, value: Option<String>) -> Option<T> {
    let value = match value {
        Some(value) => value,
        None => {
            eprintln!("Missing value for {}", flag);
            print_usage();
            return None;
        }
    };

    match value.parse() {
        Ok(value) => Some(value),
        Err(_) => {
            eprintln!("Invalid value for {}: {}", flag, value);
            print_usage();
            None
        }
    }
}

fn parse_args() -> Option<Args> {
    let mut mode = Mode::Hex;
    let mut ready_file = None;
    let mut seed = None;
    let mut interval_ms = None;
    let mut lines = None;
    let mut mode_set = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return None;
            }
            "--ready-file" => {
                ready_file = Some(PathBuf::from(match args.next() {
                    Some(value) => value,
                    None => {
                        eprintln!("Missing value for --ready-file");
                        print_usage();
                        return None;
                    }
                }));
            }
            "--seed" => seed = Some(parse_value("--seed", args.next())?),
            "--interval-ms" => interval_ms = Some(parse_value("--interval-ms", args.next())?),
            "--lines" => lines = Some(parse_value("--lines", args.next())?),
            value if value.starts_with("--") => {
                eprintln!("Unknown option: {}", value);
                print_usage();
                return None;
            }
            value => {
                if mode_set {
                    eprintln!("Unexpected argument: {}", value);
                    print_usage();
                    return None;
                }
                match parse_mode(value) {
                    Some(parsed) => {
                        mode = parsed;
                        mode_set = true;
                    }
                    None => {
                        eprintln!("Unknown mode: {}", value);
                        print_usage();
                        return None;
                    }
                }
            }
        }
    }

    Some(Args {
        mode,
        ready_file,
        seed,
        interval_ms,
        lines,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = match parse_args() {
        Some(args) => args,
        None => return Ok(()),
    };
    let mut rng = match args.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_os_rng(),
    };

    // Start socat to create PTY pair
    // socat outputs the PTY names to stderr
    let mut socat = Command::new("socat")
        .args([
            "-d",
            "-d", // Debug output to see PTY names
            "pty,raw,echo=0",
            "pty,raw,echo=0",
        ])
        .stderr(Stdio::piped())
        .spawn()?;

    let stderr = socat.stderr.take().expect("Failed to capture stderr");

    // Parse the PTY names from socat output
    // socat outputs lines like: "N PTY is /dev/pts/5"
    let mut reader = BufReader::new(stderr);
    let mut pty_paths: Vec<String> = Vec::new();

    // Read lines until we get both PTY paths
    let mut line = String::new();
    while pty_paths.len() < 2 {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }

        if let Some(path) = line
            .contains("PTY is")
            .then(|| line.split("PTY is ").nth(1))
            .flatten()
        {
            pty_paths.push(path.trim().to_string());
        }
    }

    if pty_paths.len() < 2 {
        eprintln!("Failed to get PTY paths from socat");
        return Ok(());
    }

    let our_pty = &pty_paths[0]; // We write to this one
    let their_pty = &pty_paths[1]; // TUI connects to this one

    println!("====================================");
    println!("  Serial Test Utility");
    println!("====================================");
    println!();
    println!("  Mode: {:?}", args.mode);
    println!("  Connect to: {}", their_pty);
    if let Some(path) = &args.ready_file {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, their_pty).await?;
        println!("  Ready file: {}", path.display());
    }
    println!();
    println!("  Press Ctrl+C to stop");
    println!("====================================");
    println!();

    // Open our end of the PTY
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(our_pty)
        .await?;

    let (reader, writer) = tokio::io::split(file);

    // Run the appropriate mode
    match args.mode {
        Mode::Echo => run_echo(reader, writer).await?,
        Mode::Hex => run_hex(writer, &mut rng, args.interval_ms, args.lines).await?,
        Mode::Ascii => run_ascii(writer, &mut rng, args.interval_ms, args.lines).await?,
        Mode::Sensor => run_sensor(writer, &mut rng, args.interval_ms, args.lines).await?,
        Mode::Utf8 => run_utf8(writer, &mut rng, args.interval_ms, args.lines).await?,
        Mode::Flood => run_flood(writer, &mut rng, args.lines).await?,
    }

    // Clean up
    socat.kill().await?;
    Ok(())
}

async fn run_echo(
    mut reader: tokio::io::ReadHalf<tokio::fs::File>,
    mut writer: tokio::io::WriteHalf<tokio::fs::File>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[echo] Waiting for data...");

    let mut buf = [0u8; 1024];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }

        // Print what we received
        let hex: String = buf[..n].iter().map(|b| format!("{:02X} ", b)).collect();
        println!("[echo] RX: {}", hex.trim());

        // Echo it back
        writer.write_all(&buf[..n]).await?;
        println!("[echo] TX: {}", hex.trim());
    }

    Ok(())
}

async fn run_hex(
    mut writer: tokio::io::WriteHalf<tokio::fs::File>,
    rng: &mut StdRng,
    interval_ms: Option<u64>,
    lines: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[hex] Sending random bytes...");

    let mut sent = 0;
    loop {
        if lines.is_some_and(|limit| sent >= limit) {
            return Ok(());
        }

        // Generate 8-32 random bytes
        let len: usize = rng.random_range(8..=32);
        let data: Vec<u8> = (0..len).map(|_| rng.random()).collect();

        let hex: String = data.iter().map(|b| format!("{:02X} ", b)).collect();
        println!("[hex] TX: {}", hex.trim());

        writer.write_all(&data).await?;
        sent += 1;

        // Wait 500ms-2s between chunks
        let delay = interval_ms.unwrap_or_else(|| rng.random_range(200..=1000));
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}

async fn run_ascii(
    mut writer: tokio::io::WriteHalf<tokio::fs::File>,
    rng: &mut StdRng,
    interval_ms: Option<u64>,
    lines: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[ascii] Sending text lines...");

    let text_lines = [
        "Hello, World!",
        "Serial Monitor Test",
        "The quick brown fox jumps over the lazy dog",
        "Lorem ipsum dolor sit amet",
        "Testing 1, 2, 3...",
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "0123456789",
        "!@#$%^&*()_+-=[]{}|;':\",./<>?",
        "Line with\ttabs\tand spaces",
        "End of transmission",
    ];

    let mut idx = 0;
    let mut sent = 0;

    loop {
        if lines.is_some_and(|limit| sent >= limit) {
            return Ok(());
        }

        let line = format!("{}\r\n", text_lines[idx]);
        println!("[ascii] TX: {}", text_lines[idx]);

        writer.write_all(line.as_bytes()).await?;
        sent += 1;

        idx = (idx + 1) % text_lines.len();

        // Wait 500ms-1.5s between lines
        let delay = interval_ms.unwrap_or_else(|| rng.random_range(10..=200));
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}

async fn run_sensor(
    mut writer: tokio::io::WriteHalf<tokio::fs::File>,
    rng: &mut StdRng,
    interval_ms: Option<u64>,
    lines: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[sensor] Simulating sensor data...");

    // Starting values
    let mut temp: f32 = 22.0;
    let mut humidity: f32 = 45.0;
    let mut pressure: f32 = 1013.25;
    let mut sent = 0;

    loop {
        if lines.is_some_and(|limit| sent >= limit) {
            return Ok(());
        }

        // Drift values slightly
        temp += rng.random_range(-0.5..=0.5);
        temp = temp.clamp(15.0, 35.0);

        humidity += rng.random_range(-2.0..=2.0);
        humidity = humidity.clamp(20.0, 80.0);

        pressure += rng.random_range(-1.0..=1.0);
        pressure = pressure.clamp(980.0, 1040.0);

        let line = format!(
            "temp:{:.1} humidity:{:.1} pressure:{:.2}\r\n",
            temp, humidity, pressure
        );

        println!(
            "[sensor] TX: temp:{:.1} humidity:{:.1} pressure:{:.2}",
            temp, humidity, pressure
        );

        writer.write_all(line.as_bytes()).await?;
        sent += 1;

        // Send every 1 second
        tokio::time::sleep(Duration::from_millis(interval_ms.unwrap_or(250))).await;
    }
}

async fn run_utf8(
    mut writer: tokio::io::WriteHalf<tokio::fs::File>,
    rng: &mut StdRng,
    interval_ms: Option<u64>,
    lines: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[utf8] Sending random UTF-8 strings...");

    // Emojis
    let emojis = [
        "😀", "😂", "🤣", "😊", "😍", "🥰", "😎", "🤔", "🤯", "😱", "🎉", "🎊", "🎁", "🎈", "🎯",
        "🚀", "🌟", "⭐", "✨", "💫", "❤️", "💙", "💚", "💛", "🧡", "💜", "🖤", "🤍", "💔", "💕",
        "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🦁", "🍎", "🍐", "🍊", "🍋", "🍌",
        "🍉", "🍇", "🍓", "🫐", "🍒", "🌍", "🌎", "🌏", "🌙", "☀️", "⛅", "🌈", "🔥", "💧", "❄️",
        "👍", "👎", "👋", "🤝", "🙏", "💪", "🦾", "🖖", "✌️", "🤞", "🏠", "🏡", "🏢", "🏭", "🏥",
        "🏦", "🏪", "🏫", "🏰", "🗼",
    ];

    // Various scripts and special characters
    let scripts = [
        // Latin with diacritics
        "Ñoño",
        "Ånström",
        "naïve",
        "café",
        "résumé",
        "über",
        "Müller",
        // Greek
        "Ελληνικά",
        "αβγδ",
        "ΩΨΧΦ",
        "πρόγραμμα",
        // Cyrillic
        "Привет",
        "Русский",
        "АБВГД",
        "мир",
        // Arabic
        "مرحبا",
        "العربية",
        "سلام",
        // Hebrew
        "שלום",
        "עברית",
        // Japanese (Hiragana, Katakana, Kanji)
        "こんにちは",
        "カタカナ",
        "日本語",
        "漢字",
        // Chinese
        "你好",
        "中文",
        "世界",
        // Korean
        "안녕하세요",
        "한국어",
        // Thai
        "สวัสดี",
        "ภาษาไทย",
        // Hindi/Devanagari
        "नमस्ते",
        "हिन्दी",
        // Tamil
        "வணக்கம்",
        // Emoji sequences
        "👨‍👩‍👧‍👦",
        "🏳️‍🌈",
        "👩‍💻",
        "🧑‍🚀",
    ];

    // Math and symbols
    let symbols = [
        "∀∃∄∅∆∇",
        "∈∉∊∋∌∍",
        "∏∐∑−∓∔",
        "√∛∜∝∞∟",
        "∠∡∢∣∤∥",
        "∧∨∩∪∫∬",
        "≠≡≢≣≤≥",
        "≦≧≨≩≪≫",
        "⊂⊃⊄⊅⊆⊇",
        "①②③④⑤",
        "⑥⑦⑧⑨⑩",
        "ⅠⅡⅢⅣⅤ",
        "←↑→↓↔↕",
        "↖↗↘↙↚↛",
        "⇐⇑⇒⇓⇔⇕",
        "♠♡♢♣♤♥",
        "♦♧★☆☉☊",
        "☎☏☐☑☒☓",
        "⌘⌥⌃⇧⏎⌫",
        "⎋⏏⏩⏪⏫⏬",
        "⏭⏮⏯⏰⏱⏲",
    ];

    // Box drawing and blocks
    let box_drawing = [
        "┌─┬─┐",
        "│ │ │",
        "├─┼─┤",
        "└─┴─┘",
        "╔═╦═╗",
        "║ ║ ║",
        "╠═╬═╣",
        "╚═╩═╝",
        "░▒▓█▄▀",
        "▁▂▃▄▅▆▇█",
        "▉▊▋▌▍▎▏",
        "◢◣◤◥",
        "●○◎◉◌",
        "■□▢▣▤▥",
    ];

    // Zalgo-style combining characters (be careful - these stack!)
    let combining = ["h̷e̸l̵l̶o̷", "ẅ̈ö̈r̈l̈d̈", "t̲e̲s̲t̲", "s̶t̶r̶i̶k̶e̶", "u͎n͎d͎e͎r͎"];

    // Full-width characters
    let fullwidth = ["ＦＵＬＬ　ＷＩＤＴＨ", "１２３４５", "ａｂｃｄｅ"];

    // Currency symbols
    let currency = ["$€£¥₹₽₿", "₩₪₫₭₮₯", "₰₱₲₳₴₵"];

    // Musical symbols
    let music = ["♩♪♫♬♭♮♯", "𝄞𝄢𝄪𝄫"];

    // Misc fun strings
    let misc = [
        "¯\\_(ツ)_/¯",
        "(╯°□°)╯︵ ┻━┻",
        "┬─┬ノ( º _ ºノ)",
        "( ͡° ͜ʖ ͡°)",
        "ʕ•ᴥ•ʔ",
        "（＾▽＾）",
        "٩(◕‿◕｡)۶",
        "☆*:.｡.o(≧▽≦)o.｡.:*☆",
        "♪(´ε` )",
        "The quick brown 🦊 jumps over the lazy 🐶",
        "Ťĥé qùíçk ḃŕöẃñ fôx",
        "🅣🅗🅔 🅠🅤🅘🅒🅚 🅑🅡🅞🅦🅝 🅕🅞🅧",
    ];

    let mut sent = 0;
    loop {
        if lines.is_some_and(|limit| sent >= limit) {
            return Ok(());
        }

        // Randomly select a category and item
        let category = rng.random_range(0..8);
        let line = match category {
            0 => {
                // Random emoji sequence (1-5 emojis)
                let count = rng.random_range(1..=5);
                let mut s = String::new();
                for _ in 0..count {
                    let idx = rng.random_range(0..emojis.len());
                    s.push_str(emojis[idx]);
                }
                s
            }
            1 => {
                let idx = rng.random_range(0..scripts.len());
                scripts[idx].to_string()
            }
            2 => {
                let idx = rng.random_range(0..symbols.len());
                symbols[idx].to_string()
            }
            3 => {
                let idx = rng.random_range(0..box_drawing.len());
                box_drawing[idx].to_string()
            }
            4 => {
                let idx = rng.random_range(0..combining.len());
                combining[idx].to_string()
            }
            5 => {
                let idx = rng.random_range(0..fullwidth.len());
                fullwidth[idx].to_string()
            }
            6 => {
                // Mix: currency + music
                let c_idx = rng.random_range(0..currency.len());
                let m_idx = rng.random_range(0..music.len());
                format!("{} {}", currency[c_idx], music[m_idx])
            }
            _ => {
                let idx = rng.random_range(0..misc.len());
                misc[idx].to_string()
            }
        };

        println!("[utf8] TX: {}", line);
        let output = format!("{}\r\n", line);
        writer.write_all(output.as_bytes()).await?;
        sent += 1;

        // Wait 200ms-800ms between lines
        let delay = interval_ms.unwrap_or_else(|| rng.random_range(200..=800));
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}

async fn run_flood(
    mut writer: tokio::io::WriteHalf<tokio::fs::File>,
    rng: &mut StdRng,
    lines: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[flood] Starting high-speed ASCII flood...");
    println!("[flood] WARNING: This will generate a LOT of data!");

    let mut line_count: u64 = 0;
    let mut byte_count: u64 = 0;
    let start = std::time::Instant::now();

    // Pre-generate some words for variety
    let words = [
        "the",
        "quick",
        "brown",
        "fox",
        "jumps",
        "over",
        "lazy",
        "dog",
        "hello",
        "world",
        "serial",
        "monitor",
        "test",
        "data",
        "flood",
        "alpha",
        "beta",
        "gamma",
        "delta",
        "epsilon",
        "zeta",
        "eta",
        "lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet",
        "consectetur",
        "async",
        "await",
        "rust",
        "tokio",
        "buffer",
        "stream",
        "port",
        "error",
        "warning",
        "info",
        "debug",
        "trace",
        "log",
        "message",
        "packet",
        "frame",
        "byte",
        "bit",
        "signal",
        "noise",
        "channel",
        "input",
        "output",
        "read",
        "write",
        "open",
        "close",
        "connect",
    ];

    // Buffer to batch writes for better throughput
    let mut buffer = String::with_capacity(8192);

    loop {
        buffer.clear();

        // Generate a batch of lines
        for _ in 0..100 {
            if lines.is_some_and(|limit| line_count >= limit) {
                break;
            }

            line_count += 1;

            // Generate a line with random words (5-15 words per line)
            let word_count = rng.random_range(5..=15);
            buffer.push_str(&format!("[{:08}] ", line_count));

            for i in 0..word_count {
                let idx = rng.random_range(0..words.len());
                buffer.push_str(words[idx]);
                if i < word_count - 1 {
                    buffer.push(' ');
                }
            }
            buffer.push_str("\r\n");
        }

        if buffer.is_empty() {
            return Ok(());
        }

        byte_count += buffer.len() as u64;
        writer.write_all(buffer.as_bytes()).await?;

        // Print stats every 10000 lines
        if line_count.is_multiple_of(10000) {
            let elapsed = start.elapsed().as_secs_f64();
            let lines_per_sec = line_count as f64 / elapsed;
            let kb_per_sec = (byte_count as f64 / 1024.0) / elapsed;
            println!(
                "[flood] {} lines, {:.1} KB ({:.0} lines/s, {:.1} KB/s)",
                line_count,
                byte_count as f64 / 1024.0,
                lines_per_sec,
                kb_per_sec
            );
        }

        // Tiny yield to prevent complete CPU starvation, but keep it fast
        tokio::task::yield_now().await;
    }
}
