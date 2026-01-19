//! Shared implementation for bean-check and rledger-check commands.

use crate::cmd::completions::ShellType;
use crate::report::{self, SourceCache};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Parser, ValueEnum};
use rayon::prelude::*;
use rustledger_booking::{InterpolationError, interpolate};
use rustledger_core::Directive;
use rustledger_loader::{
    CacheEntry, CachedOptions, CachedPlugin, LoadError, LoadResult, Loader, load_cache_entry,
    reintern_directives, save_cache_entry,
};
#[cfg(feature = "python-plugin-wasm")]
use rustledger_plugin::PluginManager;
use rustledger_plugin::{NativePluginRegistry, PluginInput, PluginOptions, wrappers_to_directives};
use rustledger_validate::validate;
use serde::Serialize;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan;

/// Output format for diagnostics.
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text output (default)
    #[default]
    Text,
    /// JSON output for IDE/tooling integration
    Json,
}

/// A diagnostic message in JSON format.
#[derive(Debug, Serialize)]
pub struct JsonDiagnostic {
    /// Source file path
    pub file: String,
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
    /// End line number (1-based)
    pub end_line: usize,
    /// End column number (1-based)
    pub end_column: usize,
    /// Severity: "error" or "warning"
    pub severity: String,
    /// Error code (e.g., "P0012", "E1001")
    pub code: String,
    /// Error message
    pub message: String,
    /// Optional hint for fixing the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Optional context information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// JSON output structure for all diagnostics.
#[derive(Debug, Serialize)]
pub struct JsonOutput {
    /// List of diagnostics
    pub diagnostics: Vec<JsonDiagnostic>,
    /// Total error count
    pub error_count: usize,
    /// Total warning count
    pub warning_count: usize,
}

/// Convert a byte offset to (line, column) in 1-based indexing.
fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Validate beancount files and report errors.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// The beancount file to check
    #[arg(value_name = "FILE", required_unless_present = "generate_completions")]
    pub file: Option<PathBuf>,

    /// Generate shell completions and exit
    #[arg(long, value_name = "SHELL", hide = true)]
    pub generate_completions: Option<ShellType>,

    /// Show verbose output including timing information
    #[arg(short, long)]
    pub verbose: bool,

    /// Suppress all output (just use exit code)
    #[arg(short, long)]
    pub quiet: bool,

    /// Disable the binary cache for parsed directives
    #[arg(short = 'C', long = "no-cache")]
    pub no_cache: bool,

    /// Override the cache filename (not yet implemented)
    #[arg(long, value_name = "CACHE_FILE", hide = true)]
    pub cache_filename: Option<PathBuf>,

    /// Implicitly enable auto-plugins (`auto_accounts`, etc.)
    #[arg(short = 'a', long)]
    pub auto: bool,

    /// Load a WASM plugin (can be specified multiple times)
    #[cfg(feature = "python-plugin-wasm")]
    #[arg(long = "plugin", value_name = "WASM_FILE")]
    pub plugins: Vec<PathBuf>,

    /// Run built-in native plugins (e.g., `implicit_prices`, `check_commodity`)
    #[arg(long = "native-plugin", value_name = "NAME")]
    pub native_plugins: Vec<String>,

    /// Output format (text or json)
    #[arg(long, short = 'f', value_enum, default_value = "text")]
    pub format: OutputFormat,
}

