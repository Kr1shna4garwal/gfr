//! # gfr: Grep-Find-Rust
//! A blazingly-fast, pure Rust tool for finding patterns in code.
//! This is a single-file implementation containing all logic and tests.

// --- Pre-Linter Configuration ---
#![deny(dead_code)]
#![deny(unused_imports)]

// --- Crate Imports ---
use std::fs::{self, File};
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::exit;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use grep_printer::{ColorSpecs, StandardBuilder};
use grep_regex::{RegexMatcherBuilder};
use grep_searcher::{Searcher};
use ignore::{WalkBuilder, WalkState};
use owo_colors::{OwoColorize, Style};
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, StandardStream};

// --- CLI Definition ---

/// A blazingly-fast, pure Rust tool for finding patterns based on predefined JSON files.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Search for a pattern in files or stdin
    Search {
        /// The name of the pattern to search for (without .json)
        pattern_name: String,

        /// File or directory path to search. Defaults to current directory.
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Show search configuration and exit without searching
        #[arg(long, short)]
        dump: bool,
    },
    /// List all available patterns
    List,
    /// Save a new pattern interactively
    Save {
        /// The name for the new pattern (e.g., "xss")
        name: String,
        /// The regular expression to search for
        pattern: String,
        /// Description of what the pattern finds
        #[arg(long, short)]
        description: Option<String>,
        /// List of case-insensitive file extensions to search (e.g., "js,html,ts")
        #[arg(long, short, value_delimiter = ',')]
        file_types: Option<Vec<String>>,
        /// Make the search case-insensitive
        #[arg(long, short = 'i')]
        ignore_case: bool,
        /// Enable multi-line searching
        #[arg(long, short = 'm')]
        multiline: bool,
    },
}

// --- Pattern Structure Definition ---

/// Represents a search pattern configuration loaded from a JSON file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pattern {
    /// A brief explanation of the pattern.
    description: Option<String>,
    /// A single regular expression. Use this or `patterns`.
    pattern: Option<String>,
    /// A list of regular expressions. These will be combined into a single pattern.
    patterns: Option<Vec<String>>,
    /// A list of file extensions to specifically include in the search.
    file_types: Option<Vec<String>>,
    /// If true, the search will be case-insensitive.
    #[serde(default)]
    ignore_case: bool,
    /// If true, enables multi-line searching.
    #[serde(default)]
    multiline: bool,
}

impl Pattern {
    /// Combines `pattern` and `patterns` fields into a single regex string,
    /// prepending flags for case-insensitivity and multiline matching.
    fn get_combined_pattern(&self) -> Result<String> {
        let mut flags = String::new();
        if self.ignore_case {
            flags.push('i');
        }
        if self.multiline {
            flags.push('s'); // 's' flag enables "dot all" mode, making `.` match newlines
        }

        let final_pattern = match (&self.pattern, &self.patterns) {
            (Some(p), None) => p.clone(),
            (None, Some(ps)) if !ps.is_empty() => format!("({})", ps.join("|")),
            _ => return Err(anyhow!(
                "Pattern file must contain either a single 'pattern' key or a non-empty list of 'patterns'."
            )),
        };

        if flags.is_empty() {
            Ok(final_pattern)
        } else {
            Ok(format!("(?{}){}", flags, final_pattern))
        }
    }
}

// --- Main Application Logic ---

fn main() -> Result<()> {
    let cli = Cli::parse();
    let styles = Styles::new();

    let result = match cli.command {
        Commands::Search {
            pattern_name,
            path,
            dump,
        } => {
            if dump {
                run_dump(&pattern_name, &styles)
            } else {
                run_search(&pattern_name, &path, &styles)
            }
        }
        Commands::List => run_list(&styles),
        Commands::Save {
            name,
            pattern,
            description,
            file_types,
            ignore_case,
            multiline,
        } => run_save(
            name,
            pattern,
            description,
            file_types,
            ignore_case,
            multiline,
            &styles,
        ),
    };

    if let Err(e) = result {
        eprintln!("{} {:#}", styles.error.style("Error:"), e);
        exit(1);
    }

    Ok(())
}

// --- Command Handlers ---

