//! # gfr: Grep-Find-Rust
//! A blazingly-fast, Rust-powered tool for finding patterns.

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::exit;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use grep_printer::{ColorSpecs, StandardBuilder};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use ignore::{WalkBuilder, WalkState};
use owo_colors::{OwoColorize, Style};
use semver::Version;
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, StandardStream};

fn get_color_choice() -> ColorChoice {
    if io::stdout().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    }
}

fn get_color_specs() -> ColorSpecs {
    if io::stdout().is_terminal() {
        ColorSpecs::default_with_color()
    } else {
        // Create a ColorSpecs without any color specifications (empty)
        ColorSpecs::new(&[])
    }
}

const CONFIG_DIR: &str = "gfr";
const INSTALLED_MANIFEST_FILE: &str = "installed.json";
const DEFAULT_PATTERNS_URL: &str =
    "https://raw.githubusercontent.com/Kr1shna4garwal/gfr-patterns/refs/heads/main/index.json";
const DEFAULT_PATTERN_SCHEMA_URL: &str = "https://raw.githubusercontent.com/Kr1shna4garwal/gfr-patterns/refs/heads/main/schemas/pattern.schema.json";

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Search for patterns in files or stdin.
    Search {
        /// The name of the pattern to search for (e.g., "rce", "ipv4")
        pattern_name: Option<String>,

        /// File or directory path to search. Defaults to current directory.
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Show search configuration and exit without searching.
        #[arg(long, short)]
        dump: bool,

        /// Filter patterns by comma-separated tags (e.g., "web,security").
        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,

        /// Filter patterns by author name.
        #[arg(long)]
        author: Option<String>,

        /// Include binary files in the search.
        #[arg(long)]
        include_bin: bool,
    },
    /// List all available local patterns.
    List,
    /// Install or update patterns from a remote index file.
    Install {
        /// Optional URL to a custom patterns index.json file.
        #[arg(default_value = DEFAULT_PATTERNS_URL)]
        url: String,
    },
    /// Save a new local pattern interactively.
    Save(SaveArgs),
}

#[derive(Parser, Debug)]
pub struct SaveArgs {
    /// The name for the new pattern (e.g., "xss").
    name: String,
    /// The regular expression to search for.
    pattern: String,
    /// Description of what the pattern finds.
    #[arg(long, short)]
    description: Option<String>,
    /// List of case-insensitive file extensions to search (e.g., "js,html,ts").
    #[arg(long, short, value_delimiter = ',')]
    file_types: Option<Vec<String>>,
    /// Make the search case-insensitive.
    #[arg(long, short = 'i')]
    ignore_case: bool,
    /// Enable multi-line searching (dot matches newline).
    #[arg(long, short = 'm')]
    multiline: bool,
    /// The author of the pattern.
    #[arg(long, short = 'a')]
    author: Option<String>,
    /// Comma-separated tags for categorization.
    #[arg(long, short = 't', value_delimiter = ',')]
    tags: Option<Vec<String>>,
}

/// Represents a search pattern configuration loaded from a JSON file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pattern {
    /// JSON Schema reference (optional, for validation support).
    #[serde(rename = "$schema")]
    schema: Option<String>,
    /// Semantic version of the pattern file.
    #[serde(default = "default_version")]
    version: String,
    /// Author of the pattern.
    author: Option<String>,
    /// A brief explanation of the pattern.
    description: Option<String>,
    /// A list of tags for categorization.
    tags: Option<Vec<String>>,
    /// A single regular expression. Use this or `regex_list`.
    #[serde(rename = "pattern")]
    regex: Option<String>,
    /// A list of regular expressions. These will be combined into a single pattern.
    #[serde(rename = "patterns")]
    regex_list: Option<Vec<String>>,
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
    /// Combines `regex` and `regex_list` fields into a single regex string.
    /// The patterns are joined with `|` to create a single regex.
    fn get_raw_pattern(&self) -> Result<String> {
        match (&self.regex, &self.regex_list) {
            (Some(p), None) => Ok(p.clone()),
            (None, Some(ps)) if !ps.is_empty() => Ok(format!("(?:{})", ps.join("|"))),
            _ => Err(anyhow!(
                "Pattern file must contain either a 'pattern' key or a non-empty 'patterns' key."
            )),
        }
    }
}

/// Represents the remote index file for installable patterns.
#[derive(Debug, Deserialize)]
struct Index {
    patterns: Vec<IndexPattern>,
}

/// Represents a single pattern entry in the remote index.
#[derive(Debug, Deserialize)]
struct IndexPattern {
    name: String,
    version: String,
    url: String,
}

