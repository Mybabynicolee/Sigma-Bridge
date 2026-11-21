#![allow(dead_code)]

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Once;
use std::time::Duration;

static ARM: Once = Once::new();

pub const Z: &str = "\x1b[0m";

pub const G: &str = "\x1b[1;32m";
pub const Y: &str = "\x1b[1;33m";
pub const R: &str = "\x1b[1;31m";

const ERASE: &str = "\r\x1b[2K";

static LIVE: AtomicBool = AtomicBool::new(false);
static MUTE: AtomicBool = AtomicBool::new(false);
static LINES: AtomicUsize = AtomicUsize::new(0);

pub fn arm() {
    ARM.call_once(enable_colour);
}

pub fn mute_live(mute: bool) {
    MUTE.store(mute, Ordering::Relaxed);
}

#[cfg(windows)]
fn enable_colour() {
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(which: u32) -> *mut core::ffi::c_void;
        fn GetConsoleMode(handle: *mut core::ffi::c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(handle: *mut core::ffi::c_void, mode: u32) -> i32;
    }

    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() {
            return;
        }
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return;
        }
        let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
    }
}

#[cfg(not(windows))]
fn enable_colour() {}

fn erase() {
    print!("{}", ERASE);
}

fn live_shown() -> bool {
    LIVE.load(Ordering::Relaxed) && std::io::stdout().is_terminal()
}

pub fn emit(color: &str, msg: &str) {
    if live_shown() && !MUTE.load(Ordering::Relaxed) {
        erase();
        LIVE.store(false, Ordering::Relaxed);
    }
    println!("{}{}{}", color, msg, Z);
    LINES.fetch_add(1, Ordering::Relaxed);
    let _ = std::io::stdout().flush();
}

pub fn live(color: &str, msg: &str) {
    if std::io::stdout().is_terminal() {
        if LIVE.load(Ordering::Relaxed) {
            erase();
        }
        if !MUTE.load(Ordering::Relaxed) {
            print!("{}{}{}", color, msg, Z);
            let _ = std::io::stdout().flush();
        }
        LIVE.store(true, Ordering::Relaxed);
    } else {
        println!("{}{}{}", color, msg, Z);
        LINES.fetch_add(1, Ordering::Relaxed);
        let _ = std::io::stdout().flush();
    }
}

pub fn finish_live() {
    if live_shown() {
        LIVE.store(false, Ordering::Relaxed);
        println!();
        let _ = std::io::stdout().flush();
    }
}

pub fn clear_live() {
    if live_shown() {
        erase();
        LIVE.store(false, Ordering::Relaxed);
        let _ = std::io::stdout().flush();
    }
}

pub fn blank() {
    if live_shown() {
        erase();
        LIVE.store(false, Ordering::Relaxed);
    }
    println!();
    let _ = std::io::stdout().flush();
}

pub fn lg(msg: &str) {
    emit(G, msg);
}

pub fn ly(msg: &str) {
    emit(Y, msg);
}

pub fn lr(msg: &str) {
    emit(R, msg);
}

pub fn gap() {
    blank();
}

pub fn status(msg: &str) {
    live(Y, msg);
}

pub fn clear_status() {
    clear_live();
}

pub fn finish_status() {
    finish_live();
}

pub fn squeeze(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut sp = false;
    for c in input.chars() {
        if c.is_control() || c == '\u{feff}' {
            if c == '\n' || c == '\r' || c == '\t' {
                sp = true;
            }
            continue;
        }
        if c.is_whitespace() {
            sp = true;
            continue;
        }
        if sp && !out.is_empty() {
            out.push(' ');
        }
        sp = false;
        out.push(c);
    }
    out
}

pub fn clean_text(value: &str, fallback: &str) -> String {
    let mut text = String::with_capacity(value.len());
    for c in value.chars() {
        if "[]|#!@$%^&*()-".contains(c) {
            text.push(' ');
        } else {
            text.push(c);
        }
    }
    let text = squeeze(&text);
    if text.is_empty() {
        fallback.to_string()
    } else {
        text
    }
}

pub fn clock(seconds: u64) -> String {
    let days = seconds / 86_400;
    let rest = seconds % 86_400;
    let hours = rest / 3_600;
    let minutes = (rest % 3_600) / 60;
    let secs = rest % 60;
    if days > 0 {
        format!("{}d {:02}:{:02}:{:02}", days, hours, minutes, secs)
    } else {
        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
    }
}

pub fn num(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let text = format!("{:.4}", value);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-" {
        return "0".to_string();
    }
    text.to_string()
}

pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= crate::support::consts::KEY_HIDE_MIN {
        return "*".repeat(chars.len());
    }
    let head: String = chars[..crate::support::consts::KEY_HEAD].iter().collect();
    let tail: String = chars[chars.len() - crate::support::consts::KEY_TAIL..]
        .iter()
        .collect();
    let hidden = chars.len() - crate::support::consts::KEY_HEAD - crate::support::consts::KEY_TAIL;
    format!("{}{}{}", head, "*".repeat(hidden), tail)
}

pub fn mask_uuid(uuid: &str) -> String {
    let chars: Vec<char> = uuid.chars().collect();
    if chars.len() <= 8 {
        return "*".repeat(chars.len());
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{}****{}", head, tail)
}

pub fn nonce(len: usize) -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

pub async fn countdown(seconds: u64, label: &str) {
    if seconds == 0 {
        return;
    }
    let text = clean_text(label, "Next cycle starts in");
    if !std::io::stdout().is_terminal() {
        ly(&format!("{} {}", text, clock(seconds)));
        tokio::time::sleep(Duration::from_secs(seconds)).await;
        return;
    }
    let mut left = seconds;
    while left > 0 {
        status(&format!("{} {}", text, clock(left)));
        tokio::time::sleep(Duration::from_secs(1)).await;
        left -= 1;
    }
    clear_status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_drops_banned_symbols() {
        assert_eq!(clean_text("Ad Task #1", "task"), "Ad Task 1");
        assert_eq!(clean_text("###", "task 7"), "task 7");
    }

    #[test]
    fn squeeze_drops_control_chars() {
        assert_eq!(squeeze("  task\tlist\nok  "), "task list ok");
    }

    #[test]
    fn clock_uses_two_shapes() {
        assert_eq!(clock(3_723), "01:02:03");
        assert_eq!(clock(90_061), "1d 01:01:01");
    }

    #[test]
    fn num_trims_the_tail_like_the_canon() {
        assert_eq!(num(0.0), "0");
        assert_eq!(num(12.5), "12.5");
        assert_eq!(num(1.234567), "1.2346");
        assert_eq!(num(f64::NAN), "0");
    }

    #[test]
    fn the_key_keeps_a_head_and_a_tail_only() {
        let text = mask_key("aaaabbbbccccddddeeeeffffgggghhhh");
        assert!(text.starts_with("aaaabbbb"));
        assert!(text.ends_with("ffgggghhhh"));
        assert!(text.contains("**************"));
    }

    #[test]
    fn a_short_key_is_masked_whole() {
        assert_eq!(mask_key("aaaabbbb"), "********");
    }

    #[test]
    fn the_uuid_keeps_only_the_ends() {
        assert_eq!(
            mask_uuid("3f0a1c7e-5b24-4d81-9a6f-2c8e7b4105dd"),
            "3f0a****05dd"
        );
    }

    #[test]
    fn the_ansi_reset_is_the_canon_one() {
        assert_eq!(Z, "\x1b[0m");
    }
}