/// Executes the search operation.
fn run_search(pattern_name: &str, path: &Path, styles: &Styles) -> Result<()> {
    let pattern = load_pattern(pattern_name).with_context(|| {
        format!(
            "Failed to load pattern '{}'. Try '{}' to see available patterns.",
            pattern_name.style(styles.highlight),
            "gfr list".style(styles.highlight)
        )
    })?;

    let regex_pattern = pattern.get_combined_pattern()?;
    let matcher = RegexMatcherBuilder::new()
        .line_terminator(Some(b'\n'))
        .build(&regex_pattern)?;

    if !io::stdin().is_terminal() {
        // Search stdin
        let mut printer = StandardBuilder::new()
            .color_specs(ColorSpecs::default_with_color())
            .build(StandardStream::stdout(ColorChoice::Auto));
        let mut searcher = Searcher::new();
        searcher.search_reader(&matcher, io::stdin(), printer.sink(&matcher))?;
    } else {
        // Search file system
        let mut walk_builder = WalkBuilder::new(path);
        walk_builder.add_custom_ignore_filename(".gfrignore");

        if let Some(file_types) = &pattern.file_types {
            if !file_types.is_empty() {
                let mut type_builder = ignore::types::TypesBuilder::new();
                for ft in file_types {
                    type_builder.add(&format!("type_{}", ft), &format!("*.{}", ft))?;
                    type_builder.select(&format!("type_{}", ft));
                }
                let types = type_builder.build()?;
                walk_builder.types(types);
            }
        }

        walk_builder.build_parallel().run(|| {
            // Each thread gets its own clone of the matcher and a new searcher/printer.
            let matcher = matcher.clone();
            let mut searcher = Searcher::new();
            let mut printer = StandardBuilder::new()
                .color_specs(ColorSpecs::default_with_color())
                .build(StandardStream::stdout(ColorChoice::Auto));

            Box::new(move |result| {
                let entry = match result {
                    Ok(entry) => entry,
                    Err(err) => {
                        eprintln!("{}", err);
                        return WalkState::Continue;
                    }
                };
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    let result = searcher.search_path(
                        &matcher,
                        entry.path(),
                        printer.sink_with_path(&matcher, entry.path()),
                    );
                    if let Err(e) = result {
                        eprintln!(
                            "{}: {}",
                            entry.path().display().style(styles.error),
                            e.style(styles.error)
                        );
                    }
                }
                WalkState::Continue
            })
        });
    }

    Ok(())
}

/// Lists all available patterns in the configuration directory.
fn run_list(styles: &Styles) -> Result<()> {
    println!("{}", "Available patterns:".style(styles.title));
    let pattern_dir = get_pattern_dir()?;
    if !pattern_dir.exists() {
        println!(
            "  {}",
            "No pattern directory found. Save a pattern to create it.".style(styles.dim)
        );
        return Ok(());
    }

    let mut patterns: Vec<_> = fs::read_dir(&pattern_dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().extension().map_or(false, |ext| ext == "json")
        })
        .map(|entry| {
            let path = entry.path();
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let pattern = load_pattern(&name).ok();
            (name, pattern)
        })
        .collect();

    if patterns.is_empty() {
        println!(
            "  {}",
            "No patterns found. Use 'gfr save' to add one.".style(styles.dim)
        );
        return Ok(());
    }

    patterns.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, pattern) in patterns {
        if let Some(p) = pattern {
            let description = p.description.unwrap_or_else(|| "No description".into());
            println!(
                "  {} - {}",
                name.style(styles.highlight),
                description.style(styles.dim)
            );
        } else {
            println!("  {} - {}", name.style(styles.error), "Invalid pattern file");
        }
    }

    Ok(())
}

/// Saves a new pattern to a JSON file.
fn run_save(
    name: String,
    pattern_str: String,
    description: Option<String>,
    file_types: Option<Vec<String>>,
    ignore_case: bool,
    multiline: bool,
    styles: &Styles,
) -> Result<()> {
    let pattern_dir = get_pattern_dir()?;
    fs::create_dir_all(&pattern_dir)
        .with_context(|| format!("Failed to create pattern directory at {:?}", &pattern_dir))?;

    let pattern_file_path = pattern_dir.join(format!("{}.json", name));

    if pattern_file_path.exists() {
        return Err(anyhow!(
            "Pattern '{}' already exists at {}",
            name.style(styles.highlight),
            pattern_file_path.display().style(styles.dim)
        ));
    }

    let new_pattern = Pattern {
        description,
        pattern: Some(pattern_str),
        patterns: None,
        file_types,
        ignore_case,
        multiline,
    };

    let file = File::create(&pattern_file_path)
        .with_context(|| format!("Failed to create file: {:?}", &pattern_file_path))?;

    serde_json::to_writer_pretty(file, &new_pattern)
        .context("Failed to write JSON to pattern file.")?;

    println!(
        "{} Pattern '{}' saved to {}",
        "✓".style(styles.success),
        name.style(styles.highlight),
        pattern_file_path.display().style(styles.dim)
    );

    Ok(())
}

/// Prints the configuration of a pattern without executing a search.
fn run_dump(pattern_name: &str, styles: &Styles) -> Result<()> {
    let pattern = load_pattern(pattern_name)
        .with_context(|| format!("Failed to load pattern '{}'", pattern_name))?;

    let pattern_path = get_pattern_dir()?.join(format!("{}.json", pattern_name));

    println!(
        "{}",
        format!("Configuration for '{}'", pattern_name).style(styles.title)
    );
    println!(
        "{} {}",
        "Loaded from:".style(styles.dim),
        pattern_path.display()
    );
    println!("---");

    let json_string = serde_json::to_string_pretty(&pattern)?;
    println!("{}", json_string.style(styles.highlight));

    Ok(())
}

// --- Utility Functions ---

/// Retrieves the platform-specific configuration directory for gfr patterns.
fn get_pattern_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow!("Could not determine user's config directory."))?;
    Ok(config_dir.join("gfr"))
}