/// Represents the local manifest of installed patterns and their versions.
type InstalledManifest = HashMap<String, String>;

/// Default version for a new pattern.
fn default_version() -> String {
    "1.0.0".to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let styles = Styles::new();

    // The `if let Err` block handles all errors propagated with `?` from the subcommands.
    if let Err(e) = run_command(cli.command, &styles).await {
        eprintln!("{} {:#}", "Error:".style(styles.error), e);
        exit(1);
    }

    Ok(())
}

/// Dispatches the appropriate function based on the parsed command.
async fn run_command(command: Commands, styles: &Styles) -> Result<()> {
    match command {
        Commands::Search {
            pattern_name,
            path,
            dump,
            tags,
            author,
            include_bin,
        } => {
            if dump {
                // Dump only supports a single pattern name for clarity.
                let name_to_dump = pattern_name.ok_or_else(|| {
                    anyhow!("--dump requires a single pattern_name to be specified.")
                })?;
                run_dump(&name_to_dump, styles)
            } else {
                run_search(
                    pattern_name,
                    tags.as_deref(),
                    author.as_deref(),
                    &path,
                    include_bin,
                    styles,
                )
            }
        }
        Commands::List => run_list(styles),
        Commands::Install { url } => run_install(&url, styles).await,
        Commands::Save(args) => run_save(args, styles),
    }
}

/// Executes the search operation based on provided filters.
#[allow(clippy::too_many_lines)] // This function orchestrates the entire search logic.
fn run_search(
    pattern_name: Option<String>,
    tags: Option<&[String]>,
    author: Option<&str>,
    path: &Path,
    include_bin: bool,
    styles: &Styles,
) -> Result<()> {
    // At least one filter must be provided to know what to search for.
    if pattern_name.is_none() && tags.is_none() && author.is_none() {
        return Err(anyhow!(
            "Search requires a filter. Please provide a pattern name, --tags, or --author."
        ));
    }

    // Prevent conflicting usage: pattern name should not be combined with filters.
    if pattern_name.is_some() && (tags.is_some() || author.is_some()) {
        return Err(anyhow!(
            "Cannot combine pattern name with --tags or --author filters. Use either a specific pattern name OR filters, not both."
        ));
    }

    let patterns_to_search = find_patterns_by_filter(pattern_name, tags, author, styles)?;
    println!(
        "{} {} patterns on path '{}'...",
        "Searching with".style(styles.dim),
        patterns_to_search.len().to_string().style(styles.highlight),
        path.display().style(styles.highlight)
    );

    // --- Aggregate all patterns into a single configuration ---
    let mut all_regexes: Vec<String> = Vec::new();
    let mut all_file_types: HashSet<String> = HashSet::new();
    let mut combined_ignore_case: bool = false;
    let mut combined_multiline: bool = false;

    for p in &patterns_to_search {
        all_regexes.push(p.get_raw_pattern()?);
        if let Some(fts) = &p.file_types {
            all_file_types.extend(fts.iter().cloned());
        }
        combined_ignore_case |= p.ignore_case;
        combined_multiline |= p.multiline;
    }

    let mut flags: String = String::new();
    if combined_ignore_case {
        flags.push('i');
    }
    if combined_multiline {
        flags.push('s');
    }
    let patterns_combined: String = all_regexes.join("|");
    let final_pattern: String = if flags.is_empty() {
        patterns_combined
    } else {
        format!("(?{flags}){patterns_combined}")
    };

    let matcher: grep_regex::RegexMatcher = RegexMatcherBuilder::new()
        .line_terminator(Some(b'\n'))
        .build(&final_pattern)?;

    // --- Execute Search ---
    if io::stdin().is_terminal() {
        // Search the file system.
        let mut walk_builder: WalkBuilder = WalkBuilder::new(path);
        walk_builder.add_custom_ignore_filename(".gfrignore");

        if !all_file_types.is_empty() && !include_bin {
            let mut override_builder: ignore::overrides::OverrideBuilder =
                ignore::overrides::OverrideBuilder::new(path);
            for ft in &all_file_types {
                override_builder.add(&format!("*.{ft}"))?;
            }
            let overrides: ignore::overrides::Override = override_builder.build()?;
            walk_builder.overrides(overrides);
        }

        walk_builder.build_parallel().run(|| {
            let matcher: grep_regex::RegexMatcher = matcher.clone();
            let mut searcher: Searcher = SearcherBuilder::new()
                .binary_detection(if include_bin {
                    // This disables binary detection, treating all files as text.
                    BinaryDetection::none()
                } else {
                    // This is the default behavior: skip binary files.
                    BinaryDetection::quit(b'\x00')
                })
                .build();
            let mut printer: grep_printer::Standard<StandardStream> = StandardBuilder::new()
                .color_specs(get_color_specs())
                .build(StandardStream::stdout(get_color_choice()));

            Box::new(
                move |result: std::result::Result<ignore::DirEntry, ignore::Error>| {
                    let entry: ignore::DirEntry = match result {
                        Ok(entry) => entry,
                        Err(err) => {
                            eprintln!("{} {}", "Error:".style(styles.error), err);
                            return WalkState::Continue;
                        }
                    };
                    if entry
                        .file_type()
                        .is_some_and(|ft: fs::FileType| ft.is_file())
                    {
                        let search_result: std::result::Result<(), io::Error> = searcher
                            .search_path(
                                &matcher,
                                entry.path(),
                                printer.sink_with_path(&matcher, entry.path()),
                            );
                        if let Err(e) = search_result {
                            eprintln!("{}: {}", entry.path().display().style(styles.error), e);
                        }
                    }
                    WalkState::Continue
                },
            )
        });
    } else {
        // If data is piped to stdin, search it instead of files.
        let mut printer: grep_printer::Standard<StandardStream> = StandardBuilder::new()
            .color_specs(get_color_specs())
            .build(StandardStream::stdout(get_color_choice()));
        let mut searcher: Searcher = Searcher::new();
        searcher.search_reader(&matcher, io::stdin(), printer.sink(&matcher))?;
    }

    Ok(())
}

