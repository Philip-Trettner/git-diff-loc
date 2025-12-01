use clap::Parser;
use colored::Colorize;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, exit};

/// git-diff-loc: A tool for analyzing lines of code changes between git commits
///
/// This tool parses git diffs and counts lines by language and type (code vs comments).
///
/// Methodology:
/// - Lines are classified by file extension (e.g., .rs -> Rust, .py -> Python)
/// - Empty lines and lines with no alphanumeric characters are ignored
/// - Lines starting with language-specific comment prefixes (//, #) are counted as comments
/// - All other non-empty lines are counted as code
/// - Text and Markdown files have no comment detection (all lines count as code)
///
/// Usage example:
///   git-diff-loc main feature-branch
///   git-diff-loc abc123 def456
///   git-diff-loc HEAD~5 HEAD
#[derive(Parser)]
#[command(name = "git-diff-loc")]
#[command(about = "Count lines of code changes between two git commits", long_about = None)]
struct Cli {
    commit_from: String,
    commit_to: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Language {
    Rust,
    CCpp,
    Go,
    Python,
    JavaScript,
    TypeScript,
    Java,
    CMake,
    Shell,
    Ruby,
    Markdown,
    Text,
    Dotfiles,
    Unknown,
}

impl Language {
    fn from_filename(filename: &str) -> Self {
        let lower_filename = filename.to_lowercase();

        if lower_filename == "cmakelists.txt" {
            return Language::CMake;
        }

        // Check if it's a dotfile (starts with . and has no further extension)
        if filename.starts_with('.') && !filename[1..].contains('.') {
            return Language::Dotfiles;
        }

        if let Some(ext_pos) = filename.rfind('.') {
            let ext = &filename[ext_pos + 1..];
            match ext.to_lowercase().as_str() {
                "rs" => Language::Rust,
                "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Language::CCpp,
                "go" => Language::Go,
                "py" => Language::Python,
                "js" | "jsx" => Language::JavaScript,
                "ts" | "tsx" => Language::TypeScript,
                "java" => Language::Java,
                "cmake" => Language::CMake,
                "sh" | "bash" => Language::Shell,
                "rb" => Language::Ruby,
                "md" | "markdown" => Language::Markdown,
                "txt" => Language::Text,
                _ => Language::Unknown,
            }
        } else {
            Language::Unknown
        }
    }

    fn comment_prefixes(&self) -> Vec<&str> {
        match self {
            Language::Rust
            | Language::CCpp
            | Language::Go
            | Language::JavaScript
            | Language::TypeScript
            | Language::Java => vec!["//"],
            Language::Python
            | Language::CMake
            | Language::Shell
            | Language::Ruby
            | Language::Dotfiles => vec!["#"],
            Language::Markdown | Language::Text => vec![],
            Language::Unknown => vec!["//"],
        }
    }