/// Loads and deserializes a pattern JSON file by name.
fn load_pattern(name: &str) -> Result<Pattern> {
    let pattern_dir = get_pattern_dir()?;
    let pattern_file = pattern_dir.join(format!("{}.json", name));
    if !pattern_file.exists() {
        return Err(anyhow!("Pattern file not found: {}", pattern_file.display()));
    }
    let file = File::open(&pattern_file)
        .with_context(|| format!("Failed to open pattern file: {}", pattern_file.display()))?;
    let pattern: Pattern = serde_json::from_reader(file)
        .with_context(|| format!("Failed to parse JSON from: {}", pattern_file.display()))?;
    Ok(pattern)
}

// --- Output Styling ---

/// A struct to hold styles for consistent terminal output coloring.
struct Styles {
    error: Style,
    success: Style,
    highlight: Style,
    dim: Style,
    title: Style,
}

impl Styles {
    fn new() -> Self {
        Self {
            error: Style::new().red().bold(),
            success: Style::new().green(),
            highlight: Style::new().yellow().bold(),
            dim: Style::new().dimmed(),
            title: Style::new().bold().underline(),
        }
    }
}

// --- Unit & Integration Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use grep_searcher::{Sink, SinkMatch};

    /// Helper to set up a temporary directory for pattern files.
    fn setup_test_env() -> Result<PathBuf> {
        let temp_dir = tempdir()?.path().to_path_buf();
        Ok(temp_dir)
    }

    /// A version of get_pattern_dir for testing purposes.
    fn get_test_pattern_dir(base: &Path) -> Result<PathBuf> {
        let test_dir = base.join(".config/gfr");
        fs::create_dir_all(&test_dir)?;
        Ok(test_dir)
    }

    #[test]
    fn test_save_and_load_pattern() -> Result<()> {
        let base_dir = setup_test_env()?;
        let pattern_dir = get_test_pattern_dir(&base_dir)?;
        
        let pattern_name = "test-pattern".to_string();
        let pattern_content = "test_pattern_regex".to_string();

        // Mock the `run_save` function's core logic
        let pattern_file_path = pattern_dir.join(format!("{}.json", &pattern_name));
        let new_pattern = Pattern {
            description: Some("A test pattern".to_string()),
            pattern: Some(pattern_content.clone()),
            patterns: None,
            file_types: Some(vec!["rs".to_string(), "toml".to_string()]),
            ignore_case: true,
            multiline: false,
        };
        let file = File::create(&pattern_file_path)?;
        serde_json::to_writer_pretty(file, &new_pattern)?;

        // Now, mock the `load_pattern` logic by overriding the config dir
        let file = File::open(&pattern_file_path)?;
        let loaded_pattern: Pattern = serde_json::from_reader(file)?;
        
        assert_eq!(loaded_pattern.pattern, Some(pattern_content));
        assert_eq!(loaded_pattern.description, Some("A test pattern".to_string()));
        assert_eq!(loaded_pattern.file_types, Some(vec!["rs".to_string(), "toml".to_string()]));
        assert!(loaded_pattern.ignore_case);

        Ok(())
    }

    #[test]
    fn test_get_combined_pattern_logic() {
        let p1 = Pattern {
            description: None, pattern: Some("abc".to_string()), patterns: None,
            file_types: None, ignore_case: false, multiline: false,
        };
        assert_eq!(p1.get_combined_pattern().unwrap(), "abc");

        let p2 = Pattern {
            description: None, pattern: None, patterns: Some(vec!["a".to_string(), "b".to_string()]),
            file_types: None, ignore_case: false, multiline: false,
        };
        assert_eq!(p2.get_combined_pattern().unwrap(), "(a|b)");

        let p3 = Pattern {
            description: None, pattern: Some("abc".to_string()), patterns: None,
            file_types: None, ignore_case: true, multiline: true,
        };
        assert_eq!(p3.get_combined_pattern().unwrap(), "(?is)abc");

        let p4 = Pattern {
            description: None, pattern: None, patterns: Some(vec!["a".to_string()]),
            file_types: None, ignore_case: true, multiline: false,
        };
        assert_eq!(p4.get_combined_pattern().unwrap(), "(?i)(a)");
    }

    /// A sink that captures matches into a vector for testing.
    #[derive(Debug)]
    struct TestSink {
        matches: Arc<Mutex<Vec<String>>>,
    }

    impl Sink for TestSink {
        type Error = io::Error;
        fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, io::Error> {
            let line = std::str::from_utf8(mat.lines().next().unwrap()).unwrap().trim().to_string();
            self.matches.lock().unwrap().push(line);
            Ok(true)
        }
    }

    #[test]
    fn test_searcher_logic() -> Result<()> {
        let content = "hello world\nthis is a test\nHELLO AGAIN\n";
        let matcher = RegexMatcherBuilder::new().case_insensitive(true).build("hello")?;
        
        let matches = Arc::new(Mutex::new(Vec::new()));
        let mut sink = TestSink { matches: matches.clone() };
        
        Searcher::new().search_reader(&matcher, content.as_bytes(), &mut sink)?;
        
        let found = matches.lock().unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], "hello world");
        assert_eq!(found[1], "HELLO AGAIN");
        Ok(())
    }
}