async fn run_install(url: &str, styles: &Styles) -> Result<()> {
    println!(
        "{} Fetching pattern index from {}...",
        "i".style(styles.info),
        url.style(styles.highlight)
    );

    let client: reqwest::Client = reqwest::Client::new();
    let index: Index = client
        .get(url)
        .send()
        .await?
        .json()
        .await
        .with_context(|| format!("Failed to fetch or parse index from {url}"))?;

    println!(
        "{} Found {} patterns in index.",
        "✓".style(styles.success),
        index.patterns.len().to_string().style(styles.highlight),
    );

    let pattern_dir: PathBuf = get_pattern_dir()?;
    fs::create_dir_all(&pattern_dir)?;
    let mut manifest: HashMap<String, String> = load_manifest().unwrap_or_default();
    let mut updated_count: i32 = 0;
    let mut added_count: i32 = 0;

    for remote_pattern in index.patterns {
        let local_version_str: Option<&String> = manifest.get(&remote_pattern.name);
        let remote_version: Version = Version::parse(&remote_pattern.version)?;

        let should_install: bool = local_version_str
            .and_then(|local_v_str: &String| Version::parse(local_v_str).ok())
            .is_none_or(|local_version: Version| local_version < remote_version);

        if should_install {
            print!(
                "  -> Installing/Updating '{}' (v{}) from {}... ",
                remote_pattern.name.style(styles.highlight),
                remote_pattern.version,
                remote_pattern.url.style(styles.dim)
            );
            io::stdout().flush()?;

            let pattern_response: reqwest::Response =
                client.get(&remote_pattern.url).send().await?;
            let pattern_json: serde_json::Value =
                pattern_response.json().await.with_context(|| {
                    format!(
                        "Failed to fetch or parse pattern JSON from {}",
                        remote_pattern.url
                    )
                })?;

            // Validate it's a valid Pattern struct before saving, it will save a lot of headaches later.
            let _: Pattern = serde_json::from_value(pattern_json.clone())?;

            let file_path: PathBuf = pattern_dir.join(format!("{}.json", remote_pattern.name));
            let file: File = File::create(&file_path)?;
            serde_json::to_writer_pretty(file, &pattern_json)?;

            println!("{}", "Done".style(styles.success));

            if local_version_str.is_some() {
                updated_count += 1;
            } else {
                added_count += 1;
            }
            manifest.insert(remote_pattern.name, remote_pattern.version);
        }
    }

    save_manifest(&manifest)?;
    println!(
        "\n{} Installation complete. Added {} new, updated {} existing patterns.",
        "✓".style(styles.success),
        added_count.to_string().style(styles.highlight),
        updated_count.to_string().style(styles.highlight)
    );

    Ok(())
}

