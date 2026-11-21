use std::path::PathBuf;

use crate::cli::{probe, selftest};
use crate::run::serve;

pub const DEFAULT_MODE: &str = "serve";

pub fn usage() -> String {
    "Usage: sigmabridge [serve|probe|selftest|tokenchat] [--root DIR]".to_string()
}

pub async fn dispatch(args: &[String]) -> Result<(), String> {
    let mode = mode_of(args);
    let root = root_of(args);

    match mode.as_str() {
        "serve" | "run" => serve::serve(&root).await.map_err(|err| err.to_string()),
        "probe" => probe::run(&root).await.map_err(|err| err.to_string()),
        "tokenchat" => {
            let access = probe::access_from_env()
                .ok_or_else(|| format!("{} is not set", probe::ACCESS_ENV))?;
            let text = probe::chat_text();
            probe::token_chat(&access, &text)
                .await
                .map_err(|err| err.to_string())
        }
        "selftest" => selftest::run().await.map_err(|err| err.to_string()),
        "help" => {
            println!("{}", usage());
            Ok(())
        }
        other => Err(format!("unknown mode {}\n{}", other, usage())),
    }
}

pub fn mode_of(args: &[String]) -> String {
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if arg == "--root" || arg == "-C" {
            skip = true;
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        return arg.clone();
    }
    DEFAULT_MODE.to_string()
}

pub fn root_of(args: &[String]) -> PathBuf {
    for (index, arg) in args.iter().enumerate() {
        if arg == "--root" || arg == "-C" {
            if let Some(found) = args.get(index + 1) {
                return PathBuf::from(found);
            }
        }
        if let Some(found) = arg.strip_prefix("--root=") {
            return PathBuf::from(found);
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_default_mode_is_serve() {
        assert_eq!(mode_of(&args(&[])), "serve");
        assert_eq!(DEFAULT_MODE, "serve");
    }

    #[test]
    fn the_mode_survives_the_root_flag() {
        assert_eq!(mode_of(&args(&["probe", "--root", "/srv/bot"])), "probe");
        assert_eq!(mode_of(&args(&["--root", "/srv/bot", "probe"])), "probe");
    }

    #[test]
    fn the_root_flag_is_read_both_ways() {
        assert_eq!(
            root_of(&args(&["x", "--root", "/srv/bot"])),
            PathBuf::from("/srv/bot")
        );
        assert_eq!(
            root_of(&args(&["x", "--root=/srv/bot"])),
            PathBuf::from("/srv/bot")
        );
        assert_eq!(
            root_of(&args(&["x", "-C", "/srv/bot"])),
            PathBuf::from("/srv/bot")
        );
    }

    #[test]
    fn without_a_root_flag_the_cwd_is_used() {
        let found = root_of(&args(&["serve"]));
        assert!(found.is_absolute() || found == PathBuf::from("."));
    }

    #[test]
    fn the_usage_line_is_stable() {
        assert_eq!(
            usage(),
            "Usage: sigmabridge [serve|probe|selftest|tokenchat] [--root DIR]"
        );
    }

    #[tokio::test]
    async fn the_help_mode_prints_and_stops() {
        assert!(dispatch(&args(&["help"])).await.is_ok());
    }

    #[tokio::test]
    async fn an_unknown_mode_is_refused() {
        let err = dispatch(&args(&["wat"])).await.unwrap_err();
        assert!(err.contains("unknown mode"));
    }
}