fn run(args: &Args) -> Result<ExitCode> {
    let mut stdout = io::stdout().lock();
    let start = std::time::Instant::now();

    // File is guaranteed to be Some here (checked in main)
    let file = args.file.as_ref().expect("file required");

    // Check if file exists
    if !file.exists() {
        anyhow::bail!("file not found: {}", file.display());
    }

    // Collect diagnostics for JSON output
    let json_mode = matches!(args.format, OutputFormat::Json);
    let mut diagnostics: Vec<JsonDiagnostic> = Vec::new();

    // Try loading from cache first (unless --no-cache)
    let cache_entry = if args.no_cache {
        None
    } else {
        load_cache_entry(file)
    };

    let (load_result, from_cache) = if let Some(mut entry) = cache_entry {
        if args.verbose && !args.quiet {
            eprintln!("Loaded {} directives from cache", entry.directives.len());
        }

        // Re-intern strings to deduplicate memory
        let dedup_count = reintern_directives(&mut entry.directives);
        if args.verbose && !args.quiet {
            eprintln!("Re-interned strings ({dedup_count} deduplicated)");
        }

        // Rebuild source map from cached file list
        let mut source_map = rustledger_loader::SourceMap::new();
        for path in entry.file_paths() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                source_map.add_file(path, content.into());
            }
        }

        // Convert CachedPlugin -> Plugin (span/file_id are not meaningful from cache)
        let plugins: Vec<rustledger_loader::Plugin> = entry
            .plugins
            .iter()
            .map(|p| rustledger_loader::Plugin {
                name: p.name.clone(),
                config: p.config.clone(),
                span: rustledger_parser::Span::new(0, 0),
                file_id: 0,
            })
            .collect();

        let result = rustledger_loader::LoadResult {
            directives: entry.directives,
            options: entry.options.into(),
            plugins,
            source_map,
            errors: Vec::new(),
        };
        (result, true)
    } else {
        // Load the file normally
        if args.verbose && !args.quiet {
            eprintln!("Loading {}...", file.display());
        }

        let mut loader = Loader::new();
        let result = loader
            .load(file)
            .with_context(|| format!("failed to load {}", file.display()))?;

        // Save to cache (unless --no-cache or there are parse errors)
        if !args.no_cache && result.errors.is_empty() {
            // Collect all loaded file paths for cache (as strings for serialization)
            let files: Vec<String> = result
                .source_map
                .files()
                .iter()
                .map(|f| f.path.to_string_lossy().into_owned())
                .collect();
            let files = if files.is_empty() {
                vec![file.to_string_lossy().into_owned()]
            } else {
                files
            };

            // Create full cache entry
            let entry = CacheEntry {
                directives: result.directives.clone(),
                options: CachedOptions::from(&result.options),
                plugins: result
                    .plugins
                    .iter()
                    .map(|p| CachedPlugin {
                        name: p.name.clone(),
                        config: p.config.clone(),
                    })
                    .collect(),
                files,
            };

            if let Err(e) = save_cache_entry(file, &entry) {
                if args.verbose && !args.quiet {
                    eprintln!("Warning: failed to save cache: {e}");
                }
            } else if args.verbose && !args.quiet {
                eprintln!("Saved {} directives to cache", result.directives.len());
            }
        }

        (result, false)
    };

    // Build source cache for error reporting
    let mut cache = SourceCache::new();
    for source_file in load_result.source_map.files() {
        let content = std::fs::read_to_string(&source_file.path).unwrap_or_else(|_| String::new());
        let path_str = source_file.path.display().to_string();
        cache.add(&path_str, content);
    }

    // Also add the main file
    let main_content = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read {}", file.display()))?;
    cache.add(&file.display().to_string(), main_content);

    // Count errors
    let mut error_count = 0;

    // Report load/parse errors
    for load_error in &load_result.errors {
        match load_error {
            LoadError::ParseErrors { path, errors } => {
                let source = std::fs::read_to_string(path).unwrap_or_default();
                let path_str = path.display().to_string();

                if json_mode {
                    for error in errors {
                        let (start_line, start_col) =
                            byte_offset_to_line_col(&source, error.span.start);
                        let (end_line, end_col) = byte_offset_to_line_col(&source, error.span.end);
                        diagnostics.push(JsonDiagnostic {
                            file: path_str.clone(),
                            line: start_line,
                            column: start_col,
                            end_line,
                            end_column: end_col,
                            severity: "error".to_string(),
                            code: format!("P{:04}", error.kind_code()),
                            message: error.message(),
                            hint: error.hint.clone(),
                            context: error.context.clone(),
                        });
                    }
                    error_count += errors.len();
                } else if args.quiet {
                    error_count += errors.len();
                } else {
                    error_count += report::report_parse_errors(errors, path, &source, &mut stdout)?;
                }
            }
            LoadError::Io { path, source } => {
                let path_str = path.display().to_string();
                if json_mode {
                    diagnostics.push(JsonDiagnostic {
                        file: path_str,
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 1,
                        severity: "error".to_string(),
                        code: "E0001".to_string(),
                        message: format!("failed to read file: {source}"),
                        hint: None,
                        context: None,
                    });
                } else if !args.quiet {
                    writeln!(stdout, "error: failed to read {path_str}: {source}")?;
                }
                error_count += 1;
            }
            LoadError::IncludeCycle { cycle } => {
                if json_mode {
                    diagnostics.push(JsonDiagnostic {
                        file: cycle.first().cloned().unwrap_or_default(),
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 1,
                        severity: "error".to_string(),
                        code: "E0002".to_string(),
                        message: format!("include cycle detected: {}", cycle.join(" -> ")),
                        hint: Some("break the cycle by removing one of the includes".to_string()),
                        context: None,
                    });
                } else if !args.quiet {
                    writeln!(
                        stdout,
                        "error: include cycle detected: {}",
                        cycle.join(" -> ")
                    )?;
                }
                error_count += 1;
            }
            LoadError::PathTraversal {
                include_path,
                base_dir,
            } => {
                if json_mode {
                    diagnostics.push(JsonDiagnostic {
                        file: base_dir.display().to_string(),
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 1,
                        severity: "error".to_string(),
                        code: "E0003".to_string(),
                        message: format!(
                            "path traversal not allowed: {} escapes {}",
                            include_path,
                            base_dir.display()
                        ),
                        hint: Some("use paths within the base directory".to_string()),
                        context: None,
                    });
                } else if !args.quiet {
                    writeln!(
                        stdout,
                        "error: path traversal not allowed: {} escapes {}",
                        include_path,
                        base_dir.display()
                    )?;
                }
                error_count += 1;
            }
            LoadError::Decryption { path, message } => {
                let path_str = path.display().to_string();
                if json_mode {
                    diagnostics.push(JsonDiagnostic {
                        file: path_str,
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 1,
                        severity: "error".to_string(),
                        code: "E0004".to_string(),
                        message: format!("failed to decrypt: {message}"),
                        hint: None,
                        context: None,
                    });
                } else if !args.quiet {
                    writeln!(
                        stdout,
                        "error: failed to decrypt {}: {}",
                        path.display(),
                        message
                    )?;
                }
                error_count += 1;
            }
        }
    }

    // Report option warnings (E7001, E7002, E7003)
    let main_file_str = file.display().to_string();
    let option_warning_count = load_result.options.warnings.len();
    for warning in &load_result.options.warnings {
        if json_mode {
            diagnostics.push(JsonDiagnostic {
                file: main_file_str.clone(),
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 1,
                severity: "warning".to_string(),
                code: warning.code.to_string(),
                message: warning.message.clone(),
                hint: None,
                context: None,
            });
        } else if !args.quiet {
            writeln!(stdout, "warning[{}]: {}", warning.code, warning.message)?;
        }
    }

    // Destructure to enable move instead of clone
    let LoadResult {
        directives: spanned_directives,
        options,
        ..
    } = load_result;

    // Extract directives (move, not clone)
    let mut directives: Vec<_> = spanned_directives.into_iter().map(|s| s.value).collect();

    // Build list of native plugins to run
    let mut native_plugins_to_run = args.native_plugins.clone();

    // If --auto is set, add auto-plugins
    if args.auto && !native_plugins_to_run.contains(&"auto_accounts".to_string()) {
        native_plugins_to_run.insert(0, "auto_accounts".to_string());
    }

    // Run plugins if specified
    #[cfg(feature = "python-plugin-wasm")]
    let has_wasm_plugins = !args.plugins.is_empty();
    #[cfg(not(feature = "python-plugin-wasm"))]
    let has_wasm_plugins = false;

    if !native_plugins_to_run.is_empty() || has_wasm_plugins {
        if args.verbose && !args.quiet {
            eprintln!("Running plugins...");
        }

        let wrappers = rustledger_plugin::directives_to_wrappers(&directives);
        let plugin_input = PluginInput {
            directives: wrappers,
            options: PluginOptions {
                operating_currencies: options.operating_currency,
                title: options.title,
            },
            config: None,
        };

        let native_registry = NativePluginRegistry::new();
        let mut current_input = plugin_input;

        for plugin_name in &native_plugins_to_run {
            if let Some(plugin) = native_registry.find(plugin_name) {
                if args.verbose && !args.quiet {
                    eprintln!("  Running native plugin: {}", plugin.name());
                }
                let output = plugin.process(current_input.clone());

                for err in &output.errors {
                    if !args.quiet {
                        writeln!(stdout, "{:?}: {}", err.severity, err.message)?;
                    }
                    error_count += 1;
                }

                current_input = PluginInput {
                    directives: output.directives,
                    options: current_input.options.clone(),
                    config: None,
                };
            } else if !args.quiet {
                writeln!(stdout, "warning: unknown native plugin: {plugin_name}")?;
            }
        }

        #[cfg(feature = "python-plugin-wasm")]
        if !args.plugins.is_empty() {
            let mut wasm_manager = PluginManager::new();

            for plugin_path in &args.plugins {
                if args.verbose && !args.quiet {
                    eprintln!("  Loading WASM plugin: {}", plugin_path.display());
                }
                if let Err(e) = wasm_manager.load(plugin_path) {
                    if !args.quiet {
                        writeln!(
                            stdout,
                            "error: failed to load WASM plugin {}: {}",
                            plugin_path.display(),
                            e
                        )?;
                    }
                    error_count += 1;
                }
            }

            if !wasm_manager.is_empty() {
                if args.verbose && !args.quiet {
                    eprintln!("  Executing {} WASM plugin(s)...", wasm_manager.len());
                }

                match wasm_manager.execute_all(current_input.clone()) {
                    Ok(output) => {
                        for err in &output.errors {
                            if !args.quiet {
                                writeln!(stdout, "{:?}: {}", err.severity, err.message)?;
                            }
                            error_count += 1;
                        }

                        current_input = PluginInput {
                            directives: output.directives,
                            options: current_input.options.clone(),
                            config: None,
                        };
                    }
                    Err(e) => {
                        if !args.quiet {
                            writeln!(stdout, "error: WASM plugin execution failed: {e}")?;
                        }
                        error_count += 1;
                    }
                }
            }
        }

        match wrappers_to_directives(&current_input.directives) {
            Ok(converted) => {
                directives = converted;
            }
            Err(e) => {
                if !args.quiet {
                    writeln!(stdout, "error: failed to convert plugin output: {e}")?;
                }
                error_count += 1;
            }
        }
    }

    // Run interpolation on transactions (parallel)
    if args.verbose && !args.quiet {
        eprintln!("Interpolating {} directives...", directives.len());
    }

    let interpolation_errors: Vec<(NaiveDate, String, InterpolationError)> = directives
        .par_iter_mut()
        .filter_map(|directive| {
            if let Directive::Transaction(txn) = directive {
                match interpolate(txn) {
                    Ok(result) => {
                        *txn = result.transaction;
                        None
                    }
                    Err(e) => Some((txn.date, txn.narration.to_string(), e)),
                }
            } else {
                None
            }
        })
        .collect();

    if !interpolation_errors.is_empty() {
        if json_mode {
            for (date, narration, err) in &interpolation_errors {
                diagnostics.push(JsonDiagnostic {
                    file: main_file_str.clone(),
                    line: 1, // Transaction dates don't have line numbers yet
                    column: 1,
                    end_line: 1,
                    end_column: 1,
                    severity: "error".to_string(),
                    code: "INTERP".to_string(),
                    message: format!("{err}"),
                    hint: None,
                    context: Some(format!("{date}, \"{narration}\"")),
                });
            }
        } else if !args.quiet {
            for (date, narration, err) in &interpolation_errors {
                writeln!(stdout, "error[INTERP]: {err} ({date}, \"{narration}\")")?;
                writeln!(stdout)?;
            }
        }
    }
    error_count += interpolation_errors.len();

    // Validate the directives
    if args.verbose && !args.quiet {
        eprintln!("Validating {} directives...", directives.len());
    }

    let validation_errors = validate(&directives);
    let validation_error_count = validation_errors
        .iter()
        .filter(|e| !e.code.is_warning())
        .count();
    let validation_warning_count = validation_errors
        .iter()
        .filter(|e| e.code.is_warning())
        .count();
    error_count += validation_error_count;

    if !validation_errors.is_empty() {
        if json_mode {
            for err in &validation_errors {
                let severity = if err.code.is_warning() {
                    "warning"
                } else {
                    "error"
                };
                diagnostics.push(JsonDiagnostic {
                    file: main_file_str.clone(),
                    line: 1, // Validation errors don't have precise locations yet
                    column: 1,
                    end_line: 1,
                    end_column: 1,
                    severity: severity.to_string(),
                    code: err.code.code().to_string(),
                    message: err.message.clone(),
                    hint: None,
                    context: Some(format!("{}", err.date)),
                });
            }
        } else if !args.quiet {
            report::report_validation_errors(&validation_errors, &cache, &mut stdout)?;
        }
    }

    // Print summary / output
    let elapsed = start.elapsed();
    let warning_count = option_warning_count + validation_warning_count;

    if json_mode {
        let output = JsonOutput {
            diagnostics,
            error_count,
            warning_count,
        };
        writeln!(stdout, "{}", serde_json::to_string_pretty(&output)?)?;
    } else if !args.quiet {
        if args.verbose {
            let cache_note = if from_cache { " (from cache)" } else { "" };
            writeln!(
                stdout,
                "\nChecked in {:.2}ms{}",
                elapsed.as_secs_f64() * 1000.0,
                cache_note
            )?;
        }
        report::print_summary(error_count, warning_count, &mut stdout)?;
    }

    if error_count > 0 {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

/// Main entry point for the check command.
pub fn main() -> ExitCode {
    main_with_name("rledger-check")
}

/// Main entry point with custom binary name (for bean-check compatibility).
pub fn main_with_name(bin_name: &str) -> ExitCode {
    let args = Args::parse();

    // Handle shell completion generation
    if let Some(shell) = args.generate_completions {
        crate::cmd::completions::generate_completions::<Args>(shell, bin_name);
        return ExitCode::SUCCESS;
    }

    if args.verbose {
        tracing_subscriber::fmt()
            .with_max_level(Level::DEBUG)
            .with_span_events(FmtSpan::CLOSE)
            .init();
    }

    match run(&args) {
        Ok(exit_code) => exit_code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
