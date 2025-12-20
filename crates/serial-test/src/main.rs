//! Serial Test Utility
//!
//! Creates fake serial ports for testing the serial monitor TUI.
//!
//! Uses `socat` to create a PTY pair, then writes test data to one end
//! while you connect to the other end with the TUI.

use std::env;
use std::process::Stdio;
use std::time::Duration;

use rand::Rng;
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
}

fn print_usage() {
    eprintln!(
        r#"Serial Test Utility - Create fake serial ports for testing

USAGE:
    serial-test [MODE]

MODES:
    hex      Random hex bytes (default)
    ascii    Readable ASCII text lines  
    sensor   Simulated sensor data (temp, humidity, pressure)
    echo     Echo back any received data

EXAMPLES:
    serial-test           # Start with random hex data
    serial-test sensor    # Start with sensor simulation
    serial-test echo      # Echo mode for testing TX

The program will print the PTY path to connect to.
Press Ctrl+C to stop.
"#
    );
}

fn parse_args() -> Option<Mode> {
    let args: Vec<String> = env::args().collect();

    if args.len() > 2 {
        print_usage();
        return None;
    }

    if args.len() == 2 {
        match args[1].as_str() {
            "hex" => Some(Mode::Hex),
            "ascii" => Some(Mode::Ascii),
            "sensor" => Some(Mode::Sensor),
            "echo" => Some(Mode::Echo),
            "-h" | "--help" => {
                print_usage();
                None
            }
            other => {
                eprintln!("Unknown mode: {}", other);
                print_usage();
                None
            }
        }
    } else {
        Some(Mode::Hex) // Default
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = match parse_args() {
        Some(m) => m,
        None => return Ok(()),
    };

    // Start socat to create PTY pair
    // socat outputs the PTY names to stderr
    let mut socat = Command::new("socat")
        .args([
            "-d", "-d", // Debug output to see PTY names
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
    println!("  Mode: {:?}", mode);
    println!("  Connect to: {}", their_pty);
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
    match mode {
        Mode::Echo => run_echo(reader, writer).await?,
        Mode::Hex => run_hex(writer).await?,
        Mode::Ascii => run_ascii(writer).await?,
        Mode::Sensor => run_sensor(writer).await?,
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
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[hex] Sending random bytes...");

    let mut rng = rand::rng();
    loop {
        // Generate 8-32 random bytes
        let len: usize = rng.random_range(8..=32);
        let data: Vec<u8> = (0..len).map(|_| rng.random()).collect();

        let hex: String = data.iter().map(|b| format!("{:02X} ", b)).collect();
        println!("[hex] TX: {}", hex.trim());

        writer.write_all(&data).await?;

        // Wait 500ms-2s between chunks
        let delay = rng.random_range(500..=2000);
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}

async fn run_ascii(
    mut writer: tokio::io::WriteHalf<tokio::fs::File>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[ascii] Sending text lines...");

    let lines = [
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

    let mut rng = rand::rng();
    let mut idx = 0;

    loop {
        let line = format!("{}\r\n", lines[idx]);
        println!("[ascii] TX: {}", lines[idx]);

        writer.write_all(line.as_bytes()).await?;

        idx = (idx + 1) % lines.len();

        // Wait 500ms-1.5s between lines
        let delay = rng.random_range(500..=1500);
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}

async fn run_sensor(
    mut writer: tokio::io::WriteHalf<tokio::fs::File>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[sensor] Simulating sensor data...");

    let mut rng = rand::rng();

    // Starting values
    let mut temp: f32 = 22.0;
    let mut humidity: f32 = 45.0;
    let mut pressure: f32 = 1013.25;

    loop {
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

        // Send every 1 second
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