    fn name(&self) -> &str {
        match self {
            Language::Rust => "Rust",
            Language::CCpp => "C/C++",
            Language::Go => "Go",
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Java => "Java",
            Language::CMake => "CMake",
            Language::Shell => "Shell",
            Language::Ruby => "Ruby",
            Language::Markdown => "Markdown",
            Language::Text => "Text",
            Language::Dotfiles => "Dotfiles",
            Language::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LineType {
    Code,
    Comment,
}

#[derive(Debug, Default)]
struct Stats {
    added: usize,
    removed: usize,
    test_added: usize,
    test_removed: usize,
}

impl Stats {
    fn total(&self) -> usize {
        self.added + self.removed
    }

    fn test_total(&self) -> usize {
        self.test_added + self.test_removed
    }
}

fn main() {
    let cli = Cli::parse();

    let diff_output = get_git_diff(&cli.commit_from, &cli.commit_to);

    let mut code_stats: HashMap<Language, Stats> = HashMap::new();
    let mut comment_stats: Stats = Stats::default();

    parse_diff(&diff_output, &mut code_stats, &mut comment_stats);

    print_results(&code_stats, &comment_stats);
}

fn get_git_diff(commit_from: &str, commit_to: &str) -> String {
    let output = Command::new("git")
        .args(["diff", commit_from, commit_to])
        .output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                eprintln!("{}", "Error: git diff command failed".red());
                eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                exit(1);
            }
            String::from_utf8_lossy(&output.stdout).to_string()
        }
        Err(e) => {
            eprintln!("{}", format!("Error executing git: {}", e).red());
            exit(1);
        }
    }
}

fn parse_diff(diff: &str, code_stats: &mut HashMap<Language, Stats>, comment_stats: &mut Stats) {
    let mut current_file: Option<String> = None;

    for line in diff.lines() {
        if line.starts_with("diff --git") {
            current_file = extract_file_path(line);
        } else if line.starts_with('+') && !line.starts_with("+++") {
            if let Some(ref file) = current_file {
                let content = &line[1..];
                classify_and_count(content, file, code_stats, comment_stats, true);
            }
        } else if line.starts_with('-') && !line.starts_with("---") {
            if let Some(ref file) = current_file {
                let content = &line[1..];
                classify_and_count(content, file, code_stats, comment_stats, false);
            }
        }
    }
}

fn extract_file_path(diff_line: &str) -> Option<String> {
    let parts: Vec<&str> = diff_line.split_whitespace().collect();
    if parts.len() >= 4 {
        let path = parts[3].trim_start_matches("b/");
        Some(path.to_string())
    } else {
        None
    }
}

fn classify_and_count(
    line: &str,
    file_path: &str,
    code_stats: &mut HashMap<Language, Stats>,
    comment_stats: &mut Stats,
    is_added: bool,
) {
    let trimmed = line.trim();

    if is_empty_line(trimmed) {
        return;
    }

    let language = detect_language(file_path);
    let line_type = classify_line(trimmed, language);
    let is_test = is_test_file(file_path);

    match line_type {
        LineType::Code => {
            let stats = code_stats.entry(language).or_insert_with(Stats::default);
            if is_test {
                if is_added {
                    stats.test_added += 1;
                } else {
                    stats.test_removed += 1;
                }
            } else {
                if is_added {
                    stats.added += 1;
                } else {
                    stats.removed += 1;
                }
            }
        }
        LineType::Comment => {
            if is_test {
                if is_added {
                    comment_stats.test_added += 1;
                } else {
                    comment_stats.test_removed += 1;
                }
            } else {
                if is_added {
                    comment_stats.added += 1;
                } else {
                    comment_stats.removed += 1;
                }
            }
        }
    }
}

fn is_empty_line(line: &str) -> bool {
    line.is_empty() || !line.chars().any(|c| c.is_alphanumeric())
}

fn is_test_file(file_path: &str) -> bool {
    let path = Path::new(file_path);

    // Check if any parent directory is named "test" or "tests"
    for component in path.components() {
        if let Some(name) = component.as_os_str().to_str() {
            let lower = name.to_lowercase();
            if lower == "test" || lower == "tests" {
                return true;
            }
        }
    }

    // Check if filename ends with _test, _tests, -test, or -tests
    if let Some(stem) = path.file_stem() {
        if let Some(name) = stem.to_str() {
            let lower = name.to_lowercase();
            if lower.ends_with("_test")
                || lower.ends_with("_tests")
                || lower.ends_with("-test")
                || lower.ends_with("-tests")
            {
                return true;
            }
        }
    }

    false
}

fn detect_language(file_path: &str) -> Language {
    let path = Path::new(file_path);

    if let Some(name) = path.file_name() {
        if let Some(name_str) = name.to_str() {
            return Language::from_filename(name_str);
        }
    }

    Language::Unknown
}

fn classify_line(line: &str, language: Language) -> LineType {
    let comment_prefixes = language.comment_prefixes();

    for prefix in comment_prefixes {
        if line.starts_with(prefix) {
            return LineType::Comment;
        }
    }

    LineType::Code
}

fn print_results(code_stats: &HashMap<Language, Stats>, comment_stats: &Stats) {
    println!("\n{}", "Lines of Code Changes".bold().underline());
    println!();

    // Collect all rows for table formatting (name, total, added, removed, test_total, test_added, test_removed)
    let mut rows: Vec<(String, usize, usize, usize, usize, usize, usize)> = Vec::new();

    // Add comments first if there are any
    if comment_stats.total() > 0 || comment_stats.test_total() > 0 {
        rows.push((
            "Comments".to_string(),
            comment_stats.total(),
            comment_stats.added,
            comment_stats.removed,
            comment_stats.test_total(),
            comment_stats.test_added,
            comment_stats.test_removed,
        ));
    }

    // Add language stats
    let mut languages: Vec<_> = code_stats.iter().collect();
    languages.sort_by_key(|(lang, _)| lang.name());

    for (language, stats) in languages {
        if stats.total() > 0 || stats.test_total() > 0 {
            rows.push((
                language.name().to_string(),
                stats.total(),
                stats.added,
                stats.removed,
                stats.test_total(),
                stats.test_added,
                stats.test_removed,
            ));
        }
    }

    // Calculate column widths (for the numbers only, signs are added separately)
    let name_width = rows.iter().map(|(name, ..)| name.len()).max().unwrap_or(0);
    let total_width = rows.iter().map(|(_, total, ..)| total.to_string().len()).max().unwrap_or(0);
    let added_width = rows.iter().map(|(_, _, added, ..)| added.to_string().len()).max().unwrap_or(0);
    let removed_width = rows.iter().map(|(_, _, _, removed, ..)| removed.to_string().len()).max().unwrap_or(0);
    let test_total_width = rows.iter().map(|(_, _, _, _, test_total, ..)| test_total.to_string().len()).max().unwrap_or(0);
    let test_added_width = rows.iter().map(|(_, _, _, _, _, test_added, _)| test_added.to_string().len()).max().unwrap_or(0);
    let test_removed_width = rows.iter().map(|(_, _, _, _, _, _, test_removed)| test_removed.to_string().len()).max().unwrap_or(0);

    // Print rows with proper alignment
    for (i, (name, total, added, removed, test_total, test_added, test_removed)) in rows.iter().enumerate() {
        // Format the name with padding
        let name_padded = format!("{:<width$}", name, width = name_width);
        let name_colored = if i == 0 && comment_stats.total() > 0 {
            name_padded.magenta().bold()
        } else {
            name_padded.cyan().bold()
        };

        // Format numbers with padding and colors
        let total_str = format!("{:>width$}", total, width = total_width);
        let added_str = format!("+ {:>width$}", added, width = added_width);
        let removed_str = format!("- {:>width$}", removed, width = removed_width);

        let test_total_str = format!("{:>width$}", test_total, width = test_total_width);
        let test_added_str = format!("+ {:>width$}", test_added, width = test_added_width);
        let test_removed_str = format!("- {:>width$}", test_removed, width = test_removed_width);

        println!(
            "{}  {} ({} / {})  |  {} ({} / {})",
            name_colored,
            total_str.yellow(),
            added_str.green(),
            removed_str.red(),
            test_total_str.bright_yellow(),
            test_added_str.bright_green(),
            test_removed_str.bright_red()
        );
    }

    let total_code: usize = code_stats.values().map(|s| s.total()).sum();
    let total_test: usize = code_stats.values().map(|s| s.test_total()).sum();
    let total_all = total_code + total_test + comment_stats.total();

    if total_all > 0 {
        println!();
        // Width: name + "  " + total + " (" + "+ added" + " / " + "- removed" + ")" + "  |  " + test_total + " (" + "+ test_added" + " / " + "- test_removed" + ")"
        let separator_width = name_width + 2 + total_width + 3 + 2 + added_width + 3 + 2 + removed_width + 1 + 5 + test_total_width + 3 + 2 + test_added_width + 3 + 2 + test_removed_width + 1;
        println!("{}", "─".repeat(separator_width).bright_black());

        let total_label = format!("{:<width$}", "Total changes", width = name_width);
        let total_value = format!("{:>width$}", total_all, width = total_width);
        println!(
            "{}  {}",
            total_label.white().bold(),
            total_value.yellow().bold()
        );
    }
}