/// Lists all available patterns in the configuration directory.
fn run_list(styles: &Styles) -> Result<()> {
    println!("{}", "Available local patterns:".style(styles.title));
    let pattern_dir: PathBuf = get_pattern_dir()?;
    if !pattern_dir.exists() {
        println!(
            "  {}",
            "No pattern directory found. Use `gfr install` to get started.".style(styles.dim)
        );
        return Ok(());
    }

    let mut patterns: Vec<(String, Option<Pattern>)> = Vec::new();
    for entry in fs::read_dir(pattern_dir)?.filter_map(Result::ok) {
        let path: PathBuf = entry.path();
        if path
            .extension()
            .is_some_and(|e: &std::ffi::OsStr| e == "json")
            && entry.file_name().to_str() != Some(INSTALLED_MANIFEST_FILE)
        {
            if let Some(name) = path.file_stem().and_then(std::ffi::OsStr::to_str) {
                patterns.push((name.to_string(), load_pattern(name).ok()));
            }
        }
    }

    if patterns.is_empty() {
        println!(
            "  {}",
            "No patterns found. Use 'gfr save' or 'gfr install' to add some.".style(styles.dim)
        );
        return Ok(());
    }

    patterns.sort_by(|a: &(String, Option<Pattern>), b: &(String, Option<Pattern>)| a.0.cmp(&b.0));

    for (name, pattern_opt) in patterns {
        if let Some(p) = pattern_opt {
            let desc: &str = p.description.as_deref().unwrap_or("No description");
            let tags: String = p
                .tags
                .map(|t: Vec<String>| format!("[{}]", t.join(", ")))
                .unwrap_or_default();
            println!(
                "  {} {} - {}",
                name.style(styles.highlight),
                tags.style(styles.info),
                desc.style(styles.dim)
            );
            println!(
                "    v{} by {}",
                p.version,
                p.author.as_deref().unwrap_or("Unknown").style(styles.dim)
            );
        } else {
            println!(
                "  {} - {}",
                name.style(styles.error),
                "Invalid pattern file".style(styles.error)
            );
        }
    }

    Ok(())
}

/// Saves a new pattern to a JSON file.
fn run_save(args: SaveArgs, styles: &Styles) -> Result<()> {
    if args.name.contains(['.', '/', '\\']) {
        return Err(anyhow!(
            "Invalid pattern name '{}'. Name cannot contain '.', '/', or '\\'",
            args.name.style(styles.error)
        ));
    }

    let pattern_dir = get_pattern_dir()?;
    fs::create_dir_all(&pattern_dir)?;
    let pattern_file_path = pattern_dir.join(format!("{}.json", args.name));

    if pattern_file_path.exists() {
        return Err(anyhow!(
            "Pattern '{}' already exists.",
            args.name.style(styles.highlight)
        ));
    }

    let new_pattern = Pattern {
        schema: Some(DEFAULT_PATTERN_SCHEMA_URL.to_string()),
        version: "1.0.0".to_string(),
        author: args.author,
        description: args.description,
        tags: args.tags,
        regex: Some(args.pattern),
        regex_list: None,
        file_types: args.file_types,
        ignore_case: args.ignore_case,
        multiline: args.multiline,
    };
    let file = File::create(&pattern_file_path)?;
    serde_json::to_writer_pretty(file, &new_pattern)?;

    println!(
        "{} Pattern '{}' saved to {}",
        "✓".style(styles.success),
        args.name.style(styles.highlight),
        pattern_file_path.display().style(styles.dim)
    );

    Ok(())
}

/// Prints the configuration of a pattern without executing a search.
fn run_dump(pattern_name: &str, styles: &Styles) -> Result<()> {
    let pattern: Pattern = load_pattern(pattern_name)?;
    let pattern_path: PathBuf = get_pattern_dir()?.join(format!("{pattern_name}.json"));

    println!(
        "{}",
        format!("Configuration for '{pattern_name}'").style(styles.title)
    );
    println!(
        "{} {}",
        "Loaded from:".style(styles.dim),
        pattern_path.display()
    );
    println!("---");
    let json_string: String = serde_json::to_string_pretty(&pattern)?;
    println!("{json_string}");

    Ok(())
}

// --- Filesystem and Pattern Loading Utilities ---

fn get_pattern_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .ok_or_else(|| anyhow!("Could not determine user's config directory."))
        .map(|dir: PathBuf| dir.join(CONFIG_DIR))
}

fn load_pattern(name: &str) -> Result<Pattern> {
    let pattern_file: PathBuf = get_pattern_dir()?.join(format!("{name}.json"));
    if !pattern_file.exists() {
        return Err(anyhow!(
            "Pattern file not found: {}",
            pattern_file.display()
        ));
    }
    let file: File = File::open(&pattern_file)?;
    serde_json::from_reader(file)
        .with_context(|| format!("Failed to parse JSON from: {}", pattern_file.display()))
}

fn find_patterns_by_filter(
    name: Option<String>,
    tags: Option<&[String]>,
    author: Option<&str>,
    styles: &Styles,
) -> Result<Vec<Pattern>> {
    if let Some(name) = name {
        let p: Pattern = load_pattern(&name).with_context(|| {
            format!(
                "Failed to load pattern '{}'. Try '{}' to see available patterns.",
                name.style(styles.highlight),
                "gfr list".style(styles.highlight)
            )
        })?;
        return Ok(vec![p]);
    }

    let mut matched_patterns: Vec<Pattern> = Vec::new();
    let pattern_dir: PathBuf = get_pattern_dir()?;
    if !pattern_dir.exists() {
        return Ok(matched_patterns); // No patterns to filter.
    }

    for entry in fs::read_dir(pattern_dir)?.filter_map(Result::ok) {
        let path: PathBuf = entry.path();
        if path
            .extension()
            .is_some_and(|e: &std::ffi::OsStr| e == "json")
            && entry.file_name().to_str() != Some(INSTALLED_MANIFEST_FILE)
        {
            if let Some(name) = path.file_stem().and_then(std::ffi::OsStr::to_str) {
                if let Ok(p) = load_pattern(name) {
                    let author_match: bool =
                        author.is_none_or(|a: &str| p.author.as_deref() == Some(a));
                    let tags_match: bool = tags.is_none_or(|search_tags: &[String]| {
                        p.tags.as_ref().is_some_and(|p_tags: &Vec<String>| {
                            search_tags.iter().all(|st: &String| p_tags.contains(st))
                        })
                    });

                    if author_match && tags_match {
                        matched_patterns.push(p);
                    }
                }
            }
        }
    }

    if matched_patterns.is_empty() {
        return Err(anyhow!(
            "No patterns found matching the specified criteria."
        ));
    }

    Ok(matched_patterns)
}

fn load_manifest() -> Result<InstalledManifest> {
    let manifest_path: PathBuf = get_pattern_dir()?.join(INSTALLED_MANIFEST_FILE);
    if !manifest_path.exists() {
        return Ok(HashMap::new());
    }
    let file = File::open(manifest_path)?;
    Ok(serde_json::from_reader(file)?)
}

fn save_manifest(manifest: &InstalledManifest) -> Result<()> {
    let manifest_path: PathBuf = get_pattern_dir()?.join(INSTALLED_MANIFEST_FILE);
    let file: File = File::create(manifest_path)?;
    serde_json::to_writer_pretty(file, manifest)?;
    Ok(())
}

// --- Terminal Styling ---

struct Styles {
    error: Style,
    success: Style,
    highlight: Style,
    dim: Style,
    title: Style,
    info: Style,
}

impl Styles {
    fn new() -> Self {
        if io::stdout().is_terminal() {
            // Terminal output: use colors
            Self {
                error: Style::new().red().bold(),
                success: Style::new().green(),
                highlight: Style::new().yellow().bold(),
                dim: Style::new().dimmed(),
                title: Style::new().bold().underline(),
                info: Style::new().cyan(),
            }
        } else {
            // Piped output: no colors (when output was redirected it was messing with ANSI codes)
            Self {
                error: Style::new(),
                success: Style::new(),
                highlight: Style::new(),
                dim: Style::new(),
                title: Style::new(),
                info: Style::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Default for Pattern {
        fn default() -> Self {
            Self {
                schema: None,
                version: "0.0.0".to_string(),
                author: None,
                description: None,
                tags: None,
                regex: None,
                regex_list: None,
                file_types: None,
                ignore_case: false,
                multiline: false,
            }
        }
    }
    #[test]
    fn test_get_raw_pattern_logic() {
        let p1: Pattern = Pattern {
            regex: Some("abc".to_string()),
            ..Default::default()
        };
        assert_eq!(p1.get_raw_pattern().unwrap(), "abc");

        let p2: Pattern = Pattern {
            regex_list: Some(vec!["a".to_string(), "b".to_string()]),
            ..Default::default()
        };
        assert_eq!(p2.get_raw_pattern().unwrap(), "(?:a|b)");

        let p3: Pattern = Pattern {
            regex: None,
            regex_list: Some(vec![]),
            ..Default::default()
        };
        assert!(p3.get_raw_pattern().is_err());

        let p4: Pattern = Pattern {
            regex: None,
            regex_list: None,
            ..Default::default()
        };
        assert!(p4.get_raw_pattern().is_err());
    }
}
